use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEvent {
    pub id: String,
    pub title: String,
    pub event_date: String,
    pub remind_date: Option<String>,
    pub source_episode: Option<String>,
    pub status: String,
    pub importance: f64,
    pub followup_count: i64,
    pub created_at: String,
    pub triggered_at: Option<String>,
    pub resolved_at: Option<String>,
}

/// Inserts a new pending event.
pub fn insert(conn: &Connection, ev: &PendingEvent) -> Result<(), String> {
    conn.execute(
        "INSERT INTO pending_events (
            id, title, event_date, remind_date, source_episode,
            status, importance, followup_count, created_at, triggered_at, resolved_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            ev.id, ev.title, ev.event_date, ev.remind_date, ev.source_episode,
            ev.status, ev.importance, ev.followup_count,
            ev.created_at, ev.triggered_at, ev.resolved_at,
        ],
    )
    .map_err(|e| format!("Failed to insert pending event: {}", e))?;
    Ok(())
}

/// Returns pending events whose remind_date has arrived and status is still 'pending'.
pub fn get_due(conn: &Connection, now: &str) -> Result<Vec<PendingEvent>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, event_date, remind_date, source_episode,
                    status, importance, followup_count, created_at, triggered_at, resolved_at
             FROM pending_events
             WHERE status = 'pending'
               AND remind_date IS NOT NULL
               AND remind_date <= ?1
             ORDER BY remind_date ASC",
        )
        .map_err(|e| format!("Failed to prepare pending query: {}", e))?;

    let rows = stmt
        .query_map(params![now], |row| {
            Ok(PendingEvent {
                id: row.get(0)?,
                title: row.get(1)?,
                event_date: row.get(2)?,
                remind_date: row.get(3)?,
                source_episode: row.get(4)?,
                status: row.get(5)?,
                importance: row.get(6)?,
                followup_count: row.get(7)?,
                created_at: row.get(8)?,
                triggered_at: row.get(9)?,
                resolved_at: row.get(10)?,
            })
        })
        .map_err(|e| format!("Failed to query pending events: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Marks a pending event as triggered at the given time.
pub fn mark_triggered(conn: &Connection, id: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE pending_events SET status = 'triggered', triggered_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| format!("Failed to mark triggered: {}", e))?;
    Ok(())
}

/// Marks a pending event as resolved.
pub fn mark_resolved(conn: &Connection, id: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE pending_events SET status = 'resolved', resolved_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| format!("Failed to mark resolved: {}", e))?;
    Ok(())
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_insert_and_get_due() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &PendingEvent {
                id: "pe_1".to_string(),
                title: "interview tomorrow".to_string(),
                event_date: "2026-07-15".to_string(),
                remind_date: Some("2026-07-15T08:00:00".to_string()),
                source_episode: None,
                status: "pending".to_string(),
                importance: 0.8,
                followup_count: 0,
                created_at: "2026-07-14T10:00:00".to_string(),
                triggered_at: None,
                resolved_at: None,
            })?;

            let due = get_due(conn, "2026-07-15T09:00:00")?;
            assert_eq!(due.len(), 1);
            assert_eq!(due[0].title, "interview tomorrow");

            // Not due yet
            let not_due = get_due(conn, "2026-07-14T20:00:00")?;
            assert_eq!(not_due.len(), 0);

            // Mark triggered
            mark_triggered(conn, "pe_1", "2026-07-15T08:05:00")?;
            let due_after = get_due(conn, "2026-07-15T09:00:00")?;
            assert_eq!(due_after.len(), 0, "triggered event should not be due");
            Ok(())
        })
        .unwrap();
    }
}
