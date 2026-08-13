//! Selective forgetting — user-directed memory erasure.
//!
//! The user-controlled counterpart to lifecycle_cleanup (automatic forgetting).
//! When the user asks the pet to forget something ("忘掉我说的那件事",
//! "forget what I told you about X"), we find the best-matching episode via the
//! retrieval pipeline and delete it together with its embedding vector.
//!
//! Architecture Principle #1: Rust decides WHAT to delete (and refuses when
//! uncertain or for landmarks); the LLM only classified the intent (gate) and
//! later acknowledges the erasure (converse). Never silently deletes the wrong
//! memory — a low-confidence match declines honestly ("我好像不记得这件事呢")
//! rather than risk erasing the wrong episode.

use crate::db::{DbState, episodes as db_episodes, facts as db_facts, pending as db_pending,
                vectors as db_vectors};
use crate::embedding::{cosine_similarity, EmbeddingService};
use crate::emotion::state::EmotionState;
use crate::mind::retrieval::{char_overlap, retrieve, ScoredEpisode};
use std::cmp::Ordering;

/// Confidence required on the SEMANTIC score component before a memory is
/// deleted. The retrieval TOTAL score blends in memory strength / recency /
/// emotion, so a strong recent UNRELATED memory can score highly overall —
/// thresholding on the total would delete the wrong thing. The semantic
/// component (0..1 content relevance) is the true match signal, so we gate on
/// it. 0.7 keeps unrelated memories safe (embedding cosine maps unrelated text
/// to ~0.5; keyword fallback gives 0 for no overlap). Tune with real data.
/// (Architecture Principle #11: explainable threshold.)
const FORGET_CONFIDENCE: f64 = 0.7;

/// When ≥2 candidates clear the gate, only ask back if the top two are
/// genuinely close in confidence. If the best match clearly outranks the
/// runner-up (gap ≥ this), "忘掉火锅" plainly means the high-confidence
/// "喜欢火锅" fact, not the lower-scoring "吃火锅经历" episode — asking back
/// about the latter feels over-cautious. Below the gap, both are plausible and
/// she asks which (using full summaries, never a bare keyword). (#1 trade-off:
/// a large gap means the runner-up is unlikely to be the intended target, so
/// skipping the ask-back is safe; a small gap is true ambiguity.)
const AMBIGUITY_GAP: f64 = 0.15;

/// Outcome of a forget request. Three states instead of a bool+Option: she
/// erased one, honestly declined (no confident match — never deletes the wrong
/// thing), or needs to ask which of several matches the user means (multi-turn
/// disambiguation; the caller stores the candidates and resolves the reply).
#[derive(Debug, Clone)]
pub enum ForgetOutcome {
    /// A memory was erased. Carries its summary so converse can let her
    /// acknowledge naturally (she must not repeat it verbatim).
    Deleted { summary: String },
    /// No confident match — she honestly says she doesn't remember it
    /// (Architecture Principle #1: never delete the wrong thing).
    Declined,
    /// Two or more memories matched; nothing is deleted. The summaries are
    /// surfaced so she can ask "which one?", and the candidates are stored
    /// cross-turn to resolve the user's reply.
    Ambiguous { candidates: Vec<ForgetCandidate> },
}

/// A pending disambiguation stored across turns (mirrors `last_proactive_bubble`
/// in AppState). She asked back; the second turn ("第一个" / "那次经历") is
/// resolved via `resolve_candidate` to one candidate, which is then erased.
#[derive(Debug, Clone)]
pub struct PendingForget {
    /// The original request ("忘掉咖啡"), for logging.
    pub query: String,
    /// The ≥2 candidates, in the order she'll cite them ("第一个" → index 0).
    pub candidates: Vec<ForgetCandidate>,
    /// When the disambiguation started; expiry backstop so a stale slot from an
    /// abandoned chat doesn't hijack a later one.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Pure decision: should this episode be forgotten given its semantic match?
/// Declines landmarks (formative memories are protected even if asked) and
/// low-confidence matches (honest "I don't remember" beats deleting the wrong
/// thing). Extracted for unit testing without a DB.
pub fn should_forget(semantic_score: f64, is_landmark: bool) -> bool {
    !is_landmark && semantic_score >= FORGET_CONFIDENCE
}

/// Execute the forget on the best-matching episode (if any): apply the
/// confidence gate, then delete the episode row + its embedding vector.
/// Returns Deleted (with summary) or Declined.
fn execute_forget(top: Option<&ScoredEpisode>, db: &DbState) -> ForgetOutcome {
    let Some(scored) = top else {
        return ForgetOutcome::Declined;
    };
    if !should_forget(scored.score_breakdown.semantic, scored.episode.is_landmark) {
        return ForgetOutcome::Declined;
    }
    let id = scored.episode.id.clone();
    let summary = scored.episode.summary.clone();
    match db.with_conn(|conn| crate::db::episodes::delete(conn, &id)) {
        Ok(true) => {
            // Best-effort vector cleanup; a missing vector row is harmless.
            let _ = db.with_conn(|conn| crate::db::vectors::delete(conn, &id));
            log::info!(
                "[forget] deleted episode {} ({})",
                id,
                summary.chars().take(40).collect::<String>()
            );
            ForgetOutcome::Deleted { summary }
        }
        _ => ForgetOutcome::Declined,
    }
}

/// Episode-only forget: retrieve the best-matching episode for the user's text
/// and delete it if there is a confident, non-landmark match. Retrieval's emotion
/// weighting uses the neutral baseline — forget matching is content-driven and
/// the emotion weight in retrieval is small (Architecture Principle #9: MVP).
///
/// This is the narrow single-type entry point. Production (`mod::ingest`) calls
/// `forget_best_match`, which scans episodes AND facts AND pending reminders and
/// forgets the single best confident match — the user rarely says which *kind*
/// of memory to forget. Kept as a legitimate narrower API; its `execute_forget`
/// tests pin the episode-level confidence/landmark gate behavior.
pub fn forget_episode(
    text: &str,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<ForgetOutcome, String> {
    let emotion = crate::emotion::state::EmotionState::default();
    // top_k = 1: MVP deletes only the single best match. Multiple matches is a
    // follow-up (raises over-delete risk; user can repeat the request).
    let retrieval = retrieve(text, &emotion, embedding, db, 1)?;
    Ok(execute_forget(retrieval.episodes.first(), db))
}

// ===========================================================================
// Cross-type forgetting (fact / pending extension of the episode MVP).
//
// The user does not say which *kind* of memory to forget — "忘掉咖啡" could be
// a preference (fact), a reminder (pending event), or an event (episode). So
// we scan all three, gate each on its own confidence metric, and forget the
// single best confident match. If none clears the bar, she honestly declines
// (Architecture Principle #1: Rust decides what to erase; #9: MVP deletes only
// the single best match — over-delete risk stays low).
//
// Confidence metrics differ by type because the data differs:
//   - Episode: semantic score from `retrieve` (embedding cosine, or keyword
//     Jaccard when no model). Landmark-protected.
//   - Fact / Pending: `char_overlap` (overlap coefficient on char-bigrams,
//     i.e. shared bigrams over the smaller set) — they have no embedding
//     vectors. The same 0.7 gate reads naturally as "≥70% sure this is the
//     target".
// When two types both qualify, the higher confidence wins. This tends to
// favor facts (soft expire, recoverable) over episodes (hard delete) on
// ambiguity — the safer default.
// ===========================================================================

/// Which kind of memory a forget candidate is. Used for logging + dispatching
/// the correct erase action (delete / expire / resolve).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgetTarget {
    Episode,
    Fact,
    Pending,
}

impl ForgetTarget {
    fn as_str(self) -> &'static str {
        match self {
            ForgetTarget::Episode => "episode",
            ForgetTarget::Fact => "fact",
            ForgetTarget::Pending => "pending",
        }
    }
}

/// A memory the forget request might target, with its match confidence. Only
/// candidates that already clear the confidence gate are produced, so the
/// dispatcher can pick the max without re-checking thresholds.
#[derive(Debug, Clone)]
pub struct ForgetCandidate {
    pub target: ForgetTarget,
    pub id: String,
    pub summary: String,
    pub confidence: f64,
}

/// Best episode match, or None if the top episode is a landmark or below the
/// confidence gate. (Episodes are landmark-protected; facts/pending are not.)
/// `retrieve` is a pure read (no strength/recall_count side-effect) — forgetting
/// one memory type never strengthens another. (ADR 2026-08-09 Part 2: only
/// genuine conversational/proactive recall reinforces.)
fn find_episode_candidate(
    text: &str,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<Option<ForgetCandidate>, String> {
    let emotion = EmotionState::default();
    let retrieval = retrieve(text, &emotion, embedding, db, 1)?;
    let Some(scored) = retrieval.episodes.first() else {
        return Ok(None);
    };
    if !should_forget(scored.score_breakdown.semantic, scored.episode.is_landmark) {
        return Ok(None);
    }
    Ok(Some(ForgetCandidate {
        target: ForgetTarget::Episode,
        id: scored.episode.id.clone(),
        summary: scored.episode.summary.clone(),
        confidence: scored.score_breakdown.semantic,
    }))
}

/// Best active fact match by containment ratio, or None below the gate.
/// Matches against the fact `value` (the content, e.g. "咖啡"); the `key`
/// ("drink") is usually an English attribute label that won't match a Chinese
/// request.
fn find_fact_candidate(
    text: &str,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<Option<ForgetCandidate>, String> {
    let facts = db.with_conn(|c| db_facts::get_all_active(c, 100))?;
    if facts.is_empty() {
        return Ok(None);
    }
    let mut values: Vec<String> = facts.iter().map(|f| f.value.clone()).collect();
    let mut scores: Vec<f64> = values.iter().map(|v| char_overlap(text, v)).collect();
    // Semantic re-rank when the model is ready: lets "忘掉早睡的事" match a fact
    // stored as "想早睡，总是熬夜" (no char overlap, same meaning). Facts have no
    // stored vectors, so we embed on the fly (forget is rare). On any embedding
    // hiccup, scores stay as char_overlap (Architecture Principle #6).
    if let Some(emb) = embedding {
        if emb.is_ready() {
            semantic_rerank(text, &mut values, &mut scores, emb);
        }
    }
    let best_idx = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
        .map(|(i, _)| i);
    match best_idx {
        Some(i) if scores[i] >= FORGET_CONFIDENCE => {
            let f = &facts[i];
            Ok(Some(ForgetCandidate {
                target: ForgetTarget::Fact,
                id: f.id.clone(),
                summary: f.value.clone(),
                confidence: scores[i],
            }))
        }
        _ => Ok(None),
    }
}

/// Best pending (status='pending') reminder match by containment ratio on its
/// title, or None below the gate. Triggered/resolved reminders are already
//  done and are not matchable.
fn find_pending_candidate(
    text: &str,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<Option<ForgetCandidate>, String> {
    let pending = db.with_conn(|c| db_pending::get_all_pending(c))?;
    if pending.is_empty() {
        return Ok(None);
    }
    let mut values: Vec<String> = pending.iter().map(|p| p.title.clone()).collect();
    let mut scores: Vec<f64> = values.iter().map(|v| char_overlap(text, v)).collect();
    if let Some(emb) = embedding {
        if emb.is_ready() {
            semantic_rerank(text, &mut values, &mut scores, emb);
        }
    }
    let best_idx = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
        .map(|(i, _)| i);
    match best_idx {
        Some(i) if scores[i] >= FORGET_CONFIDENCE => {
            let p = &pending[i];
            Ok(Some(ForgetCandidate {
                target: ForgetTarget::Pending,
                id: p.id.clone(),
                summary: p.title.clone(),
                confidence: scores[i],
            }))
        }
        _ => Ok(None),
    }
}

/// Execute the erase for a given candidate. Episode → hard delete (+ vector
/// cleanup). Fact → soft expire (valid_to, preserves the revive/audit trail).
/// Pending → resolve (status-lifecycle terminal; stops firing). Returns whether
/// the memory was actually changed.
pub fn execute_candidate(cand: &ForgetCandidate, db: &DbState) -> bool {
    let now = chrono::Utc::now().to_rfc3339();
    match cand.target {
        ForgetTarget::Episode => match db.with_conn(|c| db_episodes::delete(c, &cand.id)) {
            Ok(true) => {
                let _ = db.with_conn(|c| db_vectors::delete(c, &cand.id));
                true
            }
            _ => false,
        },
        ForgetTarget::Fact => db
            .with_conn(|c| db_facts::expire_by_id(c, &cand.id, &now))
            .ok()
            .unwrap_or(false),
        ForgetTarget::Pending => db
            .with_conn(|c| db_pending::mark_resolved(c, &cand.id, &now))
            .is_ok(),
    }
}

/// Top-level forget: scan episodes, facts, and pending reminders for the best
/// confident match to the user's text and erase that one memory. Replaces
/// `forget_episode` as the production entry point (episodes are still handled,
/// just no longer the only target). No confident match → honest "不记得".
pub fn forget_best_match(
    text: &str,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<ForgetOutcome, String> {
    let mut cands: Vec<ForgetCandidate> = Vec::new();
    if let Some(c) = find_episode_candidate(text, db, embedding)? {
        cands.push(c);
    }
    if let Some(c) = find_fact_candidate(text, db, embedding)? {
        cands.push(c);
    }
    if let Some(c) = find_pending_candidate(text, db, embedding)? {
        cands.push(c);
    }

    // No candidate cleared the gate → honestly decline (never delete the wrong
    // thing on a weak match).
    if cands.is_empty() {
        log::info!(
            "[forget] no confident match for {:?}",
            text.chars().take(40).collect::<String>()
        );
        return Ok(ForgetOutcome::Declined);
    }

    // Decide winner vs ambiguity. Don't always ask back when ≥2 candidates
    // clear the gate: if the best match clearly outranks the runner-up
    // (confidence gap ≥ AMBIGUITY_GAP), the request plainly means the top one
    // (e.g. "忘掉火锅" → the "喜欢火锅" fact, not the lower-scoring "吃火锅
    // 经历" episode). Only when the top two are genuinely close do we surface
    // candidates for an ask-back. Landmarks are already filtered out of the
    // episode leg, so every candidate is safe to erase once picked.
    match pick_winner_or_ambiguous(cands) {
        Pick::Ambiguous(cands) => {
            log::info!(
                "[forget] {} candidates matched {:?} — asking back",
                cands.len(),
                text.chars().take(40).collect::<String>()
            );
            Ok(ForgetOutcome::Ambiguous { candidates: cands })
        }
        Pick::Winner(w) => {
            let summary = w.summary.clone();
            let target = w.target;
            if execute_candidate(&w, db) {
                log::info!(
                    "[forget] {} {} ({})",
                    target.as_str(),
                    w.id,
                    summary.chars().take(40).collect::<String>()
                );
                Ok(ForgetOutcome::Deleted { summary })
            } else {
                Ok(ForgetOutcome::Declined)
            }
        }
    }
}

/// Result of deciding among ≥0 forget candidates: either one clear target to
/// erase, or a genuinely-ambiguous set to ask back about. Pure function over
/// candidates alone (no DB/embedding) so it is unit-testable with synthetic
/// candidates.
enum Pick {
    Winner(ForgetCandidate),
    Ambiguous(Vec<ForgetCandidate>),
}

/// Given the candidates that cleared the gate, decide whether there is a clear
/// winner (erase it) or a genuine ambiguity (ask back).
///
/// - 0 candidates → never called (caller handles Declined first).
/// - 1 candidate → Winner.
/// - ≥2 candidates sorted by confidence descending: if the top two have the
///   IDENTICAL summary and NEITHER is a Pending reminder, they are the same
///   memory stored twice (extractor put a "喜欢篮球" fact AND a "喜欢篮球"
///   episode for one message) — asking back "你是说喜欢篮球还是喜欢篮球" is
///   absurd; collapse to the higher one. A Pending reminder is a separate
///   transaction even with an identical title (fact "喝咖啡"偏好 vs pending
///   "喝咖啡"提醒 are different intents), so fact+pending stays ambiguous.
/// - Otherwise: if the top two are within AMBIGUITY_GAP of each other they are
///   both plausible → Ambiguous; if the top outranks the runner-up by ≥
///   AMBIGUITY_GAP the request plainly means the top one → Winner (asking back
///   about the runner-up feels over-cautious).
fn pick_winner_or_ambiguous(mut cands: Vec<ForgetCandidate>) -> Pick {
    if cands.len() <= 1 {
        return Pick::Winner(cands.pop().unwrap_or_else(|| ForgetCandidate {
            target: ForgetTarget::Fact,
            id: String::new(),
            summary: String::new(),
            confidence: 0.0,
        }));
    }
    cands.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(Ordering::Equal));
    // Identical summaries between non-pending memories = the same thing stored
    // twice; take the higher-conf one without asking. (Cheap: candidates are
    // few.)
    if cands[0].summary == cands[1].summary
        && cands[0].target != ForgetTarget::Pending
        && cands[1].target != ForgetTarget::Pending
    {
        cands.truncate(1);
        return Pick::Winner(cands.pop().unwrap());
    }
    let gap = cands[0].confidence - cands[1].confidence;
    if gap < AMBIGUITY_GAP {
        Pick::Ambiguous(cands)
    } else {
        cands.truncate(1);
        Pick::Winner(cands.pop().unwrap())
    }
}

/// Re-rank candidate strings by embedding cosine similarity to the query,
/// mutating `scores` in place. Only the char-overlap top-K (K=5) are
/// re-scored, bounding embedding cost (forget is rare; a few embeddings is
/// fine). Maps cosine (-1..1) to 0..1 the same way `retrieval::compute_semantic`
/// does, so the 0.7 gate reads identically. On any embedding hiccup the scores
/// are left as the char_overlap values already in place (Architecture Principle
/// #6: graceful degradation when the model is unavailable).
fn semantic_rerank(
    text: &str,
    values: &[String],
    scores: &mut [f64],
    embedding: &EmbeddingService,
) {
    let n = values.len();
    if n == 0 {
        return;
    }
    // Only re-rank entries that already share at least one char-bigram with the
    // query (char_overlap > 0). A zero-overlap entry keeps score 0: BGE-M3's
    // unrelated baseline (~0.5 raw cosine → 0.75 via the (cos+1)/2 map) would
    // otherwise clear the 0.7 delete gate and FABRICATE candidates — e.g.
    // "忘掉火锅" falsely surfacing the 早睡 fact. The shared-char anchor lets the
    // boost catch genuine near-synonyms ("忘掉早睡的事" → "想早睡总是熬夜", they
    // share 早睡) without admitting unrelated memories. (Architecture #1: never
    // delete the wrong thing; #11: explainable threshold.)
    let mut idx: Vec<usize> = (0..n).filter(|&i| scores[i] > 0.0).collect();
    if idx.is_empty() {
        return;
    }
    idx.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(Ordering::Equal));
    let top: Vec<usize> = idx.into_iter().take(n.min(5)).collect();
    let qv = match embedding.embed(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    let vals: Vec<String> = top.iter().map(|&i| values[i].clone()).collect();
    let vecs = match embedding.embed_batch(&vals) {
        Ok(v) => v,
        Err(_) => return,
    };
    for (rank, &i) in top.iter().enumerate() {
        if let Some(vv) = vecs.get(rank) {
            let cos = cosine_similarity(&qv, vv) as f64;
            scores[i] = ((cos + 1.0) / 2.0).clamp(0.0, 1.0);
        }
    }
}

/// Resolve a second-turn disambiguation reply to one of the candidates by
/// 0-based index, or None if it can't be resolved. Ordinal phrases map
/// directly ("第一个"→0, "后者"→1, "B"→1); otherwise the best char_overlap
/// against a candidate summary clears a loose 0.4 bar (the user is now
/// explicitly disambiguating and likely repeats a distinguishing keyword).
/// Pure + unit-testable (no DB, no model).
pub fn resolve_candidate(text: &str, candidates: &[ForgetCandidate]) -> Option<usize> {
    let n = candidates.len();
    if n == 0 {
        return None;
    }
    if let Some(i) = ordinal_index(text, n) {
        return Some(i);
    }
    let mut best: Option<(usize, f64)> = None;
    for (i, c) in candidates.iter().enumerate() {
        let conf = char_overlap(text, &c.summary);
        match best {
            Some((_, b)) if conf <= b => {}
            _ => best = Some((i, conf)),
        }
    }
    match best {
        Some((i, conf)) if conf >= 0.4 => Some(i),
        _ => None,
    }
}

/// Did the user abandon the disambiguation for a new topic? True when there's
/// no ordinal cue and char_overlap is low against every candidate. Conservative:
/// anything plausibly still on-topic stays in the loop (re-asked once), so the
/// slot is cleared only on a clearly new subject.
pub fn is_off_topic(text: &str, candidates: &[ForgetCandidate]) -> bool {
    if candidates.is_empty() {
        return true;
    }
    if ordinal_index(text, candidates.len()).is_some() {
        return false;
    }
    candidates.iter().all(|c| char_overlap(text, &c.summary) < 0.2)
}

/// Map an ordinal phrase in `text` to a 0-based index (< n), or None. Handles
/// CJK + ascii digits after 第 (第一个/第2个), 前者/后者, 最后, and bare single
/// tokens (1/A/甲…). Conservative: only clear ordinals resolve.
fn ordinal_index(text: &str, n: usize) -> Option<usize> {
    let t = text.trim();
    if t.is_empty() || n == 0 {
        return None;
    }
    let last = n - 1;
    if t.contains("最后") {
        return Some(last);
    }
    if t.contains("前者") || t.contains("头一个") {
        return Some(0);
    }
    if t.contains("后者") {
        return Some(last.min(1));
    }
    // Digit (ascii or CJK numeral) immediately after 第.
    if let Some(d) = t
        .find('第')
        .and_then(|p| t[p + '第'.len_utf8()..].chars().next())
        .and_then(cjk_to_digit)
    {
        if (1..=n).contains(&d) {
            return Some(d - 1);
        }
    }
    // Bare single token: "1", "2", "A", "B", "甲", "乙".
    if t.chars().count() == 1 {
        if let Some(d) = t.chars().next().and_then(|c| c.to_digit(10)) {
            if (1..=n as u32).contains(&d) {
                return Some(d as usize - 1);
            }
        }
        let idx = match t {
            "A" | "a" | "甲" => Some(0),
            "B" | "b" | "乙" => Some(last.min(1)),
            _ => None,
        };
        if let Some(i) = idx {
            return Some(i);
        }
    }
    None
}

/// Map the char after 第 to 1..=10 (ascii digit or CJK numeral), else None.
fn cjk_to_digit(c: char) -> Option<usize> {
    c.to_digit(10)
        .map(|d| d as usize)
        .or_else(|| match c {
            '一' => Some(1),
            '二' | '兩' | '两' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            '十' => Some(10),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::episodes::Episode;
    use crate::db::test_utils::test_db;
    use crate::mind::retrieval::ScoreBreakdown;

    #[test]
    fn should_forget_clear_match() {
        assert!(should_forget(0.85, false));
        assert!(should_forget(FORGET_CONFIDENCE, false)); // boundary inclusive
    }

    #[test]
    fn should_forget_low_confidence_declines() {
        assert!(!should_forget(0.69, false));
        assert!(!should_forget(0.0, false));
        assert!(!should_forget(0.5, false)); // embedding "unrelated" baseline
    }

    #[test]
    fn should_forget_protects_landmark_even_if_confident() {
        // A landmark is never deleted, no matter how strong the match.
        assert!(!should_forget(0.99, true));
    }

    /// Build a minimal episode for tests.
    fn ep(id: &str, summary: &str, landmark: bool) -> Episode {
        Episode {
            id: id.to_string(),
            time: "2026-08-04T00:00:00Z".to_string(),
            summary: summary.to_string(),
            emotion: None,
            importance: 0.5,
            is_landmark: landmark,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: 0.5,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: "2026-08-04T00:00:00Z".to_string(),
        }
    }

    fn scored(ep: Episode, semantic: f64) -> ScoredEpisode {
        ScoredEpisode {
            episode: ep,
            score: semantic, // total; unused by execute_forget (it reads breakdown)
            score_breakdown:                 ScoreBreakdown {
                semantic,
                strength: 0.0,
                novelty: 0.0,
                recency: 0.0,
                emotion: 0.0,
            },
        }
    }

    #[test]
    fn execute_forget_deletes_confident_match() {
        let db = test_db();
        db.with_conn(|c| crate::db::episodes::insert(c, &ep("ep1", "看了星际穿越", false)))
            .unwrap();
        let before = db.with_conn(|c| crate::db::episodes::get(c, "ep1")).unwrap();
        assert!(before.is_some());

        let result = execute_forget(Some(&scored(ep("ep1", "看了星际穿越", false), 0.9)), &db);

        let summary = match result {
            ForgetOutcome::Deleted { summary } => summary,
            other => panic!("expected Deleted, got {:?}", other),
        };
        assert_eq!(summary, "看了星际穿越");
        let after = db.with_conn(|c| crate::db::episodes::get(c, "ep1")).unwrap();
        assert!(after.is_none(), "episode should be gone after forget");
    }

    #[test]
    fn execute_forget_refuses_landmark() {
        let db = test_db();
        db.with_conn(|c| crate::db::episodes::insert(c, &ep("lm1", "毕业典礼", true)))
            .unwrap();
        // Even a perfect semantic match on a landmark does not delete it.
        let result = execute_forget(Some(&scored(ep("lm1", "毕业典礼", true), 0.95)), &db);
        assert!(matches!(result, ForgetOutcome::Declined));
        let still = db.with_conn(|c| crate::db::episodes::get(c, "lm1")).unwrap();
        assert!(still.is_some(), "landmark must survive a forget request");
    }

    #[test]
    fn execute_forget_refuses_low_confidence() {
        let db = test_db();
        db.with_conn(|c| crate::db::episodes::insert(c, &ep("ep2", "吃了火锅", false)))
            .unwrap();
        let result = execute_forget(Some(&scored(ep("ep2", "吃了火锅", false), 0.4)), &db);
        assert!(matches!(result, ForgetOutcome::Declined));
        let still = db.with_conn(|c| crate::db::episodes::get(c, "ep2")).unwrap();
        assert!(still.is_some(), "low-confidence match must not be deleted");
    }

    #[test]
    fn execute_forget_no_candidates() {
        let db = test_db();
        let result = execute_forget(None, &db);
        assert!(matches!(result, ForgetOutcome::Declined));
    }

    // --- cross-type dispatcher (fact / pending) ---

    use rusqlite::params;

    /// Insert a preference-style fact directly (bypassing dedup so tests can
    /// place exact rows). Category/key fixed to "preference"/"drink". Returns
    /// `Result` so it can be used inside `db.with_conn` closures.
    fn insert_fact(conn: &rusqlite::Connection, id: &str, value: &str) -> Result<(), String> {
        conn.execute(
            "INSERT INTO facts (id, category, key, value, confidence, valid_from, valid_to,
                source_episode, mention_count, created_at, updated_at)
             VALUES (?1, 'preference', 'drink', ?2, 0.8, '2026-07-14', NULL, NULL, 1,
                '2026-07-14', '2026-07-14')",
            params![id, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn insert_pending(conn: &rusqlite::Connection, id: &str, title: &str) -> Result<(), String> {
        conn.execute(
            "INSERT INTO pending_events (id, title, event_date, remind_date, source_episode,
                status, importance, followup_count, created_at, triggered_at, resolved_at)
             VALUES (?1, ?2, '2026-08-10', '2026-08-10T08:00:00', NULL, 'pending', 0.8, 0,
                '2026-08-05', NULL, NULL)",
            params![id, title],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Is a fact with this id still active (valid_to IS NULL)?
    fn fact_active(conn: &rusqlite::Connection, id: &str) -> bool {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM facts WHERE id = ?1 AND valid_to IS NULL",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        n > 0
    }

    /// Is a pending event still pending (status='pending')?
    fn pending_pending(conn: &rusqlite::Connection, id: &str) -> bool {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM pending_events WHERE id = ?1 AND status = 'pending'",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        n > 0
    }

    #[test]
    fn forget_best_match_expires_fact() {
        let db = test_db();
        // No episodes inserted → episode leg returns None, isolating the fact leg.
        db.with_conn(|c| insert_fact(c, "f_coffee", "咖啡")).ok();
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap());

        let result = forget_best_match("忘掉咖啡", &db, None).unwrap();
        let summary = match result {
            ForgetOutcome::Deleted { summary } => summary,
            other => panic!("expected Deleted, got {:?}", other),
        };
        assert_eq!(summary, "咖啡");
        assert!(
            !db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap(),
            "fact should now be expired (valid_to set)"
        );
    }

    #[test]
    fn forget_best_match_resolves_pending() {
        let db = test_db();
        db.with_conn(|c| insert_pending(c, "pe_interview", "面试提醒")).ok();
        assert!(db.with_conn(|c| Ok(pending_pending(c, "pe_interview"))).unwrap());

        let result = forget_best_match("忘掉面试提醒", &db, None).unwrap();
        assert!(
            matches!(result, ForgetOutcome::Deleted { .. }),
            "pending reminder should be forgotten"
        );
        assert!(
            !db.with_conn(|c| Ok(pending_pending(c, "pe_interview"))).unwrap(),
            "reminder should be resolved, not pending"
        );
    }

    #[test]
    fn forget_best_match_no_match_declines() {
        let db = test_db();
        db.with_conn(|c| insert_fact(c, "f_coffee", "咖啡")).ok();
        db.with_conn(|c| insert_pending(c, "pe_interview", "面试")).ok();

        // Totally unrelated request → nothing clears 0.7.
        let result = forget_best_match("忘掉那个不存在的事", &db, None).unwrap();
        assert!(matches!(result, ForgetOutcome::Declined), "should decline honestly");
        // Memories untouched.
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap());
        assert!(db.with_conn(|c| Ok(pending_pending(c, "pe_interview"))).unwrap());
    }

    #[test]
    fn find_fact_candidate_below_gate_is_none() {
        let db = test_db();
        db.with_conn(|c| insert_fact(c, "f_coffee", "咖啡")).ok();
        // "茶叶" shares no bigram with "咖啡" → char_overlap 0 → no candidate.
        let cand = find_fact_candidate("忘掉茶叶", &db, None).unwrap();
        assert!(cand.is_none(), "low-overlap fact must not be a forget candidate");
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap(), "fact untouched");
    }

    // --- multi-candidate disambiguation (Ambiguous) ---

    #[test]
    fn forget_best_match_ambiguous_keeps_both() {
        let db = test_db();
        // "喝咖啡" matches a fact AND a pending reminder (both char_overlap 1.0)
        // → ≥2 candidates → Ambiguous, nothing deleted.
        db.with_conn(|c| insert_fact(c, "f_coffee", "喝咖啡")).ok();
        db.with_conn(|c| insert_pending(c, "pe_coffee", "喝咖啡")).ok();

        let result = forget_best_match("忘掉喝咖啡", &db, None).unwrap();
        let cands = match result {
            ForgetOutcome::Ambiguous { candidates } => candidates,
            other => panic!("expected Ambiguous, got {:?}", other),
        };
        assert_eq!(cands.len(), 2, "both matches surfaced, nothing deleted");
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap());
        assert!(db.with_conn(|c| Ok(pending_pending(c, "pe_coffee"))).unwrap());
    }

    // --- second-turn resolution (resolve_candidate / is_off_topic) ---

    fn cand(target: ForgetTarget, summary: &str) -> ForgetCandidate {
        ForgetCandidate {
            target,
            id: "x".to_string(),
            summary: summary.to_string(),
            confidence: 0.9,
        }
    }

    fn cand_c(target: ForgetTarget, summary: &str, confidence: f64) -> ForgetCandidate {
        ForgetCandidate {
            target,
            id: summary.to_string(),
            summary: summary.to_string(),
            confidence,
        }
    }

    // --- gap-based ambiguity (pick_winner_or_ambiguous) ---

    #[test]
    fn pick_single_candidate_is_winner() {
        // One candidate → erase it directly, no ask-back.
        let cs = vec![cand_c(ForgetTarget::Fact, "喜欢火锅", 0.9)];
        match pick_winner_or_ambiguous(cs) {
            Pick::Winner(w) => assert_eq!(w.summary, "喜欢火锅"),
            Pick::Ambiguous(_) => panic!("single candidate should be a Winner"),
        }
    }

    #[test]
    fn pick_clear_winner_no_ask_back() {
        // "忘掉火锅" matches a fact "喜欢火锅" (0.92) and an episode "吃火锅经历"
        // (0.70). Gap 0.22 ≥ AMBIGUITY_GAP → the fact plainly wins, erase it
        // without asking. (The user's feedback: an interest + an experience where
        // semantics already point at the interest shouldn't trigger a question.)
        let cs = vec![
            cand_c(ForgetTarget::Episode, "吃火锅经历", 0.70),
            cand_c(ForgetTarget::Fact, "喜欢火锅", 0.92),
        ];
        match pick_winner_or_ambiguous(cs) {
            Pick::Winner(w) => {
                assert_eq!(w.target, ForgetTarget::Fact);
                assert_eq!(w.summary, "喜欢火锅");
            }
            Pick::Ambiguous(_) => panic!("clear gap should NOT ask back"),
        }
    }

    #[test]
    fn pick_close_candidates_ask_back() {
        // Two candidates within AMBIGUITY_GAP (0.88 vs 0.80, gap 0.08) are both
        // plausible → genuine ambiguity, surface both.
        let cs = vec![
            cand_c(ForgetTarget::Fact, "喝咖啡", 0.88),
            cand_c(ForgetTarget::Pending, "喝咖啡", 0.80),
        ];
        match pick_winner_or_ambiguous(cs) {
            Pick::Ambiguous(cands) => assert_eq!(cands.len(), 2),
            Pick::Winner(_) => panic!("close candidates should ask back"),
        }
    }

    #[test]
    fn pick_identical_summaries_collapse_no_ask() {
        // A "喜欢篮球" fact AND a "喜欢篮球" episode (same summary, close conf)
        // are the same thing stored twice — collapse to the higher one, never
        // ask "你是说喜欢篮球还是喜欢篮球".
        let cs = vec![
            cand_c(ForgetTarget::Fact, "喜欢篮球", 0.85),
            cand_c(ForgetTarget::Episode, "喜欢篮球", 0.80),
        ];
        match pick_winner_or_ambiguous(cs) {
            Pick::Winner(w) => {
                assert_eq!(w.target, ForgetTarget::Fact);
                assert_eq!(w.confidence, 0.85);
            }
            Pick::Ambiguous(_) => panic!("identical summaries must collapse, not ask"),
        }
    }

    #[test]
    fn resolve_candidate_ordinals() {
        let cs = vec![
            cand(ForgetTarget::Fact, "咖啡偏好"),
            cand(ForgetTarget::Episode, "和糯米喝咖啡"),
        ];
        assert_eq!(resolve_candidate("第一个", &cs), Some(0));
        assert_eq!(resolve_candidate("第二個", &cs), Some(1));
        assert_eq!(resolve_candidate("前者", &cs), Some(0));
        assert_eq!(resolve_candidate("后者", &cs), Some(1));
        assert_eq!(resolve_candidate("B", &cs), Some(1));
        assert_eq!(resolve_candidate("2", &cs), Some(1));
    }

    #[test]
    fn resolve_candidate_keyword_overlap() {
        let cs = vec![
            cand(ForgetTarget::Fact, "咖啡偏好"),
            cand(ForgetTarget::Episode, "和糯米喝咖啡"),
        ];
        // No ordinal, but the reply overlaps the episode summary → index 1.
        assert_eq!(resolve_candidate("和糯米喝咖啡那次", &cs), Some(1));
    }

    #[test]
    fn resolve_candidate_unresolvable_is_none() {
        let cs = vec![cand(ForgetTarget::Fact, "咖啡"), cand(ForgetTarget::Episode, "看猫")];
        assert_eq!(resolve_candidate("嗯就是那个", &cs), None);
    }

    #[test]
    fn resolve_candidate_out_of_range_ordinal_is_none() {
        let cs = vec![cand(ForgetTarget::Fact, "咖啡")]; // n=1
        assert_eq!(resolve_candidate("第二个", &cs), None);
    }

    #[test]
    fn is_off_topic_detects_new_subject() {
        let cs = vec![cand(ForgetTarget::Fact, "咖啡"), cand(ForgetTarget::Episode, "看猫")];
        assert!(is_off_topic("今天天气真好", &cs));
        assert!(!is_off_topic("第一个", &cs)); // ordinal cue
        assert!(!is_off_topic("就是咖啡那个", &cs)); // keyword overlap
    }
}
