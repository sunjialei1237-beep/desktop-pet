use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Ebbinghaus decay rate per day.
const DAILY_DECAY: f64 = 0.998;
/// Reinforcement on each recall.
const RECALL_BOOST: f64 = 0.03;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub time: String,
    pub summary: String,
    pub emotion: Option<String>,
    pub importance: f64,
    pub is_landmark: bool,
    pub subject: String,
    pub participants: Option<String>,
    pub topics: Option<String>,
    pub source_type: String,
    pub source_conversation_id: Option<String>,
    pub source_turn: Option<i64>,
    pub memory_strength: f64,
    pub recall_count: i64,
    pub last_recalled_at: Option<String>,
    pub consolidated: bool,
    pub created_at: String,
}

/// Inserts a new episode. memory_strength starts at importance.
pub fn insert(conn: &Connection, ep: &Episode) -> Result<(), String> {
    conn.execute(
        "INSERT INTO episodes (
            id, time, summary, emotion, importance, is_landmark,
            subject, participants, topics, source_type,
            source_conversation_id, source_turn,
            memory_strength, recall_count, last_recalled_at,
            consolidated, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            ep.id, ep.time, ep.summary, ep.emotion, ep.importance,
            ep.is_landmark as i32, ep.subject, ep.participants, ep.topics,
            ep.source_type, ep.source_conversation_id, ep.source_turn,
            ep.memory_strength, ep.recall_count, ep.last_recalled_at,
            ep.consolidated as i32, ep.created_at,
        ],
    )
    .map_err(|e| format!("Failed to insert episode: {}", e))?;
    Ok(())
}

/// Gets an episode by ID.
pub fn get(conn: &Connection, id: &str) -> Result<Option<Episode>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type,
                    source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at,
                    consolidated, created_at
             FROM episodes WHERE id = ?1",
        )
        .map_err(|e| format!("Failed to prepare episode query: {}", e))?;

    let result = stmt
        .query_row(params![id], |row| {
            Ok(Episode {
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
        .ok();

    Ok(result)
}

/// Decays all non-landmark episode strengths by the daily decay rate.
pub fn decay_strength(conn: &Connection) -> Result<u64, String> {
    let affected = conn
        .execute(
            "UPDATE episodes SET memory_strength = memory_strength * ?1
             WHERE is_landmark = 0 AND consolidated = 0",
            params![DAILY_DECAY],
        )
        .map_err(|e| format!("Failed to decay episodes: {}", e))?;
    Ok(affected as u64)
}

/// Reinforces an episode's memory on recall: strength += RECALL_BOOST, recall_count++.
pub fn reinforce(conn: &Connection, id: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE episodes SET
            memory_strength = MIN(1.0, memory_strength + ?1),
            recall_count = recall_count + 1,
            last_recalled_at = ?2
         WHERE id = ?3",
        params![RECALL_BOOST, now, id],
    )
    .map_err(|e| format!("Failed to reinforce episode: {}", e))?;
    Ok(())
}

/// Returns episodes by a list of IDs (used after vector search).
pub fn search_by_ids(conn: &Connection, ids: &[String]) -> Result<Vec<Episode>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT id, time, summary, emotion, importance, is_landmark,
                subject, participants, topics, source_type,
                source_conversation_id, source_turn,
                memory_strength, recall_count, last_recalled_at,
                consolidated, created_at
         FROM episodes WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Failed to prepare: {}", e))?;
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(Episode {
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
        .map_err(|e| format!("Failed to query episodes: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

// Helper trait for piping
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    fn test_episode(id: &str, strength: f64) -> Episode {
        Episode {
            id: id.to_string(),
            time: "2026-07-14T10:00:00".to_string(),
            summary: "test episode".to_string(),
            emotion: Some("happy".to_string()),
            importance: 0.5,
            is_landmark: false,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: strength,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: "2026-07-14T10:00:00".to_string(),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &test_episode("ep_1", 0.7))?;
            let ep = get(conn, "ep_1")?.unwrap();
            assert_eq!(ep.summary, "test episode");
            assert!((ep.memory_strength - 0.7).abs() < 0.001);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_decay_and_reinforce() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &test_episode("ep_1", 1.0))?;

            decay_strength(conn)?;
            let ep = get(conn, "ep_1")?.unwrap();
            assert!(ep.memory_strength < 1.0, "strength should have decayed");
            assert!((ep.memory_strength - 0.998).abs() < 0.001);

            reinforce(conn, "ep_1", "2026-07-14T11:00:00")?;
            let ep = get(conn, "ep_1")?.unwrap();
            assert!((ep.memory_strength - 1.0).abs() < 0.001, "reinforced strength should clamp to 1.0");
            assert_eq!(ep.recall_count, 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_landmark_not_decayed() {
        let db = test_db();
        db.with_conn(|conn| {
            let mut ep = test_episode("ep_lm", 0.5);
            ep.is_landmark = true;
            insert(conn, &ep)?;

            decay_strength(conn)?;
            let result = get(conn, "ep_lm")?.unwrap();
            assert!((result.memory_strength - 0.5).abs() < 0.001, "landmark should not decay");
            Ok(())
        })
        .unwrap();
    }
}
