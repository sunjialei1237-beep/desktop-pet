-- Migration v5 (proactive bubble governance): fact surfacing ledger.
-- Tracks how many times / when a fact was proactively surfaced, so anchor
-- selection can hard-exclude recently voiced memories (round-robin, never
-- repeat within the surface window). See docs/plans/2026-08-14-proactive-bubble-governance.md
-- ADD COLUMN must live ONLY here: SQLite has no IF NOT EXISTS for columns.
ALTER TABLE facts ADD COLUMN surfaced_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE facts ADD COLUMN last_surfaced_at TEXT;
INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (5, datetime('now'));
