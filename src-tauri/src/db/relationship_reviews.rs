//! Relationship reviews: periodic LLM-generated summaries of how the
//! relationship with the user is progressing (Hermes-style background review).
//!
//! Stored separately from episodes so it does not pollute event retrieval —
//! a review is a relationship-level synthesis, not a single event. Only the
//! latest review is injected into the prompt (always-on relationship context),
//! while the full history is retained for traceability (Architecture #11).
//!
//! Principle 1: the LLM only produces the summary text; Rust decides when to
//! run and where to store it. Principle 8: low-frequency background task.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipReview {
    pub id: String,
    pub summary: String,
    pub created_at: String,
}

/// Inserts a relationship review record.
pub fn insert(conn: &Connection, r: &RelationshipReview) -> Result<(), String> {
    conn.execute(
        "INSERT INTO relationship_reviews (id, summary, created_at)
         VALUES (?1, ?2, ?3)",
        params![r.id, r.summary, r.created_at],
    )
    .map_err(|e| format!("Failed to insert relationship review: {}", e))?;
    Ok(())
}

/// Returns the most recent relationship review (None if none recorded).
/// Used by retrieval to inject always-on relationship context into the prompt.
pub fn get_latest(conn: &Connection) -> Result<Option<RelationshipReview>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, summary, created_at FROM relationship_reviews
             ORDER BY created_at DESC LIMIT 1",
        )
        .map_err(|e| format!("Failed to prepare latest review query: {}", e))?;
    let mut rows = stmt
        .query_map([], |row| {
            Ok(RelationshipReview {
                id: row.get(0)?,
                summary: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("Failed to query latest review: {}", e))?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(format!("Failed to read review row: {}", e)),
        None => Ok(None),
    }
}

/// Timestamp of the most recent review (None if none recorded). Pure, used by
/// the review scheduler's due-check without a full row load.
pub fn latest_created_at(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT MAX(created_at) FROM relationship_reviews",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .map_err(|e| format!("Failed to get latest review time: {}", e))
}

/// Returns ALL relationship reviews, oldest first (full export).
pub fn get_all(conn: &Connection) -> Result<Vec<RelationshipReview>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, summary, created_at FROM relationship_reviews
             ORDER BY created_at ASC",
        )
        .map_err(|e| format!("Failed to prepare all reviews query: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RelationshipReview {
                id: row.get(0)?,
                summary: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("Failed to query all reviews: {}", e))?;
    let result: Vec<RelationshipReview> = rows.filter_map(|r| r.ok()).collect();
    Ok(result)
}

/// Total number of stored reviews (for diagnostics / threshold checks).
pub fn count(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM relationship_reviews", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("Failed to count reviews: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_insert_and_get_latest() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &RelationshipReview {
                id: "rev_1".to_string(),
                summary: "你们最近聊得很开心".to_string(),
                created_at: "2026-08-07T10:00:00".to_string(),
            })?;
            insert(conn, &RelationshipReview {
                id: "rev_2".to_string(),
                summary: "用户最近在忙实习".to_string(),
                created_at: "2026-08-07T11:00:00".to_string(),
            })?;

            let latest = get_latest(conn)?;
            let latest = latest.expect("should have a latest review");
            assert_eq!(latest.id, "rev_2", "latest = newest by created_at");
            assert_eq!(latest.summary, "用户最近在忙实习");

            assert_eq!(count(conn)?, 2);
            assert_eq!(
                latest_created_at(conn)?,
                Some("2026-08-07T11:00:00".to_string())
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_get_latest_empty() {
        let db = test_db();
        db.with_conn(|conn| {
            assert!(get_latest(conn)?.is_none());
            assert_eq!(count(conn)?, 0);
            assert_eq!(latest_created_at(conn)?, None);
            Ok(())
        })
        .unwrap();
    }
}
