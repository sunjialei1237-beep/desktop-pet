-- Migration v7 (Environment + Filesystem plan, 2026-08-17): resource-level
-- filesystem grants. Authorization state for the Observe/Inspect tools:
-- which directory roots the pet may read, granted how (once / project /
-- always / deny). Capability-level switches stay in config.toml [tools]
-- (Principle 6); this table is the runtime state produced by conversational
-- consent (plan §2.7). One row per root — upsert replaces on re-grant.
CREATE TABLE IF NOT EXISTS fs_grants (
    root       TEXT PRIMARY KEY,
    mode       TEXT NOT NULL CHECK (mode IN ('once', 'project', 'always', 'deny')),
    created_at TEXT NOT NULL,
    source     TEXT NOT NULL DEFAULT 'conversation'
);
INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (7, datetime('now'));
