-- Migration v3: relationship_reviews table.
-- Periodic LLM-generated summaries of how the relationship with the user is
-- progressing (Hermes-style background review). Stored separately from
-- episodes so it does not pollute event retrieval — a review is a
-- relationship-level synthesis, not a single event.

CREATE TABLE IF NOT EXISTS relationship_reviews (
    id          TEXT PRIMARY KEY,
    summary     TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

INSERT OR REPLACE INTO schema_migrations (version, applied_at)
    VALUES (3, datetime('now'));
