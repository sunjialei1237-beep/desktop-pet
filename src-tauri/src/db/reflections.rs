use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    pub id: String,
    pub trigger_type: String,
    pub trigger_reason: Option<String>,
    pub thought: String,
    pub persona_updates: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalThought {
    pub id: String,
    pub content: String,
    pub emotion: Option<String>,
    pub source_reflection: Option<String>,
    pub surfacing_type: String,
    pub created_at: String,
    pub surfaced_at: Option<String>,
}

/// Inserts a reflection record.
pub fn insert_reflection(conn: &Connection, r: &Reflection) -> Result<(), String> {
    conn.execute(
        "INSERT INTO reflections (id, trigger_type, trigger_reason, thought, persona_updates, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![r.id, r.trigger_type, r.trigger_reason, r.thought, r.persona_updates, r.created_at],
    )
    .map_err(|e| format!("Failed to insert reflection: {}", e))?;
    Ok(())
}

/// Inserts an internal thought.
pub fn insert_thought(conn: &Connection, t: &InternalThought) -> Result<(), String> {
    conn.execute(
        "INSERT INTO internal_thoughts (id, content, emotion, source_reflection, surfacing_type, created_at, surfaced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![t.id, t.content, t.emotion, t.source_reflection, t.surfacing_type, t.created_at, t.surfaced_at],
    )
    .map_err(|e| format!("Failed to insert thought: {}", e))?;
    Ok(())
}

/// Returns unsurfaced internal thoughts (surfaced_at IS NULL).
pub fn get_unsurfaced(conn: &Connection) -> Result<Vec<InternalThought>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, content, emotion, source_reflection, surfacing_type, created_at, surfaced_at
             FROM internal_thoughts WHERE surfaced_at IS NULL
             ORDER BY created_at ASC",
        )
        .map_err(|e| format!("Failed to prepare thought query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(InternalThought {
                id: row.get(0)?,
                content: row.get(1)?,
                emotion: row.get(2)?,
                source_reflection: row.get(3)?,
                surfacing_type: row.get(4)?,
                created_at: row.get(5)?,
                surfaced_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query thoughts: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Returns ALL reflections, oldest first (full export).
pub fn get_all_reflections(conn: &Connection) -> Result<Vec<Reflection>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, trigger_type, trigger_reason, thought, persona_updates, created_at
             FROM reflections ORDER BY created_at ASC",
        )
        .map_err(|e| format!("Failed to prepare all reflections query: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Reflection {
                id: row.get(0)?,
                trigger_type: row.get(1)?,
                trigger_reason: row.get(2)?,
                thought: row.get(3)?,
                persona_updates: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query all reflections: {}", e))?;
    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Returns ALL internal thoughts (including surfaced), oldest first.
pub fn get_all_thoughts(conn: &Connection) -> Result<Vec<InternalThought>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, content, emotion, source_reflection, surfacing_type, created_at, surfaced_at
             FROM internal_thoughts ORDER BY created_at ASC",
        )
        .map_err(|e| format!("Failed to prepare all thoughts query: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(InternalThought {
                id: row.get(0)?,
                content: row.get(1)?,
                emotion: row.get(2)?,
                source_reflection: row.get(3)?,
                surfacing_type: row.get(4)?,
                created_at: row.get(5)?,
                surfaced_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query all thoughts: {}", e))?;
    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Marks a thought as surfaced.
pub fn mark_surfaced(conn: &Connection, id: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE internal_thoughts SET surfaced_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| format!("Failed to mark surfaced: {}", e))?;
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
    fn test_reflection_and_thought() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_reflection(conn, &Reflection {
                id: "ref_1".to_string(),
                trigger_type: "daily".to_string(),
                trigger_reason: Some("30 turns reached".to_string()),
                thought: "User seems busy lately".to_string(),
                persona_updates: None,
                created_at: "2026-07-14T22:00:00".to_string(),
            })?;

            insert_thought(conn, &InternalThought {
                id: "it_1".to_string(),
                content: "Hope they rest soon".to_string(),
                emotion: Some("concern".to_string()),
                source_reflection: Some("ref_1".to_string()),
                surfacing_type: "next_interaction".to_string(),
                created_at: "2026-07-14T22:00:01".to_string(),
                surfaced_at: None,
            })?;

            let unsurfaced = get_unsurfaced(conn)?;
            assert_eq!(unsurfaced.len(), 1);
            assert_eq!(unsurfaced[0].content, "Hope they rest soon");

            mark_surfaced(conn, "it_1", "2026-07-15T10:00:00")?;
            let after = get_unsurfaced(conn)?;
            assert_eq!(after.len(), 0);
            Ok(())
        })
        .unwrap();
    }
}
