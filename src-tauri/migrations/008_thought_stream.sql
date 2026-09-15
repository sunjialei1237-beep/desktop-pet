-- v8: thought_stream — 她的持续心理状态池（thought-stream plan v3 §2.1）。
-- 种子是结构化心理状态 {stimulus, emotion_tone, relation_hint}，不是台词；
-- 台词只在统一渲染器发声时刻诞生。state 含 unspoken（想说但忍住了，
-- salience 保留，可被次日时间触发器强化——"她记得"的原料）。
CREATE TABLE IF NOT EXISTS thought_stream (
    id TEXT PRIMARY KEY,
    stimulus TEXT NOT NULL,
    emotion_tone TEXT,
    relation_hint TEXT,
    origin TEXT NOT NULL,
    salience REAL NOT NULL DEFAULT 0.5,
    created_at TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    unspoken_reason TEXT,
    voiced_at TEXT,
    evolved_from TEXT
);
CREATE INDEX IF NOT EXISTS idx_thought_stream_state
    ON thought_stream(state, salience DESC);
INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (8, datetime('now'));
