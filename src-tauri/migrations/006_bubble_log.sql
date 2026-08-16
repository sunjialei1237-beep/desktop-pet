-- Migration v6 (LLM anchor selector, 2026-08-16 续⁴¹): proactive-bubble log.
-- The speaker's own memory of what she last said unprompted. Every proactive
-- bubble outcome (memory / lively / due / welcome-back / lonely) appends a row;
-- the next surfacing decision and voicing prompt read the recent rows back so
-- she never repeats herself across bubbles and always knows how long it has
-- been since she last spoke. Rust writes/reads all state (Principle #1) — the
-- LLM only consumes the injected lines.
CREATE TABLE IF NOT EXISTS bubble_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    time          TEXT NOT NULL,
    kind          TEXT NOT NULL,
    text          TEXT NOT NULL,
    anchor        TEXT NOT NULL DEFAULT '',
    anchor_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_bubble_log_time ON bubble_log(time DESC);
INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (6, datetime('now'));
