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
    let messages = vec![ChatMessage { role: "system".to_string(), content: system_prompt }];
    let result = llm.chat_reflection(&messages, Some(0.7), Some(800)).await
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
        let persona_json = serde_json::to_string(&parsed.new_traits).ok();
        crate::db::reflections::insert_reflection(conn, &Reflection {
            id: reflection_id.clone(),
            trigger_type: trigger.as_str().to_string(),
            trigger_reason: Some(parsed.reflection.clone()),
            thought: parsed.reflection.clone(),
            persona_updates: persona_json,
            created_at: now,
        })?;
        Ok::<_, String>(())
    })?;

    log::info!("Reflection complete: {} traits, {} thoughts, trigger={}", new_trait_count, new_thought_count, trigger.as_str());
    Ok(ReflectionResult { reflection_id, summary: parsed.reflection, new_trait_count, new_thought_count })
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
}
