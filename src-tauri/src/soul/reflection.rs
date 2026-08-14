//! Reflection engine: collects recent interactions, calls LLM to reflect,
//! and persists persona updates + internal thoughts.
//!
//! Principle 1 (LLM expresses, Rust maintains state):
//!   The LLM returns JSON with suggested trait updates and thoughts.
//!   Rust code validates and writes them to the DB.
//! Principle 8 (Cost): Uses reflection_model, runs at most once daily.
//! Principle 11 (Explainability): Every reflection records trigger + reason + thought.

use crate::db::DbState;
use crate::db::reflections::{InternalThought, Reflection};
use crate::llm::client::{ChatMessage, LlmClient};
use serde::{Deserialize, Serialize};

/// Event-driven reflection cooldown (TurnThreshold/MajorEvent), independent of
/// the 20h Daily cycle. Caps event-driven frequency (Architecture Principle 8).
const EVENT_REFLECTION_COOLDOWN_HOURS: i64 = 1;
/// Conversation episodes accumulated since the last reflection that trigger a
/// TurnThreshold run (design 5.9 / Tier2 #5). Only episodes that actually became
/// memories count — gate-blocked small talk does not.
const TURN_THRESHOLD_EPISODES: i64 = 30;
/// A single episode above this importance since the last reflection triggers a
/// MajorEvent run (something significant happened, worth reflecting on now).
const MAJOR_EVENT_IMPORTANCE: f64 = 0.85;

/// What triggered this reflection run.
#[derive(Debug, Clone)]
pub enum ReflectionTrigger {
    Daily,
    TurnThreshold,
    MajorEvent,
}

impl ReflectionTrigger {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::TurnThreshold => "turn_threshold",
            Self::MajorEvent => "major_event",
        }
    }
}

/// Outcome of a reflection run.
#[derive(Debug, Clone)]
pub struct ReflectionResult {
    pub reflection_id: String,
    pub summary: String,
    pub new_trait_count: usize,
    pub new_thought_count: usize,
}

/// LLM output schema (parsed from JSON).
#[derive(Debug, Deserialize)]
struct LlmReflectionOutput {
    #[serde(default)]
    new_traits: Vec<LlmTrait>,
    #[serde(default)]
    internal_thoughts: Vec<LlmThought>,
    #[serde(default)]
    reflection: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct LlmTrait {
    trait_key: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct LlmThought {
    content: String,
    #[serde(default)]
    emotion: Option<String>,
    #[serde(default = "default_surfacing")]
    surfacing_type: String,
}

fn default_confidence() -> f64 {
    0.5
}
fn default_surfacing() -> String {
    "next_interaction".to_string()
}

/// Runs a single reflection cycle.
pub async fn run_reflection(
    trigger: ReflectionTrigger,
    db: &DbState,
    llm: &LlmClient,
) -> Result<ReflectionResult, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let reflection_id = format!("ref_{}", chrono::Utc::now().timestamp_millis());

    // 1. Gather context from DB.
    let (episodes_text, facts_text, persona_text, emotion_text) = db.with_conn(|conn| {
        let episodes = crate::db::episodes::get_recent(conn, 24)?;
        let episodes_text = episodes.iter().map(|e| format!("- {}", e.summary)).collect::<Vec<_>>().join("\n");
        let facts = crate::db::facts::get_all_active(conn, 30)?;
        let facts_text = facts.iter().map(|f| format!("- {}: {}", f.key, f.value)).collect::<Vec<_>>().join("\n");
        let traits = crate::db::persona::get_all_traits(conn)?;
        let persona_text = traits.iter().map(|t| format!("- {}", t.trait_key)).collect::<Vec<_>>().join("\n");
        let emo = crate::db::emotion::get(conn)?;
        let emotion_text = format!("mood={}, energy={}, stress={}, loneliness={}", emo.mood_label, emo.physical_energy, emo.stress, emo.loneliness);
        Ok::<_, String>((episodes_text, facts_text, persona_text, emotion_text))
    })?;

    // 2. Build prompt from template.
    let prompt_template = load_prompt_template();
    let system_prompt = prompt_template
        .replace("{episodes}", &episodes_text)
        .replace("{facts}", &facts_text)
        .replace("{persona}", &persona_text)
        .replace("{emotion}", &emotion_text);

    // 3. Call LLM with reflection model.
    let messages = vec![ChatMessage::system(system_prompt)];
    let result = llm.chat_reflection(&messages, Some(0.7), Some(4096)).await
        .map_err(|e| format!("Reflection LLM call failed: {}", e))?;

    // 4. Parse LLM output.
    let cleaned = clean_json(&result.content);
    let parsed: LlmReflectionOutput = serde_json::from_str(&cleaned)
        .map_err(|e| format!("Failed to parse reflection output: {}", e))?;

    // 5. Persist results to DB.
    let new_trait_count = parsed.new_traits.len();
    let new_thought_count = parsed.internal_thoughts.len();

    db.with_conn(|conn| {
        for t in &parsed.new_traits {
            let confidence = t.confidence.clamp(0.0, 1.0);
            crate::db::persona::upsert_trait(conn, &crate::db::persona::PersonaTrait {
                id: format!("trait_{}", uuid::Uuid::new_v4()),
                trait_type: "adaptive".to_string(),
                trait_key: t.trait_key.clone(),
                confidence,
                source: "reflection".to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
            })?;
        }
        // Order matters under PRAGMA foreign_keys = ON: insert the reflection
        // PARENT record first, then the thoughts that reference it via
        // source_reflection (FK -> reflections.id). The previous order (thoughts
        // before reflection) raised "FOREIGN KEY constraint failed" at runtime.
        let persona_json = serde_json::to_string(&parsed.new_traits).ok();
        crate::db::reflections::insert_reflection(conn, &Reflection {
            id: reflection_id.clone(),
            trigger_type: trigger.as_str().to_string(),
            trigger_reason: Some(parsed.reflection.clone()),
            thought: parsed.reflection.clone(),
            persona_updates: persona_json,
            created_at: now.clone(),
        })?;
        for th in &parsed.internal_thoughts {
            crate::db::reflections::insert_thought(conn, &InternalThought {
                id: format!("thought_{}", uuid::Uuid::new_v4()),
                content: th.content.clone(),
                emotion: th.emotion.clone(),
                source_reflection: Some(reflection_id.clone()),
                surfacing_type: th.surfacing_type.clone(),
                created_at: now.clone(),
                surfaced_at: None,
            })?;
        }
        // `now` consumed by the last insert above; no further use.
        drop(now);
        Ok::<_, String>(())
    })?;

    log::info!("Reflection complete: {} traits, {} thoughts, trigger={}", new_trait_count, new_thought_count, trigger.as_str());
    Ok(ReflectionResult { reflection_id, summary: parsed.reflection, new_trait_count, new_thought_count })
}

/// Timestamp of the most recent reflection (None if none recorded). Pure.
fn last_reflection_at(db: &DbState) -> Option<chrono::DateTime<chrono::Utc>> {
    let last: Option<String> = db
        .with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT MAX(created_at) FROM reflections",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .unwrap_or(None))
        })
        .unwrap_or(None);
    last.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    })
}

/// Whether enough time (>20h) has passed since the last reflection to run
/// another Daily cycle. Returns true when there is no prior reflection.
///
/// Pure (sync, no LLM) so the cooldown logic is unit-testable independently of
/// the LLM call. Architecture Principle 8: Reflection runs at most once daily.
pub fn should_run_reflection(db: &DbState) -> bool {
    match last_reflection_at(db) {
        None => true,
        Some(last) => (chrono::Utc::now() - last).num_hours() > 20,
    }
}

/// Whether a TurnThreshold reflection is due: at least `EVENT_REFLECTION_COOLDOWN_HOURS`
/// since the last reflection AND >= `TURN_THRESHOLD_EPISODES` conversation episodes
/// accumulated since. The 1h cooldown is independent of the 20h Daily cycle so a
/// chatty day can trigger one extra reflection, while the ceiling caps cost (#8).
/// Pure (sync, no LLM) for unit testing.
pub fn should_run_turn_threshold(db: &DbState) -> bool {
    let last = match last_reflection_at(db) {
        None => return true, // never reflected -> eligible
        Some(t) => t,
    };
    if (chrono::Utc::now() - last).num_hours() < EVENT_REFLECTION_COOLDOWN_HOURS {
        return false;
    }
    let since = last.to_rfc3339();
    let count: i64 = db
        .with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT COUNT(*) FROM episodes WHERE source_type='conversation' AND created_at > ?1",
                    rusqlite::params![since],
                    |row| row.get(0),
                )
                .unwrap_or(0))
        })
        .unwrap_or(0);
    count >= TURN_THRESHOLD_EPISODES
}

/// Whether a MajorEvent reflection is due: at least `EVENT_REFLECTION_COOLDOWN_HOURS`
/// since the last reflection AND some episode above `MAJOR_EVENT_IMPORTANCE` has
/// arrived since (something significant happened). Pure (sync, no LLM) for unit testing.
pub fn should_run_major_event(db: &DbState) -> bool {
    let last = match last_reflection_at(db) {
        None => return true,
        Some(t) => t,
    };
    if (chrono::Utc::now() - last).num_hours() < EVENT_REFLECTION_COOLDOWN_HOURS {
        return false;
    }
    let since = last.to_rfc3339();
    let count: i64 = db
        .with_conn(|conn| {
            Ok(conn
                .query_row(
                    "SELECT COUNT(*) FROM episodes WHERE importance > ?1 AND created_at > ?2",
                    rusqlite::params![MAJOR_EVENT_IMPORTANCE, since],
                    |row| row.get(0),
                )
                .unwrap_or(0))
        })
        .unwrap_or(0);
    count >= 1
}

/// Runs a reflection if any trigger is due — priority: Daily (>20h cooldown),
/// then MajorEvent (>=1 high-importance episode, 1h cooldown), then TurnThreshold
/// (>=30 conversation episodes, 1h cooldown). At most one runs per call.
/// Returns true when a reflection ran, false when all skipped.
/// Errors propagate so callers (IPC command, life loop) can decide how to
/// handle them — the command layer swallows them to keep the frontend contract.
///
/// Pure signature (no AppState/Tauri State): callable from both the IPC
/// command and the life-loop slow tick (Architecture Principle 1).
pub async fn maybe_run_if_due(
    db: &DbState,
    llm: &LlmClient,
) -> Result<bool, String> {
    let trigger = if should_run_reflection(db) {
        ReflectionTrigger::Daily
    } else if should_run_major_event(db) {
        ReflectionTrigger::MajorEvent
    } else if should_run_turn_threshold(db) {
        ReflectionTrigger::TurnThreshold
    } else {
        return Ok(false);
    };
    let label = trigger.as_str();
    let r = run_reflection(trigger, db, llm).await?;
    log::info!("Reflection ran ({}): {}", label, r.summary);
    Ok(true)
}

/// Loads the reflection prompt template, trying multiple paths.
fn load_prompt_template() -> String {
    for p in &["src-tauri/resources/prompts/reflection.txt", "resources/prompts/reflection.txt"] {
        if let Ok(content) = std::fs::read_to_string(p) {
            return content;
        }
    }
    "{\"new_traits\":[],\"internal_thoughts\":[],\"reflection\":\"\"}".to_string()
}

/// Strips markdown code fences and trims whitespace.
fn clean_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 3 {
            return lines[1..lines.len() - 1].join("\n").trim().to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::reflections::{insert_reflection, Reflection};
    use crate::db::test_utils::test_db;

    #[test]
    fn test_clean_json_plain() {
        let input = r#"{"new_traits":[],"reflection":"ok"}"#;
        assert!(clean_json(input).starts_with('{'));
    }

    #[test]
    fn test_clean_json_fenced() {
        let input = "```json\n{\"reflection\":\"hi\"}\n```";
        let cleaned = clean_json(input);
        assert!(cleaned.starts_with('{'));
        assert!(cleaned.ends_with('}'));
    }

    #[test]
    fn test_parse_output() {
        let json = r#"{"new_traits":[{"trait_key":"test","confidence":0.8}],"internal_thoughts":[{"content":"thinking","emotion":"curious"}],"reflection":"good"}"#;
        let parsed: LlmReflectionOutput = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.new_traits.len(), 1);
        assert_eq!(parsed.internal_thoughts.len(), 1);
    }

    #[test]
    fn test_parse_empty() {
        let json = r#"{"new_traits":[],"internal_thoughts":[],"reflection":""}"#;
        let parsed: LlmReflectionOutput = serde_json::from_str(json).unwrap();
        assert!(parsed.new_traits.is_empty());
    }

    fn insert_ref_at(db: &DbState, id: &str, hours_ago: i64) {
        let ts = (chrono::Utc::now() - chrono::Duration::hours(hours_ago)).to_rfc3339();
        db.with_conn(|conn| {
            insert_reflection(
                conn,
                &Reflection {
                    id: id.to_string(),
                    trigger_type: "daily".to_string(),
                    trigger_reason: None,
                    thought: "t".to_string(),
                    persona_updates: None,
                    created_at: ts,
                },
            )
        })
        .unwrap();
    }

    #[test]
    fn should_run_when_no_prior_reflection() {
        let db = test_db();
        assert!(should_run_reflection(&db), "no prior reflection -> due");
    }

    #[test]
    fn should_skip_when_within_cooldown() {
        let db = test_db();
        insert_ref_at(&db, "ref_recent", 1);
        assert!(!should_run_reflection(&db), "1h ago -> within 20h cooldown");
    }

    #[test]
    fn should_run_when_past_cooldown() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 25);
        assert!(should_run_reflection(&db), "25h ago -> past 20h cooldown");
    }

    fn insert_episode_at(db: &DbState, id: &str, hours_ago: i64, importance: f64, source_type: &str) {
        let ts = (chrono::Utc::now() - chrono::Duration::hours(hours_ago)).to_rfc3339();
        db.with_conn(|conn| {
            crate::db::episodes::insert(conn, &crate::db::episodes::Episode {
                id: id.to_string(),
                time: ts.clone(),
                summary: "s".to_string(),
                emotion: None,
                importance,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: source_type.to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.5,
                recall_count: 0,
                last_recalled_at: None,
                consolidated: false,
                created_at: ts,
            })
        })
        .unwrap();
    }

    #[test]
    fn turn_threshold_not_enough_episodes() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 2); // >1h cooldown elapsed
        for i in 0..29 {
            insert_episode_at(&db, &format!("ep{i}"), 1, 0.2, "conversation");
        }
        assert!(!should_run_turn_threshold(&db), "29 < 30 -> not due");
    }

    #[test]
    fn turn_threshold_enough_episodes() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 2);
        for i in 0..30 {
            insert_episode_at(&db, &format!("ep{i}"), 1, 0.2, "conversation");
        }
        assert!(should_run_turn_threshold(&db), "30 conversation episodes -> due");
    }

    #[test]
    fn turn_threshold_skips_non_conversation_episodes() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 2);
        for i in 0..30 {
            insert_episode_at(&db, &format!("ep{i}"), 1, 0.2, "consolidation");
        }
        assert!(!should_run_turn_threshold(&db), "non-conversation episodes don't count");
    }

    #[test]
    fn turn_threshold_within_cooldown() {
        let db = test_db();
        insert_ref_at(&db, "ref_recent", 0); // <1h
        for i in 0..30 {
            insert_episode_at(&db, &format!("ep{i}"), 0, 0.2, "conversation");
        }
        assert!(!should_run_turn_threshold(&db), "within 1h cooldown -> skip");
    }

    #[test]
    fn turn_threshold_no_prior_reflection() {
        let db = test_db();
        assert!(should_run_turn_threshold(&db), "never reflected -> eligible");
    }

    #[test]
    fn major_event_high_importance() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 2);
        insert_episode_at(&db, "ep_big", 1, 0.9, "conversation");
        assert!(should_run_major_event(&db), "importance 0.9 > 0.85 -> due");
    }

    #[test]
    fn major_event_low_importance() {
        let db = test_db();
        insert_ref_at(&db, "ref_old", 2);
        insert_episode_at(&db, "ep_small", 1, 0.5, "conversation");
        assert!(!should_run_major_event(&db), "0.5 < 0.85 -> not due");
    }

    #[test]
    fn major_event_within_cooldown() {
        let db = test_db();
        insert_ref_at(&db, "ref_recent", 0);
        insert_episode_at(&db, "ep_big", 0, 0.9, "conversation");
        assert!(!should_run_major_event(&db), "within 1h cooldown -> skip");
    }
}
