//! Memory Consolidation: compresses low-importance episodes into abstract summaries.
//! Design doc 5.9: cascade compression, detail fading, abstract cognition stabilization.
//!
//! Also handles lifecycle cleanup: deleting very old, low-importance, unrecalled episodes.
//!
//! Principle 9 (Layered complexity): MVP consolidation is simple threshold-based grouping.
//! V2: after compressing a batch, durable user facts in the summary are extracted
//! and written back to the facts table (conflict detection via expire_old).
//!
//! Principle 1: the LLM only proposes facts (JSON); Rust validates (category
//! whitelist, confidence clamp) and writes them.
//! Principle 8: consolidation is a low-frequency background task (hourly cadence,
//! no-op below threshold), so one extra LLM call per consolidation is acceptable.
//! Principle 11: every written fact carries source_episode pointing at the
//! consolidated episode; failures are logged, never silent.

use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::extractor::FactInput;

/// Minimum episode count to trigger consolidation.
const CONSOLIDATION_THRESHOLD: i64 = 100;
/// Batch size for each consolidation group.
const BATCH_SIZE: usize = 10;

/// Fact categories accepted from the LLM (mirrors extractor.txt:32). Anything
/// else is dropped — the LLM must not invent new memory taxonomies (#1).
const FACT_CATEGORIES: [&str; 7] = [
    "preference",
    "relationship",
    "goal",
    "profile",
    "school",
    "work",
    "health",
];

/// Runs consolidation if episode count exceeds threshold.
/// Returns the number of episodes consolidated.
pub async fn consolidate(db: &DbState, llm: &LlmClient) -> Result<usize, String> {
    // Check if we have enough episodes to warrant consolidation.
    let low_imp_count: i64 = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM episodes WHERE consolidated = 0 AND importance < 0.4",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count episodes: {}", e))
    })?;

    if low_imp_count < CONSOLIDATION_THRESHOLD {
        return Ok(0);
    }

    // Fetch a batch of low-importance unconsolidated episodes.
    let episodes: Vec<crate::db::episodes::Episode> = db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, time, summary, emotion, importance, is_landmark,
                        subject, participants, topics, source_type,
                        source_conversation_id, source_turn,
                        memory_strength, recall_count, last_recalled_at,
                        consolidated, created_at
                 FROM episodes WHERE consolidated = 0 AND importance < 0.4
                 ORDER BY created_at ASC LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare: {}", e))?;

        let rows = stmt
            .query_map(rusqlite::params![BATCH_SIZE as i64], |row| {
                Ok(crate::db::episodes::Episode {
                    emotion_anchor: None,
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
            .map_err(|e| format!("Failed to query: {}", e))?;

        rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
    })?;

    if episodes.is_empty() {
        return Ok(0);
    }

    // Ask LLM to compress the summaries.
    let summaries = episodes.iter().map(|e| format!("- {}", e.summary)).collect::<Vec<_>>().join("\n");
    let prompt = format!(
        "You are summarizing memories. Compress these into one concise abstract summary (1-2 sentences):\n{}",
        summaries
    );

    let messages = vec![ChatMessage::system(prompt)];
    // 4096: consolidation is a generation task. DeepSeek-v4 is a reasoning model, so
    // reasoning_content eats most of the budget — 2048 leaves `content` empty (pitfall #3).
    let result = llm.chat_reflection(&messages, Some(0.5), Some(4096)).await
        .map_err(|e| format!("Consolidation LLM call failed: {}", e))?;

    let consolidated_summary = result.content.trim().to_string();
    // Empty content (reasoning ate the budget, or transient LLM failure) must not be
    // persisted as a garbage episode. Skip this batch — it stays unconsolidated and
    // retries next cycle. Unlike reflection, consolidation returns free text (not
    // JSON), so without this check an empty reply silently writes a blank summary.
    if consolidated_summary.is_empty() {
        log::warn!(
            "[consolidation] LLM returned empty summary; skipping {} episodes (will retry next cycle)",
            episodes.len()
        );
        return Ok(0);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let consolidated_count = episodes.len();

    // Write: mark originals as consolidated, insert compressed summary.
    let (source_episode, backfill_summary) = db.with_conn(|conn| {
        for ep in &episodes {
            crate::db::episodes::mark_consolidated(conn, &ep.id)?;
        }

        // Insert the consolidated summary as a new episode.
        let new_ep = crate::db::episodes::Episode {
            emotion_anchor: None,
            id: format!("ep_consolidated_{}", chrono::Utc::now().timestamp_millis()),
            time: now.clone(),
            summary: consolidated_summary,
            emotion: None,
            importance: 0.3,
            is_landmark: false,
            subject: "user".to_string(),
            participants: None,
            topics: Some("consolidated".to_string()),
            source_type: "consolidation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: 0.5,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: now,
        };
        let (ep_id, ep_summary) = (new_ep.id.clone(), new_ep.summary.clone());
        crate::db::episodes::insert(conn, &new_ep)?;

        log::info!("Consolidated {} episodes into 1 summary", consolidated_count);
        Ok::<_, String>((ep_id, ep_summary))
    })?;

    // V2: write durable facts from the summary back to the facts table.
    // Isolated failure — the consolidation itself already succeeded; a fact
    // extraction failure must not bubble up (loop_runner would warn on a
    // consolidated batch that is actually done).
    match backfill_facts(db, llm, &backfill_summary, &source_episode).await {
        Ok(_) => {}
        Err(e) => log::warn!("[consolidation] fact backfill failed (non-fatal): {}", e),
    }

    Ok(consolidated_count)
}

/// Parses the LLM's fact-extraction JSON output into `FactInput`s.
/// Accepts markdown fences and surrounding prose (same tolerance as the
/// extractor pipeline). Pure — unit-testable without an LLM.
fn parse_facts_json(raw: &str) -> Result<Vec<FactInput>, String> {
    let trimmed = raw.trim();
    let json = if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 3 {
            lines[1..lines.len() - 1].join("\n").trim().to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    #[derive(Debug, serde::Deserialize)]
    struct LlmFacts {
        #[serde(default)]
        facts: Vec<FactInput>,
    }

    let parsed: LlmFacts = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse consolidation facts '{}': {}", raw.trim(), e))?;
    Ok(parsed.facts)
}

/// Validates LLM-proposed facts and writes them to the facts table.
/// - Unknown categories are dropped (whitelist above).
/// - Confidence is clamped to [0, 1].
/// - A contradicting active fact with the same (category, key) is expired
///   first (V2 conflict detection), then the new fact is dedup-inserted.
/// Returns the number of facts actually written.
fn write_facts(
    conn: &rusqlite::Connection,
    facts: &[FactInput],
    source_episode: &str,
) -> Result<usize, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut written = 0usize;
    for f in facts {
        let key = f.key.trim();
        let value = f.value.trim();
        if key.is_empty() || value.is_empty() || !FACT_CATEGORIES.contains(&f.category.as_str()) {
            continue;
        }
        crate::db::facts::expire_old(conn, &f.category, key, &now)?;
        crate::db::facts::dedup_insert(
            conn,
            &crate::db::facts::Fact {
                id: format!("fact_cons_{}", uuid::Uuid::new_v4()),
                category: f.category.clone(),
                key: key.to_string(),
                value: value.to_string(),
                confidence: f.confidence.clamp(0.0, 1.0),
                valid_from: Some(now.clone()),
                valid_to: None,
                source_episode: Some(source_episode.to_string()),
                mention_count: 1,
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )?;
        written += 1;
    }
    Ok(written)
}

/// Extracts durable user facts from a consolidated summary and writes them back
/// to the facts table. Called after a successful consolidation; errors are
/// isolated (logged, never fail the consolidation that already succeeded).
async fn backfill_facts(
    db: &DbState,
    llm: &LlmClient,
    summary: &str,
    source_episode: &str,
) -> Result<usize, String> {
    let prompt = format!(
        "You are extracting durable facts about the user from a memory summary. \
         The summary was produced by compressing many small memories, so facts may be \
         implied across multiple memories. Extract ONLY facts the user explicitly stated \
         about themselves or their life. Do NOT infer or extrapolate (e.g. \"busy at work\" \
         does NOT mean \"has a job\"; \"feels tired\" is NOT a fact). Return JSON only: \
         {{\"facts\": [{{\"category\": ..., \"key\": ..., \"value\": ..., \"confidence\": 0.0}}]}}. \
         Categories: preference, relationship, goal, profile, school, work, health. \
         key must be a SHORT Chinese noun phrase (e.g. \"饮料\", \"宠物\", \"烹饪\"), never free-form \
         English and never a sentence — stable keys let the system merge repeats. \
         value is the fact itself in Chinese. Confidence: \"maybe\" 0.3-0.5; stated preference \
         0.7-0.85; always/never/strong 0.9-0.98. If nothing durable, return {{\"facts\": []}}.\n\nSUMMARY:\n{}",
        summary
    );
    let messages = vec![ChatMessage::system(prompt)];
    // 4096: generation task — DeepSeek-v4 reasoning eats most of the budget
    // (pitfall #3). Same size as the consolidation summary call above.
    let result = llm
        .chat_reflection(&messages, Some(0.3), Some(4096))
        .await
        .map_err(|e| format!("Consolidation fact extraction LLM call failed: {}", e))?;

    if result.content.trim().is_empty() {
        return Ok(0);
    }
    let facts = parse_facts_json(&result.content)?;
    if facts.is_empty() {
        return Ok(0);
    }
    let written = db.with_conn(|conn| write_facts(conn, &facts, source_episode))?;
    log::info!(
        "[consolidation] backfilled {} facts into facts table (from {} proposed, source={})",
        written,
        facts.len(),
        source_episode
    );
    Ok(written)
}

/// Runs lifecycle cleanup: deletes episodes below importance threshold
/// that haven't been recalled in `days` days. Landmarks are never deleted.
pub fn lifecycle_cleanup(db: &DbState) -> Result<u64, String> {
    db.with_conn(|conn| crate::db::episodes::cleanup_old(conn, 0.2, 60))
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::episodes::{insert, Episode};
    use crate::db::test_utils::test_db;

    fn test_episode(id: &str, importance: f64) -> Episode {
        Episode {
            emotion_anchor: None,
            id: id.to_string(),
            time: "2026-07-14T10:00:00".to_string(),
            summary: "test".to_string(),
            emotion: None,
            importance,
            is_landmark: false,
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
            created_at: "2026-07-14T10:00:00".to_string(),
        }
    }

    #[test]
    fn test_lifecycle_cleanup_keeps_recent() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &test_episode("ep1", 0.1))?;
            Ok(())
        }).unwrap();

        // With 60-day threshold and recent created_at, nothing should be deleted.
        let deleted = lifecycle_cleanup(&db).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_lifecycle_cleanup_deletes_old_low_importance() {
        let db = test_db();
        db.with_conn(|conn| {
            let mut ep = test_episode("ep_old", 0.1);
            ep.created_at = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
            ep.last_recalled_at = Some((chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339());
            insert(conn, &ep)?;
            Ok(())
        }).unwrap();

        let deleted = lifecycle_cleanup(&db).unwrap();
        assert!(deleted >= 1, "should have deleted old low-importance episode");
    }

    #[test]
    fn test_lifecycle_cleanup_keeps_landmark() {
        let db = test_db();
        db.with_conn(|conn| {
            let mut ep = test_episode("ep_lm", 0.1);
            ep.is_landmark = true;
            ep.created_at = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
            ep.last_recalled_at = Some((chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339());
            insert(conn, &ep)?;
            Ok(())
        }).unwrap();

        let deleted = lifecycle_cleanup(&db).unwrap();
        assert_eq!(deleted, 0, "landmark should never be deleted");
    }

    // --- B1: consolidation fact backfill (V2) ---

    fn fact(cat: &str, key: &str, value: &str, confidence: f64) -> FactInput {
        FactInput {
            category: cat.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            confidence,
        }
    }

    /// Inserts a real episode so facts referencing it satisfy the
    /// source_episode -> episodes(id) foreign key.
    fn insert_ep(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
        insert(conn, &test_episode(id, 0.5))
    }

    #[test]
    fn test_parse_facts_json_plain() {
        let raw = r#"{"facts":[{"category":"preference","key":"drink","value":"milk tea","confidence":0.8}]}"#;
        let facts = parse_facts_json(raw).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].key, "drink");
    }

    #[test]
    fn test_parse_facts_json_fenced() {
        let raw = "```json\n{\"facts\":[{\"category\":\"work\",\"key\":\"role\",\"value\":\"designer\",\"confidence\":0.9}]}\n```";
        let facts = parse_facts_json(raw).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].category, "work");
    }

    #[test]
    fn test_parse_facts_json_empty() {
        let facts = parse_facts_json(r#"{"facts":[]}"#).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn test_write_facts_dedup_and_conflict() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_ep(conn, "ep_old")?;
            insert_ep(conn, "ep_consolidated_x")?;
            // Existing fact: coffee.
            crate::db::facts::dedup_insert(conn, &crate::db::facts::Fact {
                id: "f_old".to_string(),
                category: "preference".to_string(),
                key: "drink".to_string(),
                value: "coffee".to_string(),
                confidence: 0.7,
                valid_from: Some("2026-07-01".to_string()),
                valid_to: None,
                source_episode: Some("ep_old".to_string()),
                mention_count: 3,
                created_at: "2026-07-01".to_string(),
                updated_at: "2026-07-01".to_string(),
            })?;

            // Backfill contradicts (drink=coffee -> milk tea) + duplicates (same fact twice).
            let n = write_facts(
                conn,
                &[
                    fact("preference", "drink", "milk tea", 0.8),
                    fact("preference", "drink", "milk tea", 0.8),
                ],
                "ep_consolidated_x",
            )?;
            assert_eq!(n, 2);

            // Old coffee fact expired, new milk tea active with mention_count 2.
            let active = crate::db::facts::get_active(conn, "preference", "drink")?;
            assert_eq!(active.len(), 1, "conflicting old fact must be expired");
            assert_eq!(active[0].value, "milk tea");
            assert_eq!(active[0].mention_count, 2, "duplicate should increment mention_count");
            assert_eq!(active[0].source_episode.as_deref(), Some("ep_consolidated_x"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_write_facts_filters_category_and_clamps_confidence() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_ep(conn, "ep_consolidated_y")?;
            let n = write_facts(
                conn,
                &[
                    fact("preference", "drink", "water", 0.8),
                    fact("made_up_category", "x", "y", 0.8),
                    fact("work", "role", "designer", 1.7),
                    fact("profile", "name", "", 0.8),
                    fact("health", "", "sleepy", 0.8),
                ],
                "ep_consolidated_y",
            )?;
            // 5 proposed: 2 valid (water, designer clamped to 1.0),
            // 1 dropped (unknown category), 2 dropped (empty key/value).
            assert_eq!(n, 2, "only whitelisted non-empty facts survive");

            let all = crate::db::facts::get_all_active(conn, 10)?;
            assert_eq!(all.len(), 2);
            let water = all.iter().find(|f| f.value == "water").unwrap();
            assert_eq!(water.confidence, 0.8);
            let designer = all.iter().find(|f| f.key == "role").unwrap();
            assert_eq!(designer.confidence, 1.0, "confidence clamped to 1.0");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_write_facts_empty_is_noop() {
        let db = test_db();
        db.with_conn(|conn| {
            let n = write_facts(conn, &[], "ep_none")?;
            assert_eq!(n, 0, "empty facts list writes nothing");
            let all = crate::db::facts::get_all_active(conn, 10)?;
            assert!(all.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_write_facts_clamps_confidence() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_ep(conn, "ep_c")?;
            write_facts(conn, &[fact("goal", "target", "graduation", 1.7)], "ep_c")?;
            let all = crate::db::facts::get_all_active(conn, 10)?;
            assert_eq!(all[0].confidence, 1.0, "confidence clamped to 1.0");
            Ok(())
        })
        .unwrap();
    }
}
