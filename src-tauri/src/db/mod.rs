pub mod connection;
pub mod schema;
pub mod conversations;
pub mod episodes;
pub mod facts;
pub mod persona;
pub mod relationship;
pub mod emotion;
pub mod pending;
pub mod reflections;
pub mod relationship_reviews;
pub mod vectors;
pub mod changelog;
pub mod onboarding;

use rusqlite::Connection;
use std::sync::Mutex;

/// Shared database state. Thread-safe via Mutex.
/// One connection with WAL mode is sufficient for a desktop app.
pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    /// Opens (or creates) the database at the given path, runs migrations.
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = connection::open(path)?;
        schema::run_migrations(&conn)?;
        Ok(DbState {
            conn: Mutex::new(conn),
        })
    }

    /// Acquires the connection lock for a database operation.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&Connection) -> Result<T, String>,
    {
        let conn = self.conn.lock().map_err(|e| format!("DB lock error: {}", e))?;
        f(&conn)
    }
}

pub mod test_utils {
    use super::*;

    /// Creates an in-memory database for testing.
    pub fn test_db() -> DbState {
        let conn = connection::open_in_memory().unwrap();
        schema::run_migrations(&conn).unwrap();
        DbState {
            conn: Mutex::new(conn),
        }
    }
}
