use crate::db::emotion as db_emotion;
use crate::db::episodes as db_episodes;
use crate::db::facts as db_facts;
use crate::db::pending as db_pending;
use crate::db::vectors as db_vectors;
use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::mind::extractor::{EmotionDelta, ExtractionResult, FactInput, PendingInput};
use crate::mind::memory_gate;
use chrono::Utc;
use uuid::Uuid;

/// Backfills embeddings for episodes stored BEFORE the embedding model was
/// available — i.e. the first time BGE-M3 is enabled on a DB that already has
/// memories. Embeds the summary of every episode that has no vector yet and
/// stores it, so old memories benefit from semantic retrieval too.
///
/// No-op (returns 0) if the model isn't ready. Best-effort per episode: a
/// single embed failure is logged and skipped, not fatal.
///
/// Architecture: #1 (Rust drives, embedding only computes), #10 (old memories
/// deserve retrieval too), #11 (progress + counts logged).
pub fn backfill_missing_vectors(
    db: &DbState,
    embedding: &EmbeddingService,
) -> Result<usize, String> {
    if !embedding.is_ready() {
        return Ok(0);
    }

    // Episodes with no stored vector yet (LEFT JOIN ... IS NULL).
    let missing: Vec<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.summary FROM episodes e
                 LEFT JOIN episode_vectors v ON e.id = v.episode_id
                 WHERE v.episode_id IS NULL",
            )
            .map_err(|e| format!("prepare missing episodes: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("query missing episodes: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("read missing row: {}", e))?);
        }
        Ok(out)
    })?;

    if missing.is_empty() {
        log::info!("[backfill] all episodes already have vectors, nothing to do");
        return Ok(0);
    }
    log::info!("[backfill] embedding {} episode(s) missing vectors", missing.len());

    let mut added = 0usize;
    for (id, summary) in &missing {
        match embedding.embed(summary) {
            Ok(vec) => {
                match db.with_conn(|conn| db_vectors::insert(conn, id, &vec)) {
                    Ok(_) => added += 1,
                    Err(e) => log::warn!("[backfill] store vector for {} failed: {}", id, e),
                }
            }
            Err(e) => log::warn!("[backfill] embed {} failed: {}", id, e),
        }
    }
    log::info!("[backfill] added {} vector(s)", added);
    Ok(added)
}

/// Stores the extraction result into the database.
/// If embedding model is available, generates vectors for episodes.
///
/// Returns the episode ID if an episode was stored (for downstream reference).
pub fn store(
    result: &ExtractionResult,
    conversation_id: &str,
    turn: i32,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<Option<String>, String> {
    let now = Utc::now().to_rfc3339();
    let mut stored_episode_id: Option<String> = None;

    // 1. Episode storage + embedding
    if let Some(ep) = &result.episode {
        let ep_id = format!("ep_{}", Uuid::new_v4().simple());
        let episode = db_episodes::Episode {
            emotion_anchor: ep.emotion_anchor.clone(),
            id: ep_id.clone(),
            time: now.clone(),
            summary: ep.summary.clone(),
            emotion: ep.emotion.clone(),
            importance: ep.importance,
            is_landmark: false,
            subject: "user".to_string(),
            participants: if ep.participants.is_empty() {
                None
            } else {
                Some(ep.participants.join(", "))
            },
            topics: if ep.topics.is_empty() {
                None
            } else {
                Some(ep.topics.join(", "))
            },
            source_type: "conversation".to_string(),
            source_conversation_id: Some(conversation_id.to_string()),
            source_turn: Some(turn as i64),
            memory_strength: ep.importance, // starts at importance
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: now.clone(),
        };

        db.with_conn(|conn| db_episodes::insert(conn, &episode))?;

        // Generate and store embedding if model is ready.
        if let Some(emb) = embedding {
            if emb.is_ready() {
                match emb.embed(&ep.summary) {
                    Ok(vector) => {
                        log::info!("Generated embedding for episode {} ({} dim)", ep_id, vector.len());
                        if let Err(e) = db.with_conn(|conn| db_vectors::insert(conn, &ep_id, &vector)) {
                            log::warn!("Failed to store vector for episode {}: {}", ep_id, e);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to embed episode {}: {}", ep_id, e);
                    }
                }
            }
        }

        stored_episode_id = Some(ep_id);
    }

    // 2. Fact storage (hygiene gate -> dedup + temporal validity).
    // The gate (memory_gate::admits) drops extractor miss-extractions — knowledge
    // questions, the model's own conversational context, invented categories —
    // before they reach the DB. ADR 2026-08-09 Part 1.
    let admitted: Vec<&FactInput> = result.facts.iter().filter(|f| memory_gate::admits(f)).collect();
    let rejected = result.facts.len() - admitted.len();
    if rejected > 0 {
        log::info!(
            "[memory_gate] rejected {} noise fact(s) of {} extracted",
            rejected,
            result.facts.len()
        );
    }
    for fact in admitted {
        db.with_conn(|conn| store_fact(conn, fact, stored_episode_id.as_deref(), &now))?;
    }

    // 3. Emotion delta application
    if let Some(delta) = &result.emotion_delta {
        db.with_conn(|conn| apply_emotion_delta(conn, delta, &now))?;
    }

    // 4. Pending event (the user's own future event / reminder)
    if let Some(pe) = &result.pending_event {
        store_pending(db, pe, "user", stored_episode_id.as_deref(), &now)?;
    }

    // 5. Pet promise (something SHE agreed to do for the user). Same lifecycle
    // as a pending event — the only difference is origin, which flips the due
    // bubble from "reminding you about your thing" to "keeping my word".
    if let Some(promise) = &result.pet_promise {
        store_pending(db, promise, "pet", stored_episode_id.as_deref(), &now)?;
    }

    Ok(stored_episode_id)
}

/// Shared insert for pending events and pet promises (same table, same
/// lifecycle; `origin` distinguishes who owns it). Changelog is appended for
/// audit so an ingest-time promise is traceable later.
fn store_pending(
    db: &DbState,
    pe: &crate::mind::extractor::PendingInput,
    origin: &str,
    episode_id: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let pe_id = format!("pe_{}", Uuid::new_v4().simple());
    let remind_date = compute_remind_date(pe, now);
    // For short-term reminders (offset_minutes) there's no absolute
    // event_date; fall back to the computed remind_date so the NOT NULL
    // DB column stays meaningful (the event happens when we remind).
    let event_date = pe
        .event_date
        .clone()
        .unwrap_or_else(|| remind_date.clone().unwrap_or_else(|| now.to_string()));
    let event = db_pending::PendingEvent {
        id: pe_id,
        title: pe.title.clone(),
        event_date,
        remind_date,
        origin: origin.to_string(),
        source_episode: episode_id.map(|s| s.to_string()),
        status: "pending".to_string(),
        importance: 0.5,
        followup_count: 0,
        created_at: now.to_string(),
        triggered_at: None,
        resolved_at: None,
    };
    db.with_conn(|conn| {
        db_pending::insert(conn, &event)?;
        let _ = crate::db::changelog::append(
            conn,
            "pending",
            "insert",
            Some(&event.id),
            None,
            None,
            Some(&event.title),
            Some("memory extractor"),
        );
        Ok(())
    })
}

/// Stores a fact with dedup + conflict resolution.
///
/// First expires any currently-active fact under the same `(category, key)` so
/// a newly extracted value REPLACES the old one (single-valued slot). This
/// makes ingest consistent with the correction and consolidation paths, which
/// both call `expire_old`. Without it, a freshly stated preference ("我喜欢
/// 咖啡") would coexist with the stale one ("likes milk tea") until the next
/// background consolidation runs — and the stale fact (more mentions, ordered
/// first) wins recall, so she'd answer "奶茶" right after being told "咖啡".
/// `dedup_insert` then either revives the same value (mention++) or inserts
/// the new value; expired rows stay in the table for audit + future revival.
fn store_fact(
    conn: &rusqlite::Connection,
    fact: &FactInput,
    source_episode: Option<&str>,
    now: &str,
) -> Result<(), String> {
    // Expire conflicting same-key facts BEFORE insert. For a re-stated same
    // value this expires-then-revives (dedup_insert case 2 bumps mention_count);
    // for a new value it clears the old slot so only the current value remains.
    db_facts::expire_old(conn, &fact.category, &fact.key, now)?;

    let fact_id = format!("fact_{}", Uuid::new_v4().simple());
    let new_fact = db_facts::Fact {
        id: fact_id,
        category: fact.category.clone(),
        key: fact.key.clone(),
        value: fact.value.clone(),
        confidence: fact.confidence,
        valid_from: Some(now.to_string()),
        valid_to: None,
        source_episode: source_episode.map(|s| s.to_string()),
        mention_count: 1,
        created_at: now.to_string(),
        updated_at: now.to_string(),
        surfaced_count: 0,
        last_surfaced_at: None,
    };

    db_facts::dedup_insert(conn, &new_fact)?;

    let _ = crate::db::changelog::append(
        conn,
        "facts",
        "insert",
        Some(&new_fact.id),
        Some(&fact.category),
        None,
        Some(&format!("{}: {}", fact.key, fact.value)),
        Some("memory extractor"),
    );

    Ok(())
}

/// Computes the absolute remind_date from a pending input.
/// Architecture Principle #1: time arithmetic stays in Rust, never the LLM.
/// - `offset_minutes` (short-term reminder) -> now + offset.
/// - `event_date` (dated future event) -> that date at 08:00 UTC.
/// - neither -> None (not schedulable; caller falls back gracefully).
pub fn compute_remind_date(pending: &PendingInput, now: &str) -> Option<String> {
    if let Some(mins) = pending.offset_minutes {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(now) {
            let remind = dt.with_timezone(&Utc) + chrono::Duration::minutes(mins);
            return Some(remind.to_rfc3339());
        }
    }
    if let Some(event_date) = &pending.event_date {
        let date_part = &event_date[..event_date.len().min(10)];
        if let Ok(d) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
            return Some(format!("{}T08:00:00+00:00", d.format("%Y-%m-%d")));
        }
    }
    None
}

/// Applies emotion delta to the singleton emotion_state row.
pub(crate) fn apply_emotion_delta(conn: &rusqlite::Connection, delta: &EmotionDelta, now: &str) -> Result<(), String> {
    // Read current values
    let current = db_emotion::get(conn)?;
    let new_mood = (current.mood + delta.mood).clamp(0.0, 1.0);

    let _ = crate::db::changelog::log_change(
        conn,
        "emotion",
        "emotion_state",
        "mood",
        &format!("{:.3}", current.mood),
        &format!("{:.3}", new_mood),
        "conversation emotion delta",
    );
    let new_stress = (current.stress + delta.stress).clamp(0.0, 1.0);
    let new_energy = (current.physical_energy + delta.energy).clamp(0.0, 1.0);

    db_emotion::update_fields(
        conn,
        Some(new_mood),
        None,
        Some(new_energy),
        None,
        Some(new_stress),
        None,
        None,
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use crate::mind::extractor::{ExtractionResult, EpisodeInput, FactInput, PendingInput, EmotionDelta};

    #[test]
    fn test_store_episode_only() {
        let db = test_db();
        let result = ExtractionResult {
            pet_promise: None,
            episode: Some(EpisodeInput {
                emotion_anchor: None,
                summary: "ate hotpot with friends".to_string(),
                emotion: Some("happy".to_string()),
                importance: 0.7,
                participants: vec!["Alice".to_string()],
                topics: vec!["food".to_string()],
            }),
            facts: vec![],
            emotion_delta: None,
            pending_event: None,
        };

        let ep_id = store(&result, "conv_1", 0, &db, None).unwrap();
        assert!(ep_id.is_some());

        // Verify episode was stored
        db.with_conn(|conn| {
            let ep = db_episodes::get(conn, ep_id.as_ref().unwrap())?.unwrap();
            assert_eq!(ep.summary, "ate hotpot with friends");
            assert!((ep.importance - 0.7).abs() < 0.001);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_store_fact_dedup() {
        let db = test_db();
        let result1 = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![FactInput {
                category: "preference".to_string(),
                key: "drink".to_string(),
                value: "milk tea".to_string(),
                confidence: 0.9,
            }],
            emotion_delta: None,
            pending_event: None,
        };
        let result2 = result1.clone();

        store(&result1, "conv_1", 0, &db, None).unwrap();
        store(&result2, "conv_2", 0, &db, None).unwrap();

        // Should not produce duplicate (UNIQUE constraint on category+key+value)
        db.with_conn(|conn| {
            let facts = db_facts::get_active(conn, "preference", "drink")?;
            assert_eq!(facts.len(), 1);
            assert_eq!(facts[0].mention_count, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_store_fact_new_value_replaces_old_same_key() {
        // Regression: stating a new preference ("我喜欢咖啡") must expire the
        // old one ("likes milk tea") under the same (category, key) immediately,
        // not wait for consolidation. Otherwise the stale fact (more mentions)
        // wins recall and she answers the old preference.
        let db = test_db();
        let milk_tea = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![FactInput {
                category: "preference".to_string(),
                key: "beverage_preference".to_string(),
                value: "likes milk tea".to_string(),
                confidence: 0.85,
            }],
            emotion_delta: None,
            pending_event: None,
        };
        let coffee = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![FactInput {
                category: "preference".to_string(),
                key: "beverage_preference".to_string(),
                value: "likes coffee".to_string(),
                confidence: 0.85,
            }],
            emotion_delta: None,
            pending_event: None,
        };

        store(&milk_tea, "conv_1", 0, &db, None).unwrap();
        store(&coffee, "conv_2", 0, &db, None).unwrap();

        db.with_conn(|conn| {
            let active = db_facts::get_active(conn, "preference", "beverage_preference")?;
            assert_eq!(active.len(), 1, "only the new value should remain active");
            assert_eq!(active[0].value, "likes coffee", "new preference must win");
            // The old milk-tea row still exists but is expired (audit trail).
            let expired: Vec<String> = conn
                .prepare("SELECT value FROM facts WHERE category='preference' AND key='beverage_preference' AND valid_to IS NOT NULL")
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            assert!(expired.iter().any(|v| v == "likes milk tea"), "old value should be expired, not deleted");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_store_emotion_delta() {
        let db = test_db();
        let result = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![],
            emotion_delta: Some(EmotionDelta {
                mood: 0.1,
                stress: -0.05,
                energy: 0.0,
            }),
            pending_event: None,
        };

        store(&result, "conv_1", 0, &db, None).unwrap();

        db.with_conn(|conn| {
            let em = db_emotion::get(conn)?;
            assert!(em.mood > 0.5, "mood should have increased");
            assert!(em.stress < 0.2, "stress should have decreased");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_store_pending_event() {
        let db = test_db();
        let result = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![],
            emotion_delta: None,
            pending_event: Some(PendingInput {
                title: "job interview".to_string(),
                event_date: Some("2026-07-20".to_string()),
                offset_minutes: None,
            }),
        };

        store(&result, "conv_1", 0, &db, None).unwrap();

        // Verify the pending event was stored. get_due filters on remind_date
        // (which is NULL for this test), so we query directly.
        db.with_conn(|conn| {
        let count: Option<String> = conn
                .query_row(
                    "SELECT remind_date FROM pending_events WHERE title = 'job interview'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(|e| format!("Query error: {}", e))?;
            assert_eq!(count.as_deref(), Some("2026-07-20T08:00:00+00:00"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_compute_remind_date_dated() {
        let p = PendingInput { title: "exam".into(), event_date: Some("2026-07-20".into()), offset_minutes: None };
        let r = compute_remind_date(&p, "2026-07-14T10:00:00+00:00");
        assert_eq!(r.as_deref(), Some("2026-07-20T08:00:00+00:00"));
    }

    #[test]
    fn test_compute_remind_date_offset() {
        let p = PendingInput { title: "drink water".into(), event_date: None, offset_minutes: Some(3) };
        let r = compute_remind_date(&p, "2026-07-26T10:00:00+00:00");
        assert_eq!(r.as_deref(), Some("2026-07-26T10:03:00+00:00"));
    }

    #[test]
    fn test_compute_remind_date_dated_datetime() {
        let p = PendingInput { title: "exam".into(), event_date: Some("2026-07-20T14:00:00".into()), offset_minutes: None };
        let r = compute_remind_date(&p, "2026-07-14T10:00:00+00:00");
        assert_eq!(r.as_deref(), Some("2026-07-20T08:00:00+00:00"));
    }

    #[test]
    fn test_compute_remind_date_none_without_timing() {
        // No offset_minutes and no event_date -> not schedulable.
        let p = PendingInput { title: "vague".into(), event_date: None, offset_minutes: None };
        let r = compute_remind_date(&p, "2026-07-14T10:00:00+00:00");
        assert!(r.is_none());
    }

    #[test]
    fn test_store_pending_event_due_check() {
        let db = test_db();
        let result = ExtractionResult {
            pet_promise: None,
            episode: None,
            facts: vec![],
            emotion_delta: None,
            pending_event: Some(PendingInput {
                title: "exam".to_string(),
                event_date: Some("2020-01-01".to_string()), // past date, always due
                offset_minutes: None,
            }),
        };

        store(&result, "conv_1", 0, &db, None).unwrap();

        // Verify it's due (remind_date is in the past)
        let due = db.with_conn(|conn| {
            crate::db::pending::get_due(conn, "2099-01-01T00:00:00+00:00")
        }).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "exam");
    }

    #[test]
    fn test_store_pet_promise_origin_pet() {
        let db = test_db();
        let result = ExtractionResult {
            episode: None,
            facts: vec![],
            emotion_delta: None,
            pending_event: None,
            pet_promise: Some(PendingInput {
                title: "明早叫 ta 起床".to_string(),
                event_date: Some("2020-01-01".to_string()), // past date, always due
                offset_minutes: None,
            }),
        };

        store(&result, "conv_1", 0, &db, None).unwrap();

        // A pet promise lands in the SAME pending lifecycle but flagged
        // origin='pet' — the due bubble will voice it as her keeping her word.
        let due = db.with_conn(|conn| {
            crate::db::pending::get_due(conn, "2099-01-01T00:00:00+00:00")
        }).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "明早叫 ta 起床");
        assert_eq!(due[0].origin, "pet");
    }

    #[test]
    fn test_store_episode_emotion_anchor_persisted() {
        let db = test_db();
        let result = ExtractionResult {
            episode: Some(EpisodeInput {
                summary: "和糯米去看猫".to_string(),
                emotion: Some("开心".to_string()),
                emotion_anchor: Some("在猫咖，眼睛亮亮的".to_string()),
                importance: 0.7,
                participants: vec![],
                topics: vec![],
            }),
            facts: vec![],
            emotion_delta: None,
            pending_event: None,
            pet_promise: None,
        };

        let ep_id = store(&result, "conv_anchor", 0, &db, None).unwrap().unwrap();

        db.with_conn(|conn| {
            let ep = crate::db::episodes::get(conn, &ep_id)?.unwrap();
            assert_eq!(ep.emotion_anchor.as_deref(), Some("在猫咖，眼睛亮亮的"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_store_empty_result() {
        let db = test_db();
        let result = ExtractionResult::default();
        let ep_id = store(&result, "conv_1", 0, &db, None).unwrap();
        assert!(ep_id.is_none());
    }
}
