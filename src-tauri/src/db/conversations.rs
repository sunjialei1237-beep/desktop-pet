use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: String,
    pub turn: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// Inserts a conversation message.
pub fn insert(conn: &Connection, row: &ConversationRow) -> Result<(), String> {
    conn.execute(
        "INSERT INTO conversations (id, turn, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![row.id, row.turn, row.role, row.content, row.created_at],
    )
    .map_err(|e| format!("Failed to insert conversation: {}", e))?;
    Ok(())
}

/// Returns the most recent N conversation messages, oldest first.
pub fn get_recent(conn: &Connection, limit: i64) -> Result<Vec<ConversationRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, turn, role, content, created_at FROM conversations
             ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                turn: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to query conversations: {}", e))?;

    let mut result: Vec<ConversationRow> = rows.filter_map(|r| r.ok()).collect();
    result.reverse();
    Ok(result)
}

/// Returns ALL conversation messages, oldest first (full export — no LIMIT).
pub fn get_all(conn: &Connection) -> Result<Vec<ConversationRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, turn, role, content, created_at FROM conversations
             ORDER BY created_at ASC",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                turn: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to query conversations: {}", e))?;

    let result: Vec<ConversationRow> = rows.filter_map(|r| r.ok()).collect();
    Ok(result)
}

/// Returns the current max turn number (0 if empty).
pub fn get_max_turn(conn: &Connection, conversation_id: &str) -> Result<i64, String> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(turn) FROM conversations WHERE id = ?1",
            params![conversation_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    Ok(max.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_insert_and_get_recent() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &ConversationRow {
                id: "conv_1_t0".to_string(),
                turn: 0,
                role: "user".to_string(),
                content: "hello".to_string(),
                created_at: "2026-07-14T10:00:00".to_string(),
            })?;
            insert(conn, &ConversationRow {
                id: "conv_1_t1".to_string(),
                turn: 1,
                role: "assistant".to_string(),
                content: "hi there".to_string(),
                created_at: "2026-07-14T10:00:01".to_string(),
            })?;

            let recent = get_recent(conn, 10)?;
            assert_eq!(recent.len(), 2);
            assert_eq!(recent[0].content, "hello");
            assert_eq!(recent[1].content, "hi there");
            Ok(())
        })
        .unwrap();
    }
}
