-- Migration v4 (companion memory): pet promise origin + episode emotion anchor.
-- ADD COLUMN must live ONLY here (not back-ported into 001): SQLite has no
-- IF NOT EXISTS for columns, and fresh databases run 001 -> 004 in order anyway.
ALTER TABLE pending_events ADD COLUMN origin TEXT NOT NULL DEFAULT 'user';
ALTER TABLE episodes ADD COLUMN emotion_anchor TEXT;
INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (4, datetime('now'));
