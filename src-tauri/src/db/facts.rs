use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub source_episode: Option<String>,
    pub mention_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Inserts a fact. If the exact (category, key, value) already exists,
/// increments mention_count instead of creating a duplicate.
pub fn dedup_insert(conn: &Connection, fact: &Fact) -> Result<(), String> {
    // Case 1: identical ACTIVE fact -> increment mention_count.
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM facts WHERE category=?1 AND key=?2 AND value=?3
             AND valid_to IS NULL",
            params![fact.category, fact.key, fact.value],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(id) = existing_id {
        conn.execute(
            "UPDATE facts SET mention_count = mention_count + 1, updated_at = ?1 WHERE id = ?2",
            params![fact.updated_at, id],
        )
        .map_err(|e| format!("Failed to update fact mention_count: {}", e))?;
        return Ok(());
    }

    // Case 2: identical but EXPIRED fact -> revive it. A fresh INSERT would
    // violate UNIQUE(category, key, value) because the expired row still
    // exists; reviving keeps the row (and its mention history) while making
    // it active again.
    let expired_id: Option<String> = conn
        .query_row(
            "SELECT id FROM facts WHERE category=?1 AND key=?2 AND value=?3
             AND valid_to IS NOT NULL",
            params![fact.category, fact.key, fact.value],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(id) = expired_id {
        conn.execute(
            "UPDATE facts SET valid_to = NULL, confidence = ?1, source_episode = ?2,
                mention_count = mention_count + 1, updated_at = ?3 WHERE id = ?4",
            params![fact.confidence, fact.source_episode, fact.updated_at, id],
        )
        .map_err(|e| format!("Failed to revive expired fact: {}", e))?;
        return Ok(());
    }

    conn.execute(
        "INSERT INTO facts (id, category, key, value, confidence, valid_from, valid_to,
            source_episode, mention_count, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            fact.id, fact.category, fact.key, fact.value, fact.confidence,
            fact.valid_from, fact.valid_to, fact.source_episode,
            fact.mention_count, fact.created_at, fact.updated_at,
        ],
    )
    .map_err(|e| format!("Failed to insert fact: {}", e))?;
    Ok(())
}

/// Expires old facts when a contradicting fact arrives.
/// Sets valid_to = now on the old fact with the same (category, key) but different value.
pub fn expire_old(conn: &Connection, category: &str, key: &str, now: &str) -> Result<u64, String> {
    let affected = conn
        .execute(
            "UPDATE facts SET valid_to = ?1, updated_at = ?1
             WHERE category = ?2 AND key = ?3 AND valid_to IS NULL",
            params![now, category, key],
        )
        .map_err(|e| format!("Failed to expire old facts: {}", e))?;
    Ok(affected as u64)
}

/// Gets all active (not expired) facts by category and key.
pub fn get_active(conn: &Connection, category: &str, key: &str) -> Result<Vec<Fact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, category, key, value, confidence, valid_from, valid_to,
                    source_episode, mention_count, created_at, updated_at
             FROM facts WHERE category = ?1 AND key = ?2
             AND valid_to IS NULL
             ORDER BY mention_count DESC, confidence DESC",
        )
        .map_err(|e| format!("Failed to prepare fact query: {}", e))?;

    let rows = stmt
        .query_map(params![category, key], |row| {
            Ok(Fact {
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

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Gets all facts for a category.
pub fn get_by_category(conn: &Connection, category: &str) -> Result<Vec<Fact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, category, key, value, confidence, valid_from, valid_to,
                    source_episode, mention_count, created_at, updated_at
             FROM facts WHERE category = ?1
             ORDER BY key, mention_count DESC",
        )
        .map_err(|e| format!("Failed to prepare fact query: {}", e))?;

    let rows = stmt
        .query_map(params![category], |row| {
            Ok(Fact {
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

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    fn test_fact(id: &str, cat: &str, key: &str, val: &str) -> Fact {
        Fact {
            id: id.to_string(),
            category: cat.to_string(),
            key: key.to_string(),
            value: val.to_string(),
            confidence: 0.8,
            valid_from: Some("2026-07-14".to_string()),
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: "2026-07-14T10:00:00".to_string(),
            updated_at: "2026-07-14T10:00:00".to_string(),
        }
    }

    #[test]
    fn test_dedup_insert() {
        let db = test_db();
        db.with_conn(|conn| {
            dedup_insert(conn, &test_fact("f1", "preference", "drink", "milk tea"))?;
            dedup_insert(conn, &test_fact("f2", "preference", "drink", "milk tea"))?;

            let facts = get_active(conn, "preference", "drink")?;
            assert_eq!(facts.len(), 1, "duplicate should not create new row");
            assert_eq!(facts[0].mention_count, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_expire_old() {
        let db = test_db();
        db.with_conn(|conn| {
            dedup_insert(conn, &test_fact("f1", "preference", "drink", "coffee"))?;
            expire_old(conn, "preference", "drink", "2026-07-20T00:00:00")?;

            // Old fact should be expired
            let active = get_active(conn, "preference", "drink")?;
            assert_eq!(active.len(), 0, "expired fact should not be active");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_dedup_insert_revives_expired() {
        let db = test_db();
        db.with_conn(|conn| {
            dedup_insert(conn, &test_fact("f1", "preference", "drink", "coffee"))?;
            expire_old(conn, "preference", "drink", "2026-07-20T00:00:00")?;

            // Same (category, key, value) arrives again: must revive the row,
            // not fail on UNIQUE(category, key, value).
            let mut revived = test_fact("f2", "preference", "drink", "coffee");
            revived.source_episode = Some("ep_new".to_string());
            conn.execute(
                "INSERT INTO episodes (id, time, summary, importance, subject, source_type, memory_strength, recall_count, consolidated, created_at)
                 VALUES ('ep_new', '2026-07-20T00:00:00', 's', 0.5, 'user', 'conversation', 0.5, 0, 0, '2026-07-20T00:00:00')",
                [],
            )
            .map_err(|e| e.to_string())?;
            dedup_insert(conn, &revived)?;

            let active = get_active(conn, "preference", "drink")?;
            assert_eq!(active.len(), 1, "expired fact should be revived as active");
            assert_eq!(active[0].id, "f1", "revive keeps the original row id");
            assert_eq!(active[0].mention_count, 2, "revive increments mention_count");
            assert_eq!(active[0].source_episode.as_deref(), Some("ep_new"));
            Ok(())
        })
        .unwrap();
    }
}
/// Gets all active (non-expired) facts.
pub fn get_all_active(conn: &Connection, limit: i64) -> Result<Vec<Fact>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, category, key, value, confidence, valid_from, valid_to,
                    source_episode, mention_count, created_at, updated_at
             FROM facts WHERE valid_to IS NULL
             ORDER BY mention_count DESC, confidence DESC
             LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare fact query: {}", e))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(Fact {
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

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

