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
use crate::embedding::EmbeddingService;
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

/// Outcome of a forget request.
#[derive(Debug, Clone)]
pub struct ForgetResult {
    /// Whether a memory was actually deleted.
    pub deleted: bool,
    /// Short summary of the forgotten memory (for the converse system hint),
    /// if one was deleted. Used only to let her acknowledge naturally; she must
    /// NOT repeat it verbatim.
    pub summary: Option<String>,
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
/// Returns whether anything was deleted (and the summary, if so).
fn execute_forget(top: Option<&ScoredEpisode>, db: &DbState) -> ForgetResult {
    let Some(scored) = top else {
        return ForgetResult { deleted: false, summary: None };
    };
    if !should_forget(scored.score_breakdown.semantic, scored.episode.is_landmark) {
        return ForgetResult { deleted: false, summary: None };
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
            ForgetResult { deleted: true, summary: Some(summary) }
        }
        _ => ForgetResult { deleted: false, summary: None },
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
) -> Result<ForgetResult, String> {
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
enum ForgetTarget {
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
struct ForgetCandidate {
    target: ForgetTarget,
    id: String,
    summary: String,
    confidence: f64,
}

/// Best episode match, or None if the top episode is a landmark or below the
/// confidence gate. (Episodes are landmark-protected; facts/pending are not.)
/// Note: `retrieve` reinforces its top result as a retrieval side-effect — if
/// a fact/pending later wins, one episode got +0.03 strength. Negligible and
/// in the favorable direction (recall strengthens); accepted for MVP.
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
fn find_fact_candidate(text: &str, db: &DbState) -> Result<Option<ForgetCandidate>, String> {
    let facts = db.with_conn(|c| db_facts::get_all_active(c, 100))?;
    let best = facts
        .into_iter()
        .map(|f| {
            let conf = char_overlap(text, &f.value);
            (f, conf)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
    match best {
        Some((f, conf)) if conf >= FORGET_CONFIDENCE => Ok(Some(ForgetCandidate {
            target: ForgetTarget::Fact,
            id: f.id,
            summary: f.value,
            confidence: conf,
        })),
        _ => Ok(None),
    }
}

/// Best pending (status='pending') reminder match by containment ratio on its
/// title, or None below the gate. Triggered/resolved reminders are already
//  done and are not matchable.
fn find_pending_candidate(text: &str, db: &DbState) -> Result<Option<ForgetCandidate>, String> {
    let pending = db.with_conn(|c| db_pending::get_all_pending(c))?;
    let best = pending
        .into_iter()
        .map(|p| {
            let conf = char_overlap(text, &p.title);
            (p, conf)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
    match best {
        Some((p, conf)) if conf >= FORGET_CONFIDENCE => Ok(Some(ForgetCandidate {
            target: ForgetTarget::Pending,
            id: p.id,
            summary: p.title,
            confidence: conf,
        })),
        _ => Ok(None),
    }
}

/// Execute the erase for a given candidate. Episode → hard delete (+ vector
/// cleanup). Fact → soft expire (valid_to, preserves the revive/audit trail).
/// Pending → resolve (status-lifecycle terminal; stops firing). Returns whether
/// the memory was actually changed.
fn execute_candidate(cand: &ForgetCandidate, db: &DbState) -> bool {
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
) -> Result<ForgetResult, String> {
    let mut cands: Vec<ForgetCandidate> = Vec::new();
    if let Some(c) = find_episode_candidate(text, db, embedding)? {
        cands.push(c);
    }
    if let Some(c) = find_fact_candidate(text, db)? {
        cands.push(c);
    }
    if let Some(c) = find_pending_candidate(text, db)? {
        cands.push(c);
    }

    let Some(winner) = cands
        .into_iter()
        .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(Ordering::Equal))
    else {
        log::info!(
            "[forget] no confident match for {:?}",
            text.chars().take(40).collect::<String>()
        );
        return Ok(ForgetResult { deleted: false, summary: None });
    };

    let summary = winner.summary.clone();
    let target = winner.target;
    let deleted = execute_candidate(&winner, db);
    if deleted {
        log::info!(
            "[forget] {} {} ({})",
            target.as_str(),
            winner.id,
            summary.chars().take(40).collect::<String>()
        );
    }
    Ok(ForgetResult {
        deleted,
        summary: if deleted { Some(summary) } else { None },
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
            score_breakdown: ScoreBreakdown {
                semantic,
                strength: 0.0,
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

        assert!(result.deleted);
        assert_eq!(result.summary.as_deref(), Some("看了星际穿越"));
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
        assert!(!result.deleted);
        let still = db.with_conn(|c| crate::db::episodes::get(c, "lm1")).unwrap();
        assert!(still.is_some(), "landmark must survive a forget request");
    }

    #[test]
    fn execute_forget_refuses_low_confidence() {
        let db = test_db();
        db.with_conn(|c| crate::db::episodes::insert(c, &ep("ep2", "吃了火锅", false)))
            .unwrap();
        let result = execute_forget(Some(&scored(ep("ep2", "吃了火锅", false), 0.4)), &db);
        assert!(!result.deleted);
        let still = db.with_conn(|c| crate::db::episodes::get(c, "ep2")).unwrap();
        assert!(still.is_some(), "low-confidence match must not be deleted");
    }

    #[test]
    fn execute_forget_no_candidates() {
        let db = test_db();
        let result = execute_forget(None, &db);
        assert!(!result.deleted);
        assert!(result.summary.is_none());
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
        assert!(result.deleted, "fact should be forgotten");
        assert_eq!(result.summary.as_deref(), Some("咖啡"));
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
        assert!(result.deleted, "pending reminder should be forgotten");
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
        assert!(!result.deleted, "should decline honestly");
        assert!(result.summary.is_none());
        // Memories untouched.
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap());
        assert!(db.with_conn(|c| Ok(pending_pending(c, "pe_interview"))).unwrap());
    }

    #[test]
    fn find_fact_candidate_below_gate_is_none() {
        let db = test_db();
        db.with_conn(|c| insert_fact(c, "f_coffee", "咖啡")).ok();
        // "茶叶" shares no bigram with "咖啡" → char_overlap 0 → no candidate.
        let cand = find_fact_candidate("忘掉茶叶", &db).unwrap();
        assert!(cand.is_none(), "low-overlap fact must not be a forget candidate");
        assert!(db.with_conn(|c| Ok(fact_active(c, "f_coffee"))).unwrap(), "fact untouched");
    }
}
