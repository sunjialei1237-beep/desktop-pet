use crate::db::facts as db_facts;
use crate::db::persona as db_persona;
use crate::db::relationship as db_relationship;
use crate::db::episodes as db_episodes;
use crate::db::DbState;
use crate::db::vectors as db_vectors;
use crate::embedding::{cosine_similarity, EmbeddingService};
use crate::emotion::state::EmotionState;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use crate::db::onboarding::UserProfile;

/// An episode paired with its stored embedding vector (if available).
type EpisodeWithVector = (db_episodes::Episode, Option<Vec<f32>>);

/// Weights for the hybrid retrieval score. Novelty (2026-08-13) gives
/// never-recalled memories an exploration bonus so dominant topics can't
/// monopolize the ranking; strength was trimmed to make room for it.
const W_SEMANTIC: f64 = 0.4;
const W_STRENGTH: f64 = 0.2;
const W_NOVELTY: f64 = 0.15;
const W_RECENCY: f64 = 0.15;
const W_EMOTION: f64 = 0.1;

/// Recency half-life in days.
const RECENCY_HALFLIFE_DAYS: f64 = 30.0;

/// Novelty half-life in recall counts: exp(-recall_count / NOVELTY_TAU).
/// 0 recalls → 1.0 (full exploration bonus), 5 → ~0.37, 20 → ~0.02.
const NOVELTY_TAU: f64 = 5.0;

/// Surfacing cooldown: an episode recalled within this many hours is dropped
/// from proactive-anchor sampling (the pet just talked about it).
pub const SURFACE_COOLDOWN_HOURS: i64 = 12;

/// Softmax temperature for the weighted surfacing draw. Lower = still mostly
/// the top memories; higher = flatter, more exploratory. 0.6 keeps the best
/// memories clearly favored while letting others in (diversity fix).
pub const SURFACE_TEMPERATURE: f64 = 0.6;

/// A scored episode from retrieval.
#[derive(Debug, Clone, Serialize)]
pub struct ScoredEpisode {
    pub episode: db_episodes::Episode,
    pub score: f64,
    pub score_breakdown: ScoreBreakdown,
}

/// Individual score components for explainability (architecture principle #11).
#[derive(Debug, Clone, Serialize)]
pub struct ScoreBreakdown {
    pub semantic: f64,
    pub strength: f64,
    pub novelty: f64,
    pub recency: f64,
    pub emotion: f64,
}

/// Full retrieval result.
#[derive(Debug, Clone, Default)]
pub struct RetrievalResult {
    pub episodes: Vec<ScoredEpisode>,
    pub facts: Vec<db_facts::Fact>,
    pub relationship: Option<db_relationship::Relationship>,
    /// Latest relationship-review summary (always-on relationship context).
    /// None until the first background review runs. Injected as [Relationship].
    pub relationship_review: Option<String>,
    pub persona_traits: Vec<db_persona::PersonaTrait>,
    pub user_profile: UserProfile,
}

/// Performs hybrid retrieval: semantic + strength + recency + emotion scoring.
/// Returns top-K episodes plus all active facts and persona snapshot.
///
/// If embedding model is not ready, falls back to keyword-based candidate selection.
pub fn retrieve(
    query: &str,
    emotion: &EmotionState,
    embedding: Option<&EmbeddingService>,
    db: &DbState,
    top_k: usize,
) -> Result<RetrievalResult, String> {
    let now = Utc::now();

    // Generate query embedding if model is available.
    let query_vec = if let Some(emb) = embedding {
        if emb.is_ready() {
            emb.embed(query).ok()
        } else {
            None
        }
    } else {
        None
    };

    // Get candidate episodes from DB.
    let candidates = db.with_conn(|conn| {
        // When a query embedding is available, search the vector store by
        // cosine similarity and load those specific episodes. This replaces
        // the old keyword-only fallback for any conversation where the model
        // is loaded.
        if let Some(qv) = query_vec.as_ref() {
            let hits = db_vectors::search(conn, qv, 50)?;
            if !hits.is_empty() {
                let mut result = Vec::new();
                for (ep_id, _) in hits {
                    if let Some(ep) = db_episodes::get(conn, &ep_id)? {
                        let vec = db_vectors::get(conn, &ep_id)?;
                        result.push((ep, vec));
                    }
                }
                return Ok(result);
            }
        }
        // Fallback: keyword-based candidate selection.
        get_candidate_episodes(conn, 50)
    })?;

    // Score each candidate.
    let mut scored: Vec<ScoredEpisode> = candidates
        .into_iter()
        .map(|(ep, ep_vec)| {
            let semantic = compute_semantic(&query_vec, &ep_vec, query, &ep.summary);
            let strength = ep.memory_strength * W_STRENGTH;
            let novelty = compute_novelty(ep.recall_count) * W_NOVELTY;
            let recency = compute_recency(&ep.time, &now) * W_RECENCY;
            let emotion_score = compute_emotion_match(&ep.emotion, emotion) * W_EMOTION;
            let total = semantic + strength + novelty + recency + emotion_score;

            ScoredEpisode {
                episode: ep,
                score: total,
                score_breakdown: ScoreBreakdown {
                    semantic: semantic / W_SEMANTIC,
                    strength: strength / W_STRENGTH,
                    novelty: novelty / W_NOVELTY,
                    recency: recency / W_RECENCY,
                    emotion: emotion_score / W_EMOTION,
                },
            }
        })
        .collect();

    // Sort by score descending, take top-K.
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    // retrieve() is a PURE READ: it does not reinforce. Reinforcement is a write
    // that belongs to the caller, and only callers representing a genuine recall
    // (a conversational reply, a proactive memory mention) should call
    // `reinforce_top`. Read-only paths (forget lookups, tests, A/B harnesses)
    // must never inflate memory_strength / recall_count. ADR 2026-08-09 Part 2.

    // Retrieve active facts.
    let facts = db.with_conn(get_active_facts)?;

    // Retrieve persona snapshot.
    let relationship = db.with_conn(db_relationship::get).ok();
    let persona_traits = db.with_conn(|conn| {
        Ok(db_persona::get_all_traits(conn)
            .unwrap_or_default())
    })?;

    // Onboarding profile (user-chosen nickname / pet name / personality / relationship).
    let user_profile = db.with_conn(crate::db::onboarding::load)?;

    // Latest relationship review (always-on relationship context). Cheap DB
    // read (no embedding); None until the first background review runs.
    let relationship_review = db
        .with_conn(crate::db::relationship_reviews::get_latest)
        .ok()
        .flatten()
        .map(|r| r.summary);

    Ok(RetrievalResult {
        episodes: scored,
        facts,
        relationship,
        relationship_review,
        persona_traits,
        user_profile,
    })
}

/// Reinforces the given episodes as a genuine-recall write: strength grows
/// with diminishing returns toward 1.0, recall_count++, last_recalled_at = now.
///
/// `retrieve` itself is a pure read; this is the ONLY retrieval-path write, and
/// only callers that represent a REAL recall — a conversational reply or a
/// proactive mention of a memory — should call it. Keeps read-only paths (forget
/// matching, tests, embedding A/B harness) from inflating strength/recall_count.
/// ADR 2026-08-09 Part 2.
pub fn reinforce_top(db: &DbState, episodes: &[ScoredEpisode]) {
    let now = Utc::now().to_rfc3339();
    for se in episodes {
        if let Err(e) = db.with_conn(|conn| db_episodes::reinforce(conn, &se.episode.id, &now)) {
            log::warn!("Failed to reinforce episode {}: {}", se.episode.id, e);
        }
    }
}

/// Novelty (exploration) score: exp(-recall_count / NOVELTY_TAU), in 0..=1.
/// A memory never recalled scores 1.0; one recalled ~20 times is ~0. Prevents
/// the recall→reinforce→recall loop from letting a few dominant topics
/// monopolize the ranking (user feedback 2026-08-13).
pub fn compute_novelty(recall_count: i64) -> f64 {
    (-(recall_count as f64) / NOVELTY_TAU).exp()
}

/// Weighted-random surfacing anchor selection (diversity fix 2026-08-13).
///
/// Picks ONE episode from the scored pool for a proactive bubble — a weighted
/// draw, not an argmax:
///   1. Cooldown: episodes with `last_recalled_at` within
///      `SURFACE_COOLDOWN_HOURS` are dropped (the pet just talked about them).
///      If that empties the pool, cooldown is relaxed so she can still speak
///      rather than fall silent.
///   2. Softmax over `score / SURFACE_TEMPERATURE` among the survivors —
///      dominant memories stay *more likely* but can no longer win every
///      bubble ("浮现永远都是星际穿越/糯米").
///
/// Returns the index into `scored` of the drawn episode.
pub fn sample_surface_anchor(
    scored: &[ScoredEpisode],
    now: &DateTime<Utc>,
    rng: &mut impl rand::Rng,
) -> Option<usize> {
    if scored.is_empty() {
        return None;
    }
    let cooldown = chrono::Duration::hours(SURFACE_COOLDOWN_HOURS);
    let in_cooldown = |i: usize| {
        scored[i]
            .episode
            .last_recalled_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| now.signed_duration_since(dt.with_timezone(&Utc)) < cooldown)
            .unwrap_or(false)
    };
    let mut idxs: Vec<usize> = (0..scored.len()).filter(|&i| !in_cooldown(i)).collect();
    if idxs.is_empty() {
        // Everything is on cooldown — relax rather than go silent.
        idxs = (0..scored.len()).collect();
    }
    let weights: Vec<f64> = idxs
        .iter()
        .map(|&i| (scored[i].score / SURFACE_TEMPERATURE).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return Some(idxs[0]);
    }
    let mut roll = rng.gen::<f64>() * total;
    for (pos, &i) in idxs.iter().enumerate() {
        roll -= weights[pos];
        if roll <= 0.0 {
            return Some(i);
        }
    }
    Some(*idxs.last().unwrap())
}

/// Gets candidate episodes with their stored vectors (if available).
/// Fallback candidate selection by memory strength (used when no query embedding).
fn get_candidate_episodes(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<EpisodeWithVector>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type,
                    source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at,
                    consolidated, created_at
             FROM episodes
             ORDER BY memory_strength DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare episode candidates: {}", e))?;

    let rows = stmt
        .query_map(rusqlite::params![limit as i64], |row| {
            Ok(db_episodes::Episode {
                id: row.get(0)?,
                time: row.get(1)?,
                summary: row.get(2)?,
                emotion: row.get(3)?,
                importance: row.get(4)?,
                is_landmark: row.get::<_, i32>(5)? != 0,
                subject: row.get(6)?,
                participants: row.get(7)?,
                topics: row.get(8)?,
                source_type: row.get(9)?,
                source_conversation_id: row.get(10)?,
                source_turn: row.get(11)?,
                memory_strength: row.get(12)?,
                recall_count: row.get(13)?,
                last_recalled_at: row.get(14)?,
                consolidated: row.get::<_, i32>(15)? != 0,
                created_at: row.get(16)?,
            })
        })
        .map_err(|e| format!("Failed to query candidates: {}", e))?;

    // No vectors stored yet (sqlite-vec not integrated), so all are None.
    rows.filter_map(|r| r.ok())
        .map(|ep| Ok((ep, None)))
        .collect()
}

/// Gets all active facts (valid_to IS NULL).
fn get_active_facts(conn: &Connection) -> Result<Vec<db_facts::Fact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, category, key, value, confidence, valid_from, valid_to,
                    source_episode, mention_count, created_at, updated_at
             FROM facts
             WHERE valid_to IS NULL
             ORDER BY confidence DESC",
        )
        .map_err(|e| format!("Failed to prepare facts query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(db_facts::Fact {
                id: row.get(0)?,
                category: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
                valid_from: row.get(5)?,
                valid_to: row.get(6)?,
                source_episode: row.get(7)?,
                mention_count: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|e| format!("Failed to query facts: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
        .into_iter()
        .map(Ok)
        .collect()
}

/// Computes semantic similarity score (0..1, weighted by W_SEMANTIC).
fn compute_semantic(
    query_vec: &Option<Vec<f32>>,
    episode_vec: &Option<Vec<f32>>,
    query: &str,
    summary: &str,
) -> f64 {
    if let (Some(qv), Some(ev)) = (query_vec, episode_vec) {
        if qv.len() == ev.len() && !qv.is_empty() {
            let sim = cosine_similarity(qv, ev);
            // Map cosine (-1..1) to 0..1
            let normalized = ((sim + 1.0) / 2.0) as f64;
            return normalized * W_SEMANTIC;
        }
    }
    // Fallback: keyword overlap (Jaccard on word sets).
    keyword_similarity(query, summary) * W_SEMANTIC
}

/// Keyword overlap similarity for fallback. Falls back to character bigrams
/// for CJK text where whitespace tokenization produces no word overlap.
///
/// `pub(crate)` so `mind::forget` can reuse this proven CJK matcher when
/// finding the best fact / pending event to forget (facts and pending events
/// have no embedding vectors, so their match confidence is keyword-based).
pub(crate) fn keyword_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count() as f64;
    let union = words_a.union(&words_b).count() as f64;
    let word_sim = intersection / union;
    if word_sim > 0.0 {
        return word_sim;
    }
    // CJK fallback: character bigram Jaccard similarity.
    let bigrams_a = char_bigrams(a);
    let bigrams_b = char_bigrams(b);
    if bigrams_a.is_empty() || bigrams_b.is_empty() {
        return 0.0;
    }
    let bi_i = bigrams_a.intersection(&bigrams_b).count() as f64;
    let bi_u = bigrams_a.union(&bigrams_b).count() as f64;
    bi_i / bi_u
}

/// Generates character bigrams from a string (for CJK fallback similarity).
fn char_bigrams(s: &str) -> std::collections::HashSet<String> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    chars
        .windows(2)
        .map(|w| format!("{}{}", w[0], w[1]))
        .collect()
}

/// Overlap coefficient on character-bigrams: `|A ∩ B| / min(|A|, |B|)` (0..1).
/// Unlike the symmetric `keyword_similarity` (Jaccard, which divides by the
/// UNION), this normalizes by the SMALLER bigram set — so it stays high when a
/// short memory is surrounded by extra words in either direction:
///   - "忘掉咖啡" vs fact value "咖啡"      → 1.0 (the value is fully mentioned)
///   - "忘掉面试提醒" vs reminder "面试提醒" → 1.0 (the title is fully mentioned)
///   - "忘掉面试" vs reminder "明天的面试"   → 0.33 (genuinely ambiguous → declines)
/// Used by selective forgetting for facts / pending events, which have no
/// embedding vectors so their match confidence is keyword-driven. (Architecture
/// Principle #11: documented metric.)
pub(crate) fn char_overlap(a: &str, b: &str) -> f64 {
    let ba = char_bigrams(a);
    let bb = char_bigrams(b);
    if ba.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let inter = ba.intersection(&bb).count() as f64;
    let smaller = ba.len().min(bb.len()) as f64;
    inter / smaller
}

/// Computes time-based recency score: exp(-days_old / halflife).
fn compute_recency(time_str: &str, now: &DateTime<Utc>) -> f64 {
    let parsed = DateTime::parse_from_rfc3339(time_str)
        .or_else(|_| DateTime::parse_from_rfc3339(&format!("{}+00:00", time_str)));

    match parsed {
        Ok(dt) => {
            let days_old = (now.signed_duration_since(dt.with_timezone(&Utc))).num_hours() as f64 / 24.0;
            (-days_old / RECENCY_HALFLIFE_DAYS).exp()
        }
        Err(_) => 0.0,
    }
}

/// Computes emotion match score.
fn compute_emotion_match(episode_emotion: &Option<String>, current: &EmotionState) -> f64 {
    let current_label = crate::emotion::state::derive_mood_label(current);
    match episode_emotion {
        Some(emo) if emo == current_label => 1.0,
        _ => 0.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use crate::db::episodes as db_episodes;

    fn make_episode(conn: &Connection, summary: &str, strength: f64, time: &str) -> String {
        let id = format!("ep_test_{}", uuid::Uuid::new_v4().simple());
        let ep = db_episodes::Episode {
            id: id.clone(),
            time: time.to_string(),
            summary: summary.to_string(),
            emotion: Some("happy".to_string()),
            importance: 0.5,
            is_landmark: false,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: strength,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: time.to_string(),
        };
        db_episodes::insert(conn, &ep).unwrap();
        id
    }

    #[test]
    fn test_retrieve_basic() {
        let db = test_db();
        db.with_conn(|conn| {
            make_episode(conn, "ate hotpot with friends", 0.8, "2026-07-13T10:00:00+00:00");
            make_episode(conn, "wrote code all day", 0.3, "2026-07-10T10:00:00+00:00");
            Ok(())
        })
        .unwrap();

        let emotion = EmotionState::default();
        let result = retrieve("hotpot", &emotion, None, &db, 5).unwrap();

        assert!(!result.episodes.is_empty());
        // Hotpot episode should score higher on keyword similarity
        assert_eq!(result.episodes[0].episode.summary, "ate hotpot with friends");
    }

    #[test]
    fn test_retrieve_empty_db() {
        let db = test_db();
        let emotion = EmotionState::default();
        let result = retrieve("anything", &emotion, None, &db, 5).unwrap();
        assert!(result.episodes.is_empty());
        assert!(result.facts.is_empty());
    }

    #[test]
    fn test_retrieve_top_k() {
        let db = test_db();
        db.with_conn(|conn| {
            for i in 0..10 {
                make_episode(conn, &format!("event {}", i), 0.5, "2026-07-13T10:00:00+00:00");
            }
            Ok(())
        })
        .unwrap();

        let emotion = EmotionState::default();
        let result = retrieve("event", &emotion, None, &db, 3).unwrap();
        assert_eq!(result.episodes.len(), 3);
    }

    #[test]
    fn test_compute_novelty() {
        assert!((compute_novelty(0) - 1.0).abs() < 1e-9, "never recalled = full bonus");
        assert!((compute_novelty(5) - (-1.0_f64).exp()).abs() < 1e-9, "tau=5 half-life");
        assert!(compute_novelty(20) < 0.05, "heavily recalled ~= 0");
        assert!(compute_novelty(0) > compute_novelty(1) && compute_novelty(1) > compute_novelty(2));
    }

    #[test]
    fn test_novelty_affects_ranking() {
        // Two episodes identical except recall_count: the never-recalled one
        // must outrank the heavily-recalled one (exploration beats saturation).
        let db = test_db();
        let now = "2026-07-13T10:00:00+00:00";
        let fresh = db.with_conn(|conn| {
            Ok(make_episode(conn, "shared topic event A", 0.8, now))
        }).unwrap();
        let stale = db.with_conn(|conn| {
            Ok(make_episode(conn, "shared topic event B", 0.8, now))
        }).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE episodes SET recall_count = 25 WHERE id = ?1",
                rusqlite::params![stale],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();

        let result = retrieve("shared topic", &EmotionState::default(), None, &db, 2).unwrap();
        assert_eq!(result.episodes.len(), 2);
        assert_eq!(result.episodes[0].episode.id, fresh, "fresh memory should outrank saturated one");
    }

    #[test]
    fn test_sample_surface_anchor_cooldown() {
        // An episode recalled 10 minutes ago is on cooldown; the draw must
        // never pick it while a non-cooldown alternative exists (any seed).
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let make = |id: &str, score: f64, recalled: Option<String>| ScoredEpisode {
            episode: db_episodes::Episode {
                id: id.to_string(),
                time: "2026-07-13T10:00:00+00:00".to_string(),
                summary: format!("ep {}", id),
                emotion: None,
                importance: 0.5,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: "conversation".to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.5,
                recall_count: 0,
                last_recalled_at: recalled,
                consolidated: false,
                created_at: "2026-07-13T10:00:00+00:00".to_string(),
            },
            score,
            score_breakdown: ScoreBreakdown { semantic: 0.0, strength: 0.0, novelty: 0.0, recency: 0.0, emotion: 0.0 },
        };
        let now = Utc::now();
        let hot = make("hot", 1.0, Some(now.to_rfc3339()));
        let cold = make("cold", 0.01, Some((now - chrono::Duration::days(2)).to_rfc3339()));
        let scored = vec![hot, cold];
        for seed in 0..20u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let idx = sample_surface_anchor(&scored, &now, &mut rng).unwrap();
            assert_eq!(idx, 1, "cooldown episode must not be sampled (seed {})", seed);
        }
    }

    #[test]
    fn test_sample_surface_anchor_relaxes_when_all_cooldown() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let now = Utc::now();
        let make = |id: &str| ScoredEpisode {
            episode: db_episodes::Episode {
                id: id.to_string(),
                time: now.to_rfc3339(),
                summary: format!("ep {}", id),
                emotion: None,
                importance: 0.5,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: "conversation".to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.5,
                recall_count: 0,
                last_recalled_at: Some(now.to_rfc3339()),
                consolidated: false,
                created_at: now.to_rfc3339(),
            },
            score: 0.5,
            score_breakdown: ScoreBreakdown { semantic: 0.0, strength: 0.0, novelty: 0.0, recency: 0.0, emotion: 0.0 },
        };
        let scored = vec![make("a"), make("b")];
        let mut rng = StdRng::seed_from_u64(7);
        // All on cooldown → relax and still pick something (never None).
        assert!(sample_surface_anchor(&scored, &now, &mut rng).is_some());
    }

    #[test]
    fn test_sample_surface_anchor_empty() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(1);
        assert!(sample_surface_anchor(&[], &Utc::now(), &mut rng).is_none());
    }

    #[test]
    fn test_reinforce_diminishing() {
        // Diminishing boost: a strength-1.0 memory gains nothing; a 0.5 memory
        // gains less than the old flat +0.03 (0.015 here).
        let db = test_db();
        let id = db
            .with_conn(|conn| {
                Ok(make_episode(conn, "diminishing", 0.5, "2026-07-13T10:00:00+00:00"))
            })
            .unwrap();
        let now = Utc::now().to_rfc3339();
        db.with_conn(|conn| db_episodes::reinforce(conn, &id, &now)).unwrap();
        db.with_conn(|conn| {
            let ep = db_episodes::get(conn, &id)?.unwrap();
            assert!((ep.memory_strength - 0.515).abs() < 1e-9, "0.5 → 0.5+0.03*0.5 = 0.515, got {}", ep.memory_strength);
            Ok(())
        })
        .unwrap();
        // Saturate it and confirm no further growth past ~1.0.
        for _ in 0..100 {
            db.with_conn(|conn| db_episodes::reinforce(conn, &id, &now)).unwrap();
        }
        db.with_conn(|conn| {
            let ep = db_episodes::get(conn, &id)?.unwrap();
            assert!(ep.memory_strength < 1.0, "diminishing boost never exceeds 1.0");
            assert!(ep.memory_strength > 0.9, "but it does keep approaching 1.0");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_keyword_similarity() {
        let sim = keyword_similarity("ate hotpot", "ate hotpot with friends");
        assert!(sim > 0.0);
        assert!(sim <= 1.0);
    }

    #[test]
    fn test_keyword_similarity_no_overlap() {
        let sim = keyword_similarity("apple", "banana");
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_char_overlap() {
        // Short memory fully mentioned inside a longer request (either direction).
        assert!((char_overlap("忘掉咖啡", "咖啡") - 1.0).abs() < 0.001, "value fully contained");
        assert!((char_overlap("咖啡", "忘掉咖啡") - 1.0).abs() < 0.001, "symmetric");
        // Unrelated memory.
        assert!((char_overlap("忘掉那个", "咖啡") - 0.0).abs() < 0.001, "no overlap");
        // Multi-char value fully contained.
        assert!((char_overlap("忘掉咖啡奶茶这件事", "咖啡奶茶") - 1.0).abs() < 0.001);
        // Partial: only one shared bigram between two 3-bigram strings → 1/3.
        let partial = char_overlap("我提到了咖啡", "咖啡奶茶"); // shared {咖啡}, min(4,3)=3
        assert!((partial - 1.0 / 3.0).abs() < 0.01, "partial overlap got {}", partial);
        // Empty side is undefined -> 0.
        assert_eq!(char_overlap("abc", ""), 0.0);
    }

    #[test]
    fn test_recency_recent() {
        let now = Utc::now();
        let recent = Utc::now().to_rfc3339();
        let score = compute_recency(&recent, &now);
        assert!(score > 0.9);
    }

    #[test]
    fn test_recency_old() {
        let now = Utc::now();
        let old = (now - chrono::Duration::days(90)).to_rfc3339();
        let score = compute_recency(&old, &now);
        assert!(score < 0.2);
    }

    #[test]
    fn test_emotion_match() {
        let emotion = EmotionState::default();
       let matched = compute_emotion_match(&Some("调皮".to_string()), &emotion);
        let unmatched = compute_emotion_match(&Some("angry".to_string()), &emotion);
        assert!(matched > unmatched);
    }

    #[test]
    fn test_retrieve_is_pure_read() {
        // retrieve() must NOT mutate memory_strength or recall_count — it is a
        // pure read. Only genuine recall (converse/proactive via reinforce_top)
        // strengthens memory. ADR 2026-08-09 Part 2.
        let db = test_db();
        let ep_id = db
            .with_conn(|conn| {
                let id = make_episode(conn, "test event", 0.5, "2026-07-13T10:00:00+00:00");
                Ok(id)
            })
            .unwrap();

        let emotion = EmotionState::default();
        let _ = retrieve("test", &emotion, None, &db, 5).unwrap();

        db.with_conn(|conn| {
            let ep = db_episodes::get(conn, &ep_id)?.unwrap();
            assert!(
                (ep.memory_strength - 0.5).abs() < 1e-9,
                "pure read changed strength (was 0.5, now {})",
                ep.memory_strength
            );
            assert_eq!(ep.recall_count, 0, "pure read bumped recall_count");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_reinforce_top_strengthens() {
        // reinforce_top is the ONLY retrieval-path write: a genuine recall boosts
        // strength (+RECALL_BOOST, capped at 1.0) and recall_count.
        let db = test_db();
        let ep_id = db
            .with_conn(|conn| {
                let id = make_episode(conn, "test event", 0.5, "2026-07-13T10:00:00+00:00");
                Ok(id)
            })
            .unwrap();

        // Retrieve (pure read) then explicitly reinforce, as converse/proactive do.
        let result = retrieve("test", &EmotionState::default(), None, &db, 5).unwrap();
        reinforce_top(&db, &result.episodes);

        db.with_conn(|conn| {
            let ep = db_episodes::get(conn, &ep_id)?.unwrap();
            assert!(
                ep.memory_strength > 0.5,
                "strength should increase after reinforce_top (was {})",
                ep.memory_strength
            );
            assert_eq!(ep.recall_count, 1, "recall_count should be 1");
            Ok(())
        })
        .unwrap();
    }
}

