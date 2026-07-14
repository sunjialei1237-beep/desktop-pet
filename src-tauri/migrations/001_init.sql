-- P1: Full schema definition for desktop pet memory system
-- 8-layer memory + auxiliary tables per design doc v2

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Schema migration tracking (principle A6: version control)
CREATE TABLE IF NOT EXISTS schema_migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL
);

-- Conversation log (source traceability)
CREATE TABLE IF NOT EXISTS conversations (
    id              TEXT PRIMARY KEY,
    turn            INTEGER NOT NULL,
    role            TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_conv_created ON conversations(created_at);

-- Episodic Memory (design doc 5.2 Episode structure)
CREATE TABLE IF NOT EXISTS episodes (
    id                      TEXT PRIMARY KEY,
    time                    TEXT NOT NULL,
    summary                 TEXT NOT NULL,
    emotion                 TEXT,
    importance              REAL NOT NULL DEFAULT 0.5,
    is_landmark             INTEGER NOT NULL DEFAULT 0,
    subject                 TEXT NOT NULL DEFAULT 'user',
    participants            TEXT,
    topics                  TEXT,
    source_type             TEXT NOT NULL DEFAULT 'conversation',
    source_conversation_id  TEXT,
    source_turn             INTEGER,
    memory_strength         REAL NOT NULL,
    recall_count            INTEGER NOT NULL DEFAULT 0,
    last_recalled_at        TEXT,
    consolidated            INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ep_importance ON episodes(importance);
CREATE INDEX IF NOT EXISTS idx_ep_strength ON episodes(memory_strength);
CREATE INDEX IF NOT EXISTS idx_ep_time ON episodes(time);

-- Semantic Memory / Facts (design doc 5.2 Fact structure + temporal validity)
CREATE TABLE IF NOT EXISTS facts (
    id              TEXT PRIMARY KEY,
    category        TEXT NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,
    valid_from      TEXT,
    valid_to        TEXT,
    source_episode  TEXT,
    mention_count   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    FOREIGN KEY (source_episode) REFERENCES episodes(id),
    UNIQUE(category, key, value)
);
CREATE INDEX IF NOT EXISTS idx_facts_cat ON facts(category, key);
CREATE INDEX IF NOT EXISTS idx_facts_valid ON facts(valid_to);

-- Persona: Traits (user impression, low-frequency updates)
CREATE TABLE IF NOT EXISTS persona_traits (
    id          TEXT PRIMARY KEY,
    trait_type  TEXT NOT NULL,
    trait_key   TEXT NOT NULL,
    confidence  REAL NOT NULL DEFAULT 0.5,
    source      TEXT NOT NULL DEFAULT 'reflection',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE(trait_type, trait_key)
);

-- Persona: Relationship (design doc 10.2 Relationship Pace, singleton)
CREATE TABLE IF NOT EXISTS relationship (
    id                      INTEGER PRIMARY KEY CHECK(id = 1),
    closeness               REAL NOT NULL DEFAULT 0.0,
    trust                   REAL NOT NULL DEFAULT 0.0,
    days_known              INTEGER NOT NULL DEFAULT 0,
    total_conversations     INTEGER NOT NULL DEFAULT 0,
    shared_events           INTEGER NOT NULL DEFAULT 0,
    last_interaction_at     TEXT,
    last_interaction_type   TEXT,
    closeness_log           TEXT,
    updated_at              TEXT NOT NULL
);

-- Emotion (design doc 11.1 + 7.7 Homeostasis + 7.8 Needs, singleton)
CREATE TABLE IF NOT EXISTS emotion_state (
    id                  INTEGER PRIMARY KEY CHECK(id = 1),
    mood                REAL NOT NULL DEFAULT 0.5,
    mood_label          TEXT NOT NULL DEFAULT 'ping jing',
    physical_energy     REAL NOT NULL DEFAULT 0.7,
    social_battery      REAL NOT NULL DEFAULT 0.8,
    stress              REAL NOT NULL DEFAULT 0.2,
    loneliness          REAL NOT NULL DEFAULT 0.0,
    rest_need           REAL NOT NULL DEFAULT 0.0,
    bl_mood             REAL NOT NULL DEFAULT 0.5,
    bl_energy           REAL NOT NULL DEFAULT 0.7,
    bl_social           REAL NOT NULL DEFAULT 0.8,
    bl_stress           REAL NOT NULL DEFAULT 0.2,
    last_homeostasis_at TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

-- Pending Events (design doc 5.6)
CREATE TABLE IF NOT EXISTS pending_events (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    event_date      TEXT NOT NULL,
    remind_date     TEXT,
    source_episode  TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    importance      REAL NOT NULL DEFAULT 0.5,
    followup_count  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    triggered_at    TEXT,
    resolved_at     TEXT,
    FOREIGN KEY (source_episode) REFERENCES episodes(id)
);
CREATE INDEX IF NOT EXISTS idx_pending_status ON pending_events(status);
CREATE INDEX IF NOT EXISTS idx_pending_remind ON pending_events(remind_date);

-- Reflections (design doc 5.1 Reflection + 7.1 Internal Monologue)
CREATE TABLE IF NOT EXISTS reflections (
    id              TEXT PRIMARY KEY,
    trigger_type    TEXT NOT NULL,
    trigger_reason  TEXT,
    thought         TEXT NOT NULL,
    persona_updates TEXT,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS internal_thoughts (
    id                  TEXT PRIMARY KEY,
    content             TEXT NOT NULL,
    emotion             TEXT,
    source_reflection   TEXT,
    surfacing_type      TEXT NOT NULL DEFAULT 'next_interaction',
    created_at          TEXT NOT NULL,
    surfaced_at         TEXT,
    FOREIGN KEY (source_reflection) REFERENCES reflections(id)
);

-- App Config (runtime key-value store)
CREATE TABLE IF NOT EXISTS app_config (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Change Log (principle A4: lightweight event sourcing for Debug Panel)
CREATE TABLE IF NOT EXISTS change_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL,
    module      TEXT NOT NULL,
    action      TEXT NOT NULL,
    target      TEXT,
    field       TEXT,
    old_value   TEXT,
    new_value   TEXT,
    reason      TEXT
);

-- Initialize singleton rows
INSERT OR IGNORE INTO relationship (id, updated_at) VALUES (1, datetime('now'));
INSERT OR IGNORE INTO emotion_state (id, last_homeostasis_at, updated_at)
    VALUES (1, datetime('now'), datetime('now'));

-- Record migration
INSERT OR REPLACE INTO schema_migrations (version, applied_at)
    VALUES (1, datetime('now'));
