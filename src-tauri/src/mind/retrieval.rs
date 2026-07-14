use crate::db::facts as db_facts;
use crate::db::persona as db_persona;
use crate::db::relationship as db_relationship;
use crate::db::episodes as db_episodes;
use crate::db::DbState;
use crate::embedding::{cosine_similarity, EmbeddingService};
use crate::emotion::state::EmotionState;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

/// Weights for the hybrid retrieval score.
const W_SEMANTIC: f64 = 0.4;
const W_STRENGTH: f64 = 0.3;
const W_RECENCY: f64 = 0.2;
const W_EMOTION: f64 = 0.1;

/// Recency half-life in days.
const RECENCY_HALFLIFE_DAYS: f64 = 30.0;

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
    pub recency: f64,
    pub emotion: f64,
}

/// Full retrieval result.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub episodes: Vec<ScoredEpisode>,
    pub facts: Vec<db_facts::Fact>,
    pub relationship: Option<db_relationship::Relationship>,
    pub persona_traits: Vec<db_persona::PersonaTrait>,
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
    let candidates: Vec<(db_episodes::Episode, Option<Vec<f32>>)> = db.with_conn(|conn| {
        get_candidate_episodes(conn, 50)
    })?;

    // Score each candidate.
    let mut scored: Vec<ScoredEpisode> = candidates
        .into_iter()
        .map(|(ep, ep_vec)| {
            let semantic = compute_semantic(&query_vec, &ep_vec, query, &ep.summary);
            let strength = ep.memory_strength * W_STRENGTH;
            let recency = compute_recency(&ep.time, &now) * W_RECENCY;
            let emotion_score = compute_emotion_match(&ep.emotion, emotion) * W_EMOTION;
            let total = semantic + strength + recency + emotion_score;

            ScoredEpisode {
                episode: ep,
                score: total,
                score_breakdown: ScoreBreakdown {
                    semantic: semantic / W_SEMANTIC,
                    strength: strength / W_STRENGTH,
                    recency: recency / W_RECENCY,
                    emotion: emotion_score / W_EMOTION,
                },
            }
        })
        .collect();

    // Sort by score descending, take top-K.
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    // Reinforce retrieved episodes (memory strength += 0.03).
    let now_str = now.to_rfc3339();
    for scored_ep in &scored {
        if let Err(e) = db.with_conn(|conn| {
            db_episodes::reinforce(conn, &scored_ep.episode.id, &now_str)
        }) {
            log::warn!("Failed to reinforce episode {}: {}", scored_ep.episode.id, e);
        }
    }

    // Retrieve active facts.
    let facts = db.with_conn(|conn| get_active_facts(conn))?;

    // Retrieve persona snapshot.
    let relationship = db.with_conn(|conn| db_relationship::get(conn)).ok();
    let persona_traits = db.with_conn(|conn| {
        Ok(db_persona::get_traits_by_type(conn, "all")
            .unwrap_or_default())
    })?;

    Ok(RetrievalResult {
        episodes: scored,
        facts,
        relationship,
        persona_traits,
    })
}

/// Gets candidate episodes with their stored vectors (if available).
/// TODO: use sqlite-vec for vector search once integrated.
fn get_candidate_episodes(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<(db_episodes::Episode, Option<Vec<f32>>)>, String> {
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

/// Simple keyword overlap similarity for fallback.
fn keyword_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count() as f64;
    let union = words_a.union(&words_b).count() as f64;
    intersection / union
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
        let matched = compute_emotion_match(&Some("tiao pi".to_string()), &emotion);
        let unmatched = compute_emotion_match(&Some("angry".to_string()), &emotion);
        assert!(matched > unmatched);
    }

    #[test]
    fn test_strength_reinforcement() {
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
            // Strength should have increased by 0.03 (reinforce boost)
            assert!(ep.memory_strength > 0.5, "strength was {} should be > 0.5", ep.memory_strength);
            assert_eq!(ep.recall_count, 1);
            Ok(())
        })
        .unwrap();
    }
}
