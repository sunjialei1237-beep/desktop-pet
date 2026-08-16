//! Proactive-bubble log — the speaker's own memory of what she last said
//! unprompted (2026-08-16 续⁴¹: LLM anchor selector + surfacing continuity).
//!
//! Every proactive bubble outcome appends a row; the next surfacing decision
//! (selector prompt) and the voicing prompts read the recent rows back, so she
//! always knows what she last said and how long ago — the cross-bubble
//! continuity the round-robin governance could not give (it rotated WHAT gets
//! picked, but the speaker never saw her own last words). Rust writes/reads
//! all state (Principle #1); the LLM only consumes the injected lines.
//!
//! Retention: bounded to the newest [`KEEP_ROWS`] entries, pruned inline on
//! insert — a log of one-liners never justifies unbounded growth.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Cap on retained rows (pruned on insert). 200 ≈ months of bubbles at ≤1/hour.
const KEEP_ROWS: i64 = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BubbleLogEntry {
    pub id: i64,
    /// RFC3339 UTC timestamp of the bubble.
    pub time: String,
    /// Which generator produced it: "proactive_memory" | "lively" |
    /// "due_pending" | "welcome_back" | "lonely_nudge".
    pub kind: String,
    /// The voiced reply (full text).
    pub text: String,
    /// The memory anchor it was grounded on ("" for anchorless bubbles).
    pub anchor: String,
    /// Why the anchor surfaced (selector's reason / Rust-computed 由头).
    pub anchor_reason: Option<String>,
}

/// Appends a bubble outcome and prunes beyond the retention cap.
pub fn insert(
    conn: &Connection,
    kind: &str,
    text: &str,
    anchor: &str,
    anchor_reason: Option<&str>,
    time: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO bubble_log (time, kind, text, anchor, anchor_reason) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![time, kind, text, anchor, anchor_reason],
    )
    .map_err(|e| format!("Failed to insert bubble_log: {}", e))?;
    // Bound the log: keep only the newest KEEP_ROWS rows.
    conn.execute(
        "DELETE FROM bubble_log WHERE id NOT IN (SELECT id FROM bubble_log ORDER BY id DESC LIMIT ?1)",
        rusqlite::params![KEEP_ROWS],
    )
    .map_err(|e| format!("Failed to prune bubble_log: {}", e))?;
    Ok(())
}

/// Returns the `n` most recent bubbles, newest first.
pub fn get_recent(conn: &Connection, n: usize) -> Result<Vec<BubbleLogEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, time, kind, text, anchor, anchor_reason FROM bubble_log ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare bubble_log query: {}", e))?;
    let rows = stmt
        .query_map(rusqlite::params![n as i64], |row| {
            Ok(BubbleLogEntry {
                id: row.get(0)?,
                time: row.get(1)?,
                kind: row.get(2)?,
                text: row.get(3)?,
                anchor: row.get(4)?,
                anchor_reason: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query bubble_log: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read bubble_log row: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn insert_and_get_recent_newest_first() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, "lively", "窗外的蝉好吵", "", None, "2026-08-16T01:00:00Z")?;
            insert(conn, "proactive_memory", "面试加油呀", "在准备找实习", Some("一直惦记的事"), "2026-08-16T02:00:00Z")?;
            let recent = get_recent(conn, 2)?;
            assert_eq!(recent.len(), 2);
            assert_eq!(recent[0].kind, "proactive_memory", "newest first");
            assert_eq!(recent[0].text, "面试加油呀");
            assert_eq!(recent[0].anchor, "在准备找实习");
            assert_eq!(recent[0].anchor_reason.as_deref(), Some("一直惦记的事"));
            assert_eq!(recent[1].kind, "lively");
            // Limit respected.
            assert_eq!(get_recent(conn, 1)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn retention_prunes_old_rows() {
        let db = test_db();
        db.with_conn(|conn| {
            for i in 0..(KEEP_ROWS + 10) {
                insert(conn, "lively", &format!("bubble {i}"), "", None, "2026-08-16T00:00:00Z")?;
            }
            let recent = get_recent(conn, 1000)?;
            assert_eq!(recent.len() as i64, KEEP_ROWS, "log bounded to KEEP_ROWS");
            assert_eq!(recent[0].text, format!("bubble {}", KEEP_ROWS + 9), "newest kept");
            Ok(())
        })
        .unwrap();
    }
}
