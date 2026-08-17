use rusqlite::Connection;

/// Runs all pending migrations (v1 init + v2 episode_vectors).
/// Future versions will branch on the recorded version number.
pub fn run_migrations(conn: &Connection) -> Result<(), String> {
    let current_version = get_schema_version(conn)?;

    // NOTE: this guard must match the LATEST version — an early-return pinned
    // to an older version silently skips every migration added after it (the
    // v6 bubble_log migration was unreachable on real v5 DBs; caught by the
    // 2026-08-16 selector smoke, lib tests never saw it because fresh test
    // DBs run the full chain from version 0).
    if current_version >= 7 {
        log::info!("Database schema at version {}, no migration needed", current_version);
        return Ok(());
    }

    if current_version < 1 {
        log::info!("Running migration v1 (init)...");
        let sql = include_str!("../../migrations/001_init.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v1 failed: {}", e))?;
        log::info!("Migration v1 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 2 {
        log::info!("Running migration v2 (episode_vectors)...");
        let sql = include_str!("../../migrations/002_vectors.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v2 failed: {}", e))?;
        log::info!("Migration v2 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 3 {
        log::info!("Running migration v3 (relationship_reviews)...");
        let sql = include_str!("../../migrations/003_relationship_reviews.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v3 failed: {}", e))?;
        log::info!("Migration v3 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 4 {
        log::info!("Running migration v4 (companion memory)...");
        let sql = include_str!("../../migrations/004_companion_memory.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v4 failed: {}", e))?;
        log::info!("Migration v4 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 5 {
        log::info!("Running migration v5 (fact surfacing ledger)...");
        let sql = include_str!("../../migrations/005_fact_surfacing.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v5 failed: {}", e))?;
        log::info!("Migration v5 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 6 {
        log::info!("Running migration v6 (proactive bubble log)...");
        let sql = include_str!("../../migrations/006_bubble_log.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v6 failed: {}", e))?;
        log::info!("Migration v6 applied successfully");
    }

    let current_version = get_schema_version(conn)?;
    if current_version < 7 {
        log::info!("Running migration v7 (fs_grants)...");
        let sql = include_str!("../../migrations/007_fs_grants.sql");
        conn.execute_batch(sql)
            .map_err(|e| format!("Migration v7 failed: {}", e))?;
        log::info!("Migration v7 applied successfully");
    }

    Ok(())
}

/// Returns the highest applied schema version, or 0 if none.
fn get_schema_version(conn: &Connection) -> Result<i64, String> {
    // Check if schema_migrations table exists
    let exists: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to check schema: {}", e))?;

    if exists == 0 {
        return Ok(0);
    }

    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get schema version: {}", e))?;

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_runs_once() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 7);

        // Running again should be a no-op
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 7);
    }

    #[test]
    fn test_v5_database_migrates_to_v6() {
        // Regression: a real DB stamped at version 5 (no bubble_log table) must
        // run the v6 migration — the early-return guard used to be pinned to 5
        // and skipped it entirely.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations (version, applied_at) VALUES (5, datetime('now'));",
        )
        .unwrap();
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 7);
        let has_table: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='bubble_log'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_table, 1, "bubble_log must exist after v5→v6 migration");
    }

    #[test]
    fn test_v6_database_migrates_to_v7() {
        // Same regression class as v5→v6: a real DB stamped at version 6 must
        // run the v7 migration.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations (version, applied_at) VALUES (6, datetime('now'));",
        )
        .unwrap();
        run_migrations(&conn).unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 7);
        let has_table: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='fs_grants'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_table, 1, "fs_grants must exist after v6→v7 migration");
    }

    #[test]
    fn test_all_tables_created() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let expected = [
            "schema_migrations",
            "conversations",
            "episodes",
            "facts",
            "persona_traits",
            "relationship",
            "emotion_state",
            "pending_events",
            "reflections",
            "internal_thoughts",
            "app_config",
            "change_log",
            "episode_vectors",
            "relationship_reviews",
            "bubble_log",
            "fs_grants",
        ];

        for table in &expected {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Table '{}' should exist", table);
        }
    }

    #[test]
    fn test_singletons_initialized() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let rel_count: i64 = conn
            .query_row("SELECT count(*) FROM relationship WHERE id=1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rel_count, 1, "Relationship singleton should exist");

        let emo_count: i64 = conn
            .query_row("SELECT count(*) FROM emotion_state WHERE id=1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(emo_count, 1, "Emotion singleton should exist");
    }
}
