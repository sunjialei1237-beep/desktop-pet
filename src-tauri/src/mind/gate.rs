use crate::llm::client::{ChatMessage, LlmClient};
use serde::Deserialize;

/// Classification routes determined by the Memory Gate.
/// The gate is a router, not a binary store/discard classifier.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateRoute {
    /// Full episode + fact extraction pipeline.
    StoreFull,
    /// Only update emotion state (e.g., "hahaha").
    EmotionOnly,
    /// Future plan to track (e.g., "interview tomorrow").
    PendingEvent,
    /// User is correcting a prior memory.
    Correction,
    /// Simple greeting/acknowledgment, minor emotion nudge.
    Silence,
    /// Pure noise, do nothing.
    Discard,
}

impl GateRoute {
    /// Converts the route to a lowercase string for JSON serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            GateRoute::StoreFull => "store_full",
            GateRoute::EmotionOnly => "emotion_only",
            GateRoute::PendingEvent => "pending_event",
            GateRoute::Correction => "correction",
            GateRoute::Silence => "silence",
            GateRoute::Discard => "discard",
        }
    }
}

/// Internal struct for parsing LLM JSON output.
#[derive(Debug, Deserialize)]
struct GateResponse {
    route: String,
    #[serde(default)]
    #[allow(dead_code)]
    reason: String,
}

/// The system prompt loaded from the bundled template.
const GATE_PROMPT: &str = include_str!("../../resources/prompts/gate.txt");

/// Classifies the user's message into a GateRoute using the reflection model.
/// Uses temperature 0.1 for deterministic classification.
pub async fn classify(text: &str, llm: &LlmClient) -> Result<GateRoute, String> {
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: GATE_PROMPT.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: text.to_string(),
        },
    ];

    let result = llm
        .chat_reflection(&messages, Some(0.1), Some(2048))
        .await
        .map_err(|e| format!("Gate LLM call failed: {}", e))?;

    // Parse JSON from the response. Tolerate extra text around the JSON.
    let route = parse_gate_json(&result.content)?;
    Ok(route)
}

/// Parses the gate response JSON, extracting the route field.
/// Tolerates markdown code fences and extra whitespace.
fn parse_gate_json(raw: &str) -> Result<GateRoute, String> {
    let json_str = extract_json_block(raw);
    let resp: GateResponse = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse gate response '{}': {}", raw.trim(), e))?;

    match resp.route.as_str() {
        "store_full" => Ok(GateRoute::StoreFull),
        "emotion_only" => Ok(GateRoute::EmotionOnly),
        "pending_event" => Ok(GateRoute::PendingEvent),
        "correction" => Ok(GateRoute::Correction),
        "silence" => Ok(GateRoute::Silence),
        "discard" => Ok(GateRoute::Discard),
        other => Err(format!("Unknown gate route: {}", other)),
    }
}

/// Extracts the first JSON object from a string that may contain
/// markdown fences or surrounding text.
fn extract_json_block(raw: &str) -> String {
    let trimmed = raw.trim();

    // Try to find a JSON object between { and }
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
    fn test_parse_store_full() {
        let route = parse_gate_json(r#"{"route": "store_full", "reason": "event"}"#).unwrap();
        assert_eq!(route, GateRoute::StoreFull);
    }

    #[test]
    fn test_parse_emotion_only() {
        let route = parse_gate_json(r#"{"route": "emotion_only"}"#).unwrap();
        assert_eq!(route, GateRoute::EmotionOnly);
    }

    #[test]
    fn test_parse_with_markdown_fence() {
        let raw = "```json\n{\"route\": \"discard\"}\n```";
        let route = parse_gate_json(raw).unwrap();
        assert_eq!(route, GateRoute::Discard);
    }

    #[test]
    fn test_parse_with_surrounding_text() {
        let raw = "Here is the result:\n{\"route\": \"pending_event\"}\nDone.";
        let route = parse_gate_json(raw).unwrap();
        assert_eq!(route, GateRoute::PendingEvent);
    }

    #[test]
    fn test_parse_correction() {
        let route = parse_gate_json(r#"{"route": "correction"}"#).unwrap();
        assert_eq!(route, GateRoute::Correction);
    }

    #[test]
    fn test_parse_silence() {
        let route = parse_gate_json(r#"{"route": "silence"}"#).unwrap();
        assert_eq!(route, GateRoute::Silence);
    }

    #[test]
    fn test_parse_unknown_route() {
        let result = parse_gate_json(r#"{"route": "unknown_thing"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_gate_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_route_as_str() {
        assert_eq!(GateRoute::StoreFull.as_str(), "store_full");
        assert_eq!(GateRoute::Discard.as_str(), "discard");
    }
}
