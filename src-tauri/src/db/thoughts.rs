//! thought_stream CRUD — her persistent pool of psychological states
//! (thought-stream plan v3 §2.1). Seeds are structured states
//! {stimulus, emotion_tone, relation_hint}, NOT pre-baked lines; lines are
//! born only at render time in `soul::stream::voice`.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Seed states (mirror of the v8 schema).
pub const STATE_PENDING: &str = "pending";
pub const STATE_VOICED: &str = "voiced";
pub const STATE_UNSPOKEN: &str = "unspoken";
pub const STATE_EXPIRED: &str = "expired";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtSeed {
    pub id: String,
    /// 触发素材（脱敏事实，Rust 组装）："用户连续编辑 main.rs 40分钟"。
    pub stimulus: String,
    /// 种子产生时的情绪色调（EmotionState 映射）："好奇"/"惦记"/"困倦"。
    pub emotion_tone: Option<String>,
    /// 它对"我们"意味着什么："关注项目进展"/"他最近压力大"。
    pub relation_hint: Option<String>,
    /// environment | body | memory | self_state | relationship | temporal |
    /// ritual | pending | reflection
    pub origin: String,
    pub salience: f64,
    pub created_at: String,
    pub state: String,
    pub unspoken_reason: Option<String>,
    pub voiced_at: Option<String>,
    pub evolved_from: Option<String>,
}

pub fn insert(conn: &Connection, s: &ThoughtSeed) -> Result<(), String> {
    conn.execute(
        "INSERT INTO thought_stream (id, stimulus, emotion_tone, relation_hint, origin, salience, created_at, state, unspoken_reason, voiced_at, evolved_from)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            s.id,
            s.stimulus,
            s.emotion_tone,
            s.relation_hint,
            s.origin,
            s.salience,
            s.created_at,
            s.state,
            s.unspoken_reason,
            s.voiced_at,
            s.evolved_from,
        ],
    )
    .map_err(|e| format!("Failed to insert thought seed: {}", e))?;
    Ok(())
}

fn row_to_seed(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThoughtSeed> {
    Ok(ThoughtSeed {
        id: row.get(0)?,
        stimulus: row.get(1)?,
        emotion_tone: row.get(2)?,
        relation_hint: row.get(3)?,
        origin: row.get(4)?,
        salience: row.get(5)?,
        created_at: row.get(6)?,
        state: row.get(7)?,
        unspoken_reason: row.get(8)?,
        voiced_at: row.get(9)?,
        evolved_from: row.get(10)?,
    })
}

const COLS: &str =
    "id, stimulus, emotion_tone, relation_hint, origin, salience, created_at, state, unspoken_reason, voiced_at, evolved_from";

/// Pending seeds, highest salience first (the score layer re-ranks; this is
/// just the raw candidate pool).
pub fn get_pending(conn: &Connection, limit: usize) -> Result<Vec<ThoughtSeed>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM thought_stream WHERE state = ?1 ORDER BY salience DESC, created_at ASC LIMIT ?2"
        ))
        .map_err(|e| format!("Failed to prepare thought_stream query: {}", e))?;
    let rows = stmt
        .query_map(rusqlite::params![STATE_PENDING, limit as i64], row_to_seed)
        .map_err(|e| format!("Failed to query thought_stream: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Whether a pending seed with exactly this stimulus already exists (ingest
/// dedup — the same environment summary must not pile up).
pub fn pending_stimulus_exists(conn: &Connection, stimulus: &str) -> Result<bool, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM thought_stream WHERE state = ?1 AND stimulus = ?2",
            rusqlite::params![STATE_PENDING, stimulus],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count pending seeds: {}", e))?;
    Ok(n > 0)
}

pub fn count_pending(conn: &Connection) -> Result<i64, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM thought_stream WHERE state = ?1",
            rusqlite::params![STATE_PENDING],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count pending seeds: {}", e))?;
    Ok(n)
}

pub fn mark_voiced(conn: &Connection, id: &str, now: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE thought_stream SET state = ?2, voiced_at = ?3 WHERE id = ?1",
        rusqlite::params![id, STATE_VOICED, now],
    )
    .map_err(|e| format!("Failed to mark seed voiced: {}", e))?;
    Ok(())
}

/// The seed was evaluated and she chose NOT to say it — kept with salience
/// intact for later reinforcement (the "她记得" raw material).
pub fn mark_unspoken(conn: &Connection, id: &str, reason: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE thought_stream SET state = ?2, unspoken_reason = ?3 WHERE id = ?1",
        rusqlite::params![id, STATE_UNSPOKEN, reason],
    )
    .map_err(|e| format!("Failed to mark seed unspoken: {}", e))?;
    Ok(())
}

/// Natural forgetting: pending seeds older than the horizon are expired so the
/// pool never bloats with stale states ( Architecture #8's storage cousin).
pub fn expire_stale(conn: &Connection, before_rfc3339: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "UPDATE thought_stream SET state = ?2 WHERE state = ?1 AND created_at < ?3",
            rusqlite::params![STATE_PENDING, STATE_EXPIRED, before_rfc3339],
        )
        .map_err(|e| format!("Failed to expire stale seeds: {}", e))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    fn seed(id: &str, stimulus: &str, salience: f64) -> ThoughtSeed {
        ThoughtSeed {
            id: id.to_string(),
            stimulus: stimulus.to_string(),
            emotion_tone: Some("好奇".into()),
            relation_hint: None,
            origin: "environment".to_string(),
            salience,
            created_at: "2026-08-28T10:00:00+00:00".to_string(),
            state: STATE_PENDING.to_string(),
            unspoken_reason: None,
            voiced_at: None,
            evolved_from: None,
        }
    }

    #[test]
    fn crud_roundtrip_and_ordering() {
        let db = test_db();
        db.with_conn(|conn| {
            insert(conn, &seed("s1", "用户连续编辑 main.rs", 0.4))?;
            insert(conn, &seed("s2", "心里有点空", 0.8))?;
            let pending = get_pending(conn, 10)?;
            assert_eq!(pending.len(), 2);
            assert_eq!(pending[0].id, "s2", "highest salience first");
            assert!(pending_stimulus_exists(conn, "用户连续编辑 main.rs")?);
            assert!(!pending_stimulus_exists(conn, "不存在")?);
            mark_voiced(conn, "s2", "2026-08-28T12:00:00+00:00")?;
            mark_unspoken(conn, "s1", "刚聊过没多久")?;
            assert_eq!(count_pending(conn)?, 0);
            Ok::<_, String>(())
        })
        .unwrap();
    }

    #[test]
    fn stale_expiry_only_touches_pending() {
        let db = test_db();
        db.with_conn(|conn| {
            let mut old = seed("s1", "旧的", 0.5);
            old.created_at = "2026-08-01T00:00:00+00:00".to_string();
            insert(conn, &old)?;
            let mut voiced = seed("s2", "已说的", 0.9);
            voiced.created_at = "2026-08-01T00:00:00+00:00".to_string();
            voiced.state = STATE_VOICED.to_string();
            insert(conn, &voiced)?;
            let n = expire_stale(conn, "2026-08-27T00:00:00+00:00")?;
            assert_eq!(n, 1, "only the stale pending seed expires");
            assert_eq!(count_pending(conn)?, 0);
            assert!(get_pending(conn, 10)?.is_empty());
            Ok::<_, String>(())
        })
        .unwrap();
    }
}
