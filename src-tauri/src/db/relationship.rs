use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub closeness: f64,
    pub trust: f64,
    pub days_known: i64,
    pub total_conversations: i64,
    pub shared_events: i64,
    pub last_interaction_at: Option<String>,
    pub last_interaction_type: Option<String>,
    pub closeness_log: Option<String>,
    pub updated_at: String,
}

/// Gets the singleton relationship row.
pub fn get(conn: &Connection) -> Result<Relationship, String> {
    conn.query_row(
        "SELECT closeness, trust, days_known, total_conversations, shared_events,
                last_interaction_at, last_interaction_type, closeness_log, updated_at
         FROM relationship WHERE id = 1",
        [],
        |row| {
            Ok(Relationship {
                closeness: row.get(0)?,
                trust: row.get(1)?,
                days_known: row.get(2)?,
                total_conversations: row.get(3)?,
                shared_events: row.get(4)?,
                last_interaction_at: row.get(5)?,
                last_interaction_type: row.get(6)?,
                closeness_log: row.get(7)?,
                updated_at: row.get(8)?,
            })
        },
    )
    .map_err(|e| format!("Failed to get relationship: {}", e))
}

/// Adds delta to closeness, clamped to [0, 100].
pub fn add_closeness(conn: &Connection, delta: f64, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE relationship SET
            closeness = MAX(0, MIN(100, closeness + ?1)),
            updated_at = ?2
         WHERE id = 1",
        params![delta, now],
    )
    .map_err(|e| format!("Failed to add closeness: {}", e))?;
    Ok(())
}

/// Decays closeness by a factor (e.g. after a week of no interaction).
pub fn decay_closeness(conn: &Connection, factor: f64, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE relationship SET closeness = closeness * ?1, updated_at = ?2 WHERE id = 1",
        params![factor, now],
    )
    .map_err(|e| format!("Failed to decay closeness: {}", e))?;
    Ok(())
}

/// Increments conversation counter and updates last interaction info.
pub fn record_interaction(conn: &Connection, interaction_type: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE relationship SET
            total_conversations = total_conversations + 1,
            last_interaction_at = ?1,
            last_interaction_type = ?2,
            updated_at = ?1
         WHERE id = 1",
        params![now, interaction_type],
    )
    .map_err(|e| format!("Failed to record interaction: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_relationship_singleton() {
        let db = test_db();
        db.with_conn(|conn| {
            let rel = get(conn)?;
            assert!((rel.closeness - 0.0).abs() < 0.001);
            assert_eq!(rel.total_conversations, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_add_closeness_and_clamp() {
        let db = test_db();
        db.with_conn(|conn| {
            add_closeness(conn, 10.0, "now")?;
            assert!((get(conn)?.closeness - 10.0).abs() < 0.001);

            add_closeness(conn, 200.0, "now")?;
            assert!((get(conn)?.closeness - 100.0).abs() < 0.001, "should clamp to 100");
            Ok(())
        })
        .unwrap();
    }
}
