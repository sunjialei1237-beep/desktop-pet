//! Filesystem grants (plan 2026-08-17 §2.7): resource-level authorization
//! state for the Observe/Inspect tools. One row per canonical root path.
//!
//! Storage boundary (the "two stores" ruling): capability-level switches
//! live in config.toml `[tools]` (Principle 6, Settings-editable); this
//! table holds only what conversational consent produces — which roots are
//! readable, granted how, and explicit denials (with a re-ask cooldown so
//! the pet never nags after a "no").

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// How a root is authorized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    /// Valid for the current authorization interaction only.
    Once,
    /// Persistent grant for this root.
    Project,
    /// Persistent grant (alias of project — kept for schema compatibility).
    Always,
    /// Explicit refusal; re-asking is cooled down (DENY_REASK_COOLDOWN).
    Deny,
}

impl GrantMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Project => "project",
            Self::Always => "always",
            Self::Deny => "deny",
        }
    }
}

/// After an explicit deny, do not re-ask about the same root for 24h
/// (annoying the user damages trust more than missing a read).
pub const DENY_REASK_COOLDOWN_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize)]
pub struct FsGrant {
    pub root: String,
    pub mode: String,
    pub created_at: String,
    pub source: String,
}

/// Insert or replace the grant for `root` (one row per root).
pub fn upsert(conn: &Connection, root: &str, mode: GrantMode, source: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO fs_grants (root, mode, created_at, source)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(root) DO UPDATE SET mode=?2, created_at=?3, source=?4",
        params![root, mode.as_str(), chrono::Utc::now().to_rfc3339(), source],
    )
    .map_err(|e| format!("Failed to upsert fs grant: {}", e))?;
    Ok(())
}

/// Exact-match lookup by root.
pub fn get(conn: &Connection, root: &str) -> Result<Option<FsGrant>, String> {
    conn.query_row(
        "SELECT root, mode, created_at, source FROM fs_grants WHERE root = ?1",
        params![root],
        |row| {
            Ok(FsGrant {
                root: row.get(0)?,
                mode: row.get(1)?,
                created_at: row.get(2)?,
                source: row.get(3)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("Failed to query fs grant: {}", other)),
    })
}

/// All grants (audit / Settings display).
pub fn list(conn: &Connection) -> Result<Vec<FsGrant>, String> {
    let mut stmt = conn
        .prepare("SELECT root, mode, created_at, source FROM fs_grants ORDER BY created_at DESC")
        .map_err(|e| format!("Failed to prepare fs grants: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FsGrant {
                root: row.get(0)?,
                mode: row.get(1)?,
                created_at: row.get(2)?,
                source: row.get(3)?,
            })
        })
        .map_err(|e| format!("Failed to query fs grants: {}", e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Remove a grant (revoke from Settings / user request).
pub fn revoke(conn: &Connection, root: &str) -> Result<(), String> {
    conn.execute("DELETE FROM fs_grants WHERE root = ?1", params![root])
        .map_err(|e| format!("Failed to revoke fs grant: {}", e))?;
    Ok(())
}

/// Whether a deny recorded in `created_at` (RFC3339) is still inside the
/// re-ask cooldown window.
pub fn deny_in_cooldown(created_at: &str) -> bool {
    match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(ts) => {
            let age = chrono::Utc::now()
                .signed_duration_since(ts)
                .num_seconds()
                .max(0) as u64;
            age < DENY_REASK_COOLDOWN_SECS
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
        )
        .unwrap();
        c.execute_batch(include_str!("../../migrations/007_fs_grants.sql"))
            .unwrap();
        c
    }

    #[test]
    fn upsert_replaces_same_root() {
        let c = conn();
        upsert(&c, "D:\\Projects\\Liri", GrantMode::Once, "conversation").unwrap();
        upsert(&c, "D:\\Projects\\Liri", GrantMode::Project, "conversation").unwrap();
        let g = get(&c, "D:\\Projects\\Liri").unwrap().unwrap();
        assert_eq!(g.mode, "project");
        assert_eq!(list(&c).unwrap().len(), 1);
    }

    #[test]
    fn get_missing_returns_none() {
        let c = conn();
        assert!(get(&c, "D:\\nope").unwrap().is_none());
    }

    #[test]
    fn revoke_removes() {
        let c = conn();
        upsert(&c, "D:\\Projects\\Liri", GrantMode::Always, "settings").unwrap();
        revoke(&c, "D:\\Projects\\Liri").unwrap();
        assert!(get(&c, "D:\\Projects\\Liri").unwrap().is_none());
    }

    #[test]
    fn deny_cooldown_fresh_vs_stale() {
        let fresh = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let stale = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
        assert!(deny_in_cooldown(&fresh));
        assert!(!deny_in_cooldown(&stale));
        assert!(!deny_in_cooldown("not-a-date"));
    }
}
