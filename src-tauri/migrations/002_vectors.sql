-- Migration v2: Add episode_vectors table for semantic retrieval
-- (existing v1 databases need this; fresh databases get it in 001_init.sql)

CREATE TABLE IF NOT EXISTS episode_vectors (
    episode_id  TEXT PRIMARY KEY,
    embedding   BLOB NOT NULL,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (episode_id) REFERENCES episodes(id)
);

INSERT OR REPLACE INTO schema_migrations (version, applied_at)
    VALUES (2, datetime('now'));
