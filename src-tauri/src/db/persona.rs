use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaTrait {
    pub id: String,
    pub trait_type: String,
    pub trait_key: String,
    pub confidence: f64,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Upserts a trait. If (trait_type, trait_key) already exists, updates confidence and source.
pub fn upsert_trait(conn: &Connection, t: &PersonaTrait) -> Result<(), String> {
    conn.execute(
        "INSERT INTO persona_traits (id, trait_type, trait_key, confidence, source, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(trait_type, trait_key) DO UPDATE SET
            confidence = ?4, source = ?5, updated_at = ?7",
        params![
            t.id, t.trait_type, t.trait_key, t.confidence,
            t.source, t.created_at, t.updated_at,
        ],
    )
    .map_err(|e| format!("Failed to upsert trait: {}", e))?;
    Ok(())
}

/// Gets all traits of a given type (e.g. "core", "adaptive").
pub fn get_traits_by_type(conn: &Connection, trait_type: &str) -> Result<Vec<PersonaTrait>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, trait_type, trait_key, confidence, source, created_at, updated_at
             FROM persona_traits WHERE trait_type = ?1
             ORDER BY confidence DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map(params![trait_type], |row| {
            Ok(PersonaTrait {
                id: row.get(0)?,
                trait_type: row.get(1)?,
                trait_key: row.get(2)?,
                confidence: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query traits: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
}

/// Gets all traits regardless of type, ordered by confidence descending.
pub fn get_all_traits(conn: &Connection) -> Result<Vec<PersonaTrait>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, trait_type, trait_key, confidence, source, created_at, updated_at
             FROM persona_traits
             ORDER BY confidence DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PersonaTrait {
                id: row.get(0)?,
                trait_type: row.get(1)?,
                trait_key: row.get(2)?,
                confidence: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query traits: {}", e))?;

    rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe(Ok)
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
    fn test_upsert_and_query() {
        let db = test_db();
        db.with_conn(|conn| {
            upsert_trait(conn, &PersonaTrait {
                id: "t1".to_string(),
                trait_type: "core".to_string(),
                trait_key: "gentle".to_string(),
                confidence: 0.9,
                source: "design".to_string(),
                created_at: "2026-07-14T10:00:00".to_string(),
                updated_at: "2026-07-14T10:00:00".to_string(),
            })?;

            // Upsert again with different confidence
            upsert_trait(conn, &PersonaTrait {
                id: "t2".to_string(),
                trait_type: "core".to_string(),
                trait_key: "gentle".to_string(),
                confidence: 0.95,
                source: "reflection".to_string(),
                created_at: "2026-07-14T10:00:00".to_string(),
                updated_at: "2026-07-14T12:00:00".to_string(),
            })?;

            let traits = get_traits_by_type(conn, "core")?;
            assert_eq!(traits.len(), 1, "upsert should not create duplicate");
            assert!((traits[0].confidence - 0.95).abs() < 0.001);
            assert_eq!(traits[0].source, "reflection");
            Ok(())
        })
        .unwrap();
    }
}
