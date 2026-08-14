//! Pending event tracker: checks for due events and manages their lifecycle.

use crate::db::pending::PendingEvent;
use crate::db::DbState;

/// Returns pending events whose remind_date has arrived.
pub fn check_due(db: &DbState) -> Result<Vec<PendingEvent>, String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| crate::db::pending::get_due(conn, &now))
}

/// Marks an event as resolved after the user confirms or addresses it.
pub fn resolve(db: &DbState, event_id: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| crate::db::pending::mark_resolved(conn, event_id, &now))
}

/// Marks an event as triggered (it was used in a proactive bubble).
pub fn mark_triggered(db: &DbState, event_id: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| crate::db::pending::mark_triggered(conn, event_id, &now))
}

/// Expires events that have been followed up too many times without resolution.
/// Sets status to 'expired' for events exceeding max_followups.
pub fn expire_stale(db: &DbState, max_followups: i32) -> Result<u64, String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        let affected = conn
            .execute(
                "UPDATE pending_events SET status = 'expired', resolved_at = ?1
                 WHERE status = 'triggered' AND followup_count >= ?2",
                rusqlite::params![now, max_followups],
            )
            .map_err(|e| format!("Failed to expire stale events: {}", e))?;
        Ok(affected as u64)
    })
}

/// Increments the followup counter for an event (after a proactive bubble about it).
pub fn increment_followup(db: &DbState, event_id: &str) -> Result<(), String> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE pending_events SET followup_count = followup_count + 1 WHERE id = ?1",
            rusqlite::params![event_id],
        )
        .map_err(|e| format!("Failed to increment followup: {}", e))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use rusqlite::Connection;

fn insert_event(conn: &Connection, id: &str, remind: Option<&str>, status: &str, followups: i64) {
        crate::db::pending::insert(
            conn,
            &PendingEvent {
                origin: "user".to_string(),
                id: id.to_string(),
                title: format!("event {}", id),
                event_date: "2026-07-15".to_string(),
                remind_date: remind.map(|s| s.to_string()),
                source_episode: None,
                status: status.to_string(),
                importance: 0.5,
                followup_count: followups,
                created_at: "2026-07-14T10:00:00".to_string(),
                triggered_at: None,
                resolved_at: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn test_check_due() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_event(conn, "pe_1", Some("2026-07-14T08:00:00"), "pending", 0);
            insert_event(conn, "pe_2", Some("2099-01-01T00:00:00"), "pending", 0);
            insert_event(conn, "pe_3", None, "pending", 0);
            Ok(())
        })
        .unwrap();

        let due = check_due(&db).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "pe_1");
    }

    #[test]
    fn test_resolve() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_event(conn, "pe_1", Some("2026-07-14T08:00:00"), "pending", 0);
            Ok(())
        })
        .unwrap();

        resolve(&db, "pe_1").unwrap();

        let due = check_due(&db).unwrap();
        assert!(due.is_empty(), "resolved event should not be due");
    }

    #[test]
    fn test_expire_stale() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_event(conn, "pe_1", Some("2026-07-14T08:00:00"), "triggered", 3);
            insert_event(conn, "pe_2", Some("2026-07-14T08:00:00"), "triggered", 1);
            Ok(())
        })
        .unwrap();

        let expired = expire_stale(&db, 3).unwrap();
        assert_eq!(expired, 1);
    }

    #[test]
    fn test_increment_followup() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_event(conn, "pe_1", Some("2026-07-14T08:00:00"), "pending", 0);
            Ok(())
        })
        .unwrap();

        increment_followup(&db, "pe_1").unwrap();

        let due = check_due(&db).unwrap();
        assert_eq!(due[0].followup_count, 1);
    }
}
