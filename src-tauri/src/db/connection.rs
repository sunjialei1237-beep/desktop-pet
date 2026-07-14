use rusqlite::Connection;
use std::path::Path;

/// Opens a database connection with WAL mode and foreign keys enabled.
/// Creates the parent directory if it does not exist.
pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create DB directory: {}", e))?;
    }

    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    // Enable WAL mode for crash safety and concurrent reads (principle A5)
    conn.execute_batch("PRAGMA journal_mode = WAL;")
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

    // Enable foreign keys
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    Ok(conn)
}

/// Opens an in-memory database for testing.
pub fn open_in_memory() -> Result<Connection, String> {
    let conn = Connection::open_in_memory()
        .map_err(|e| format!("Failed to open in-memory database: {}", e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;
    Ok(conn)
}
