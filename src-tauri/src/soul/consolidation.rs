//! Memory Consolidation: compresses low-importance episodes into abstract summaries.
//! Design doc 5.9: cascade compression, detail fading, abstract cognition stabilization.
//!
//! Also handles lifecycle cleanup: deleting very old, low-importance, unrecalled episodes.
//!
//! Principle 9 (Layered complexity): MVP consolidation is simple threshold-based grouping.
//! V2 will add conflict detection and formal validators.

use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};

/// Minimum episode count to trigger consolidation.
const CONSOLIDATION_THRESHOLD: i64 = 100;
/// Batch size for each consolidation group.
const BATCH_SIZE: usize = 10;

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

    let messages = vec![ChatMessage { role: "system".to_string(), content: prompt }];
    let result = llm.chat_reflection(&messages, Some(0.5), Some(2048)).await
        .map_err(|e| format!("Consolidation LLM call failed: {}", e))?;

    let consolidated_summary = result.content.trim().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let consolidated_count = episodes.len();

    // Write: mark originals as consolidated, insert compressed summary.
    db.with_conn(|conn| {
        for ep in &episodes {
            crate::db::episodes::mark_consolidated(conn, &ep.id)?;
        }

        // Insert the consolidated summary as a new episode.
        let new_ep = crate::db::episodes::Episode {
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
        crate::db::episodes::insert(conn, &new_ep)?;

        log::info!("Consolidated {} episodes into 1 summary", consolidated_count);
        Ok::<_, String>(())
    })?;

    Ok(consolidated_count)
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
}
