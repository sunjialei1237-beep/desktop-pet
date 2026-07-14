use crate::llm::client::{ChatMessage, LlmClient};
use serde::{Deserialize, Serialize};

const EXTRACTOR_PROMPT: &str = include_str!("../../resources/prompts/extractor.txt");

/// Result of memory extraction from a single user message.
#[derive(Debug, Clone, Default)]
pub struct ExtractionResult {
    pub episode: Option<EpisodeInput>,
    pub facts: Vec<FactInput>,
    pub emotion_delta: Option<EmotionDelta>,
    pub pending_event: Option<PendingInput>,
}

/// Episode data extracted by the LLM, before DB insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInput {
    pub summary: String,
    #[serde(default)]
    pub emotion: Option<String>,
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

/// Pending event extracted by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInput {
    pub title: String,
    pub event_date: String,
}

/// Internal struct matching the LLM JSON output.
#[derive(Debug, Deserialize)]
struct LlmExtraction {
    episode: Option<EpisodeInput>,
    #[serde(default)]
    facts: Vec<FactInput>,
    emotion_delta: Option<EmotionDelta>,
    pending_event: Option<PendingInput>,
}

/// Extracts structured memory items from the user's message.
/// `known_facts` is injected into the prompt for contradiction detection.
pub async fn extract(
    text: &str,
    known_facts: &str,
    llm: &LlmClient,
) -> Result<ExtractionResult, String> {
    let system_prompt = EXTRACTOR_PROMPT.replace("{known_facts}", known_facts);

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        },
        ChatMessage {
            role: "user".to_string(),
            content: text.to_string(),
        },
    ];

    let result = llm
        .chat_reflection(&messages, Some(0.3), Some(1024))
        .await
        .map_err(|e| format!("Extractor LLM call failed: {}", e))?;

    parse_extraction(&result.content)
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
