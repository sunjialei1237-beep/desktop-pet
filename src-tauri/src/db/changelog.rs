//! Append-only Change Log (Architecture Principle A4).
//! Records state transitions for Debug Panel timeline replay.
//! This is a debug/observability aid; SQLite remains the source of truth.

use rusqlite::{params, Connection};

/// Appends a change-log entry. All fields except module/action are optional.
pub fn append(
    conn: &Connection,
    module: &str,
    action: &str,
    target: Option<&str>,
    field: Option<&str>,
    old_value: Option<&str>,
    new_value: Option<&str>,
    reason: Option<&str>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO change_log (timestamp, module, action, target, field, old_value, new_value, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![now, module, action, target, field, old_value, new_value, reason],
    )
    .map_err(|e| format!("Failed to append change_log: {}", e))?;
    Ok(())
}

/// Convenience for logging a value change on a named target.
pub fn log_change(
    conn: &Connection,
    module: &str,
    target: &str,
    field: &str,
    old_value: &str,
    new_value: &str,
    reason: &str,
) -> Result<(), String> {
    append(
        conn,
        module,
        "change",
        Some(target),
        Some(field),
        Some(old_value),
        Some(new_value),
        Some(reason),
    )
}

/// Returns the most recent entries (newest first) for the Debug Panel.
pub fn recent(conn: &Connection, limit: u32) -> Result<Vec<ChangeLogEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, module, action, target, field, old_value, new_value, reason
             FROM change_log ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| format!("Failed to query change_log: {}", e))?;
    let entries = stmt
        .query_map(params![limit], |row| {
            Ok(ChangeLogEntry {
                timestamp: row.get(0)?,
                module: row.get(1)?,
                action: row.get(2)?,
                target: row.get::<_, Option<String>>(3)?,
                field: row.get::<_, Option<String>>(4)?,
                old_value: row.get::<_, Option<String>>(5)?,
                new_value: row.get::<_, Option<String>>(6)?,
                reason: row.get::<_, Option<String>>(7)?,
            })
        })
        .map_err(|e| format!("Failed to read change_log rows: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect change_log rows: {}", e))?;
    Ok(entries)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChangeLogEntry {
    pub timestamp: String,
    pub module: String,
    pub action: String,
    pub target: Option<String>,
    pub field: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_append_and_recent() {
        let db = test_db();
        db.with_conn(|conn| {
            append(conn, "emotion", "change", Some("mood"), Some("mood"), Some("0.3"), Some("0.6"), Some("user happy"))?;
            append(conn, "facts", "insert", Some("fact_1"), None, None, Some("likes milk tea"), Some("extractor"))?;
            let entries = recent(conn, 10)?;
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].module, "facts");
            assert_eq!(entries[1].module, "emotion");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_recent_limit() {
        let db = test_db();
        db.with_conn(|conn| {
            for i in 0..5 {
                append(conn, "test", "tick", None, None, None, None, Some(&format!("entry {}", i)))?;
            }
            let entries = recent(conn, 3)?;
            assert_eq!(entries.len(), 3);
            Ok(())
        })
        .unwrap();
    }
}
