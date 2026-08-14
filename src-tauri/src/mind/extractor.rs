use crate::llm::client::{ChatMessage, LlmClient};
use chrono::Datelike;
use serde::{Deserialize, Serialize};

const EXTRACTOR_PROMPT: &str = include_str!("../../resources/prompts/extractor.txt");

/// Result of memory extraction from a single user message.
#[derive(Debug, Clone, Default)]
pub struct ExtractionResult {
    pub episode: Option<EpisodeInput>,
    pub facts: Vec<FactInput>,
    pub emotion_delta: Option<EmotionDelta>,
    pub pending_event: Option<PendingInput>,
    /// A promise the PET just made: the user asked her to do something at a
    /// future time ("你明早叫我起床"). Stored as a pending_event with
    /// origin='pet' so she shows up to fulfill it — forgetting her own words
    /// is the most trust-damaging failure for a companion (memory-trigger
    /// v2.9 insight, adapted to our Rust-driven ingest).
    pub pet_promise: Option<PendingInput>,
}

/// Episode data extracted by the LLM, before DB insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInput {
    pub summary: String,
    #[serde(default)]
    pub emotion: Option<String>,
    /// One short scene/mood snapshot from the moment it happened
    /// ("在奶茶店门口，眼睛亮亮的") — surfaced later with the memory so she
    /// recalls it with warmth instead of reciting a file (memory-trigger
    /// "context" idea, adapted). Optional; omitted unless the moment had a
    /// clear atmosphere.
    #[serde(default)]
    pub emotion_anchor: Option<String>,
    #[serde(default = "default_importance")]
    pub importance: f64,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
}

fn default_importance() -> f64 {
    0.5
}

/// Fact data extracted by the LLM, before DB insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactInput {
    pub category: String,
    pub key: String,
    pub value: String,
    #[serde(default = "default_importance")]
    pub confidence: f64,
}

/// Emotion change suggested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionDelta {
    #[serde(default)]
    pub mood: f64,
    #[serde(default)]
    pub stress: f64,
    #[serde(default)]
    pub energy: f64,
}

/// Pending event / reminder extracted by the LLM.
///
/// Two mutually-exclusive timing modes (Architecture Principle #1: absolute
/// time is computed by Rust, never by the LLM):
/// - Short-term reminder ("remind me in 30min"): set `offset_minutes`.
/// - Dated future event ("exam next Friday"): set `event_date` (ISO date).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInput {
    pub title: String,
    /// Absolute ISO date for dated future events, e.g. "2026-08-05".
    #[serde(default)]
    pub event_date: Option<String>,
    /// Relative minutes-from-now for short-term reminders, e.g. 3 for
    /// "remind me in 3 minutes". Rust converts this to an absolute remind_date.
    #[serde(default)]
    pub offset_minutes: Option<i64>,
}

/// Internal struct matching the LLM JSON output.
#[derive(Debug, Deserialize)]
struct LlmExtraction {
    episode: Option<EpisodeInput>,
    #[serde(default)]
    facts: Vec<FactInput>,
    emotion_delta: Option<EmotionDelta>,
    pending_event: Option<PendingInput>,
    #[serde(default)]
    pet_promise: Option<PendingInput>,
}

/// Extracts structured memory items from the user's message.
/// `known_facts` is injected into the prompt for contradiction detection.
pub async fn extract(
    text: &str,
    known_facts: &str,
    llm: &LlmClient,
) -> Result<ExtractionResult, String> {
    // Inject today's date (local time, weekday) so the LLM can resolve
    // relative dates like "明天" / "下周二" correctly instead of hallucinating
    // a training-date (observed: "明天" → 2026-01-02, "下周二" → 2026-05-12).
    let now_local = chrono::Local::now();
    let weekday_cn = ["星期一", "星期二", "星期三", "星期四", "星期五", "星期六", "星期日"]
        [now_local.weekday().num_days_from_monday() as usize];    let today = format!("{}（{}）", now_local.format("%Y-%m-%d"), weekday_cn);
    let system_prompt = EXTRACTOR_PROMPT
        .replace("{known_facts}", known_facts)
        .replace("{today}", &today);

    let messages = || {
        vec![
            ChatMessage::system(system_prompt.clone()),
            ChatMessage::user(text),
        ]
    };

    let mut last_err = String::new();
    for attempt in 1..=2 {
        let result = llm
            .chat_reflection(&messages(), Some(0.3), Some(4096))
            .await
            .map_err(|e| format!("Extractor LLM call failed: {}", e))?;

        match parse_extraction(&result.content) {
            Ok(extraction) => {
                if attempt > 1 {
                    log::info!("[extractor] recovery succeeded on retry {attempt}");
                }
                return Ok(extraction);
            }
            Err(e) => {
                last_err = e;
                // Empty/blank content is a transient flash empty-output (same
                // failure mode as pitfall #3, but with 4096 the reasoning ate
                // the full budget). One retry; then degrade to an empty
                // extraction so a lost reminder never kills the whole turn.
                if result.content.trim().is_empty() {
                    log::warn!(
                        "[extractor] empty LLM content (attempt {}); retrying or degrading",
                        attempt
                    );
                } else {
                    log::warn!(
                        "[extractor] parse failed (attempt {}): {}",
                        attempt,
                        truncate_for_log(&result.content, 120)
                    );
                }
            }
        }
    }

    log::warn!(
        "[extractor] extraction failed after retries: {}; degrading to empty extraction (turn continues, memory skipped)",
        last_err
    );
    Ok(ExtractionResult::default())
}

fn truncate_for_log(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// Parses the LLM's JSON output into an ExtractionResult.
fn parse_extraction(raw: &str) -> Result<ExtractionResult, String> {
    let json_str = extract_json_block(raw);
    let parsed: LlmExtraction = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse extraction '{}': {}", raw.trim(), e))?;

    Ok(ExtractionResult {
        episode: parsed.episode,
        facts: parsed.facts,
        emotion_delta: parsed.emotion_delta,
        pending_event: parsed.pending_event,
        pet_promise: parsed.pet_promise,
    })
}

/// Extracts the first JSON object from a string that may contain
/// markdown fences or surrounding text.
fn extract_json_block(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[start..=end].to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_extraction() {
        let json = r#"{
            "episode": {"summary": "ate hotpot with friends", "emotion": "happy", "importance": 0.7, "participants": ["friends"], "topics": ["food"]},
            "facts": [{"category": "preference", "key": "food", "value": "hotpot", "confidence": 0.85}],
            "emotion_delta": {"mood": 0.05, "stress": -0.02, "energy": 0.0},
            "pending_event": null
        }"#;
        let result = parse_extraction(json).unwrap();
        assert!(result.episode.is_some());
        assert_eq!(result.facts.len(), 1);
        assert!(result.emotion_delta.is_some());
        assert!(result.pending_event.is_none());
    }

    #[test]
    fn test_parse_minimal() {
        let json = r#"{
            "facts": [{"category": "preference", "key": "drink", "value": "milk tea", "confidence": 0.95}]
        }"#;
        let result = parse_extraction(json).unwrap();
        assert!(result.episode.is_none());
        assert_eq!(result.facts.len(), 1);
        assert!(result.emotion_delta.is_none());
        assert!(result.pending_event.is_none());
    }

    #[test]
    fn test_parse_pending_event() {
        let json = r#"{
            "pending_event": {"title": "job interview", "event_date": "2026-07-20"}
        }"#;
        let result = parse_extraction(json).unwrap();
        assert!(result.pending_event.is_some());
        assert_eq!(result.pending_event.unwrap().title, "job interview");
    }

    #[test]
    fn test_parse_pet_promise() {
        let json = r#"{
            "pet_promise": {"title": "明早叫 ta 起床", "event_date": "2026-08-15"}
        }"#;
        let result = parse_extraction(json).unwrap();
        assert!(result.pending_event.is_none(), "pet promise is not a user event");
        let promise = result.pet_promise.expect("pet_promise should parse");
        assert_eq!(promise.title, "明早叫 ta 起床");
        assert_eq!(promise.event_date.as_deref(), Some("2026-08-15"));
    }

    #[test]
    fn test_parse_pet_promise_absent_defaults_none() {
        let json = r#"{"facts": []}"#;
        let result = parse_extraction(json).unwrap();
        assert!(result.pet_promise.is_none());
    }

    #[test]
    fn test_parse_emotion_anchor() {
        let json = r#"{
            "episode": {"summary": "和糯米去看猫", "emotion": "开心", "emotion_anchor": "在猫咖，眼睛亮亮的", "importance": 0.7}
        }"#;
        let result = parse_extraction(json).unwrap();
        let ep = result.episode.expect("episode should parse");
        assert_eq!(ep.emotion_anchor.as_deref(), Some("在猫咖，眼睛亮亮的"));

        // Absent key -> None (serde default), old-format output still parses.
        let legacy = r#"{"episode": {"summary": "去面试了"}}"#;
        let legacy_result = parse_extraction(legacy).unwrap();
        assert!(legacy_result.episode.unwrap().emotion_anchor.is_none());
    }

    #[test]
    fn test_parse_empty() {
        let json = "{}";
        let result = parse_extraction(json).unwrap();
        assert!(result.episode.is_none());
        assert!(result.facts.is_empty());
    }

    #[test]
    fn test_parse_with_markdown_fence() {
        let json = "```json\n{\"facts\": []}\n```";
        let result = parse_extraction(json).unwrap();
        assert!(result.facts.is_empty());
    }

    #[test]
    fn test_parse_invalid() {
        let result = parse_extraction("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_importance_default() {
        let json = r#"{"episode": {"summary": "test"}}"#;
        let result = parse_extraction(json).unwrap();
        let ep = result.episode.unwrap();
        assert!((ep.importance - 0.5).abs() < 0.001);
    }
}
