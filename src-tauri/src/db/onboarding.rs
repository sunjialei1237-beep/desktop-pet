use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Onboarding profile captured at first launch. Persisted as 4 rows in
/// `app_config`; injected into the system prompt's [Persona] section so the
/// pet addresses the user by their chosen nickname, adopts the requested
/// personality, etc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_nickname: Option<String>,
    pub pet_name: Option<String>,
    pub personality_style: Option<String>,
    pub relationship_style: Option<String>,
}

fn read_config(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("read app_config {}: {}", key, e))
}

pub fn load(conn: &Connection) -> Result<UserProfile, String> {
    Ok(UserProfile {
        user_nickname: read_config(conn, "user_nickname")?,
        pet_name: read_config(conn, "pet_name")?,
        personality_style: read_config(conn, "personality_style")?,
        relationship_style: read_config(conn, "relationship_style")?,
    })
}

pub fn save(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        rusqlite::params![key, value, now],
    )
    .map_err(|e| format!("save app_config {}: {}", key, e))?;
    Ok(())
}

pub fn needs_onboarding(conn: &Connection) -> Result<bool, String> {
    let v = read_config(conn, "onboard_completed")?;
    Ok(v.as_deref() != Some("true"))
}
