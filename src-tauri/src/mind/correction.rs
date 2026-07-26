use crate::db::facts as db_facts;
use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

/// Result of handling a user correction.
#[derive(Debug, Clone)]
pub struct CorrectionResult {
    /// The fact category that was corrected.
    pub category: String,
    /// The key within the category.
    pub key: String,
    /// The old (incorrect) value.
    pub old_value: String,
    /// The new (correct) value.
    pub new_value: String,
}

/// Internal struct for parsing LLM correction output.
#[derive(Debug, Deserialize)]
struct CorrectionParse {
    category: String,
    key: String,
    new_value: String,
}

/// Handles a user correction: identifies what fact is being corrected,
/// expires the old fact, and inserts the corrected version with high confidence.
///
/// Architecture principle: the LLM only suggests what to correct.
/// The actual DB mutation is done by Rust code.
pub async fn handle_correction(
    text: &str,
    known_facts: &str,
    llm: &LlmClient,
    db: &DbState,
) -> Result<Option<CorrectionResult>, String> {
    // Step 1: Ask LLM what the user is correcting.
    let system_prompt = format!(
"You are a memory correction assistant. The user is correcting something the pet remembered wrong.
Known facts:
{known_facts}

User's correction: \"{text}\"

Identify which fact is being corrected and what the correct value should be.
Respond with JSON only:
{{\"category\": \"...\", \"key\": \"...\", \"new_value\": \"...\"}}

If the correction is unclear or does not match any known fact, respond with: {{\"category\": \"\", \"key\": \"\", \"new_value\": \"\"}}"
    );

    let messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
    }];

    let result = llm
        .chat_reflection(&messages, Some(0.2), Some(2048))
        .await
        .map_err(|e| format!("Correction LLM call failed: {}", e))?;

    let parsed = parse_correction(&result.content)?;

    // Empty response means the correction didn't match any known fact.
    if parsed.category.is_empty() || parsed.key.is_empty() {
        return Ok(None);
    }

    // Step 2: Find the old fact and expire it.
    let now = Utc::now().to_rfc3339();
    let correction_result = db.with_conn(|conn| {
        let active_facts = db_facts::get_active(conn, &parsed.category, &parsed.key)?;
        let old_value = active_facts
            .first()
            .map(|f| f.value.clone())
            .unwrap_or_default();

        // Expire old facts.
        if !active_facts.is_empty() {
            db_facts::expire_old(conn, &parsed.category, &parsed.key, &now)?;
        }

        // Step 3: Insert corrected fact with high confidence (user explicitly corrected).
        let fact_id = format!("fact_{}", Uuid::new_v4().simple());
        let new_fact = db_facts::Fact {
            id: fact_id,
            category: parsed.category.clone(),
            key: parsed.key.clone(),
            value: parsed.new_value.clone(),
            confidence: 0.98, // User explicitly corrected: very high confidence
            valid_from: Some(now.clone()),
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: now.clone(),
            updated_at: now,
        };
        db_facts::dedup_insert(conn, &new_fact)?;

        Ok(CorrectionResult {
            category: parsed.category,
            key: parsed.key,
            old_value,
            new_value: new_fact.value,
        })
    })?;

    Ok(Some(correction_result))
}

/// Parses the LLM's correction output JSON.
fn parse_correction(raw: &str) -> Result<CorrectionParse, String> {
    let json_str = extract_json_block(raw);
    let parsed: CorrectionParse = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse correction '{}': {}", raw.trim(), e))?;
    Ok(parsed)
}

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
    fn test_parse_correction() {
        let json = r#"{"category": "preference", "key": "drink", "new_value": "milk tea"}"#;
        let parsed = parse_correction(json).unwrap();
        assert_eq!(parsed.category, "preference");
        assert_eq!(parsed.key, "drink");
        assert_eq!(parsed.new_value, "milk tea");
    }

    #[test]
    fn test_parse_empty_correction() {
        let json = r#"{"category": "", "key": "", "new_value": ""}"#;
        let parsed = parse_correction(json).unwrap();
        assert!(parsed.category.is_empty());
    }

    #[test]
    fn test_parse_with_fence() {
        let raw = "```json\n{\"category\": \"work\", \"key\": \"role\", \"new_value\": \"engineer\"}\n```";
        let parsed = parse_correction(raw).unwrap();
        assert_eq!(parsed.category, "work");
    }

    #[test]
    fn test_parse_invalid() {
        let result = parse_correction("not json");
        assert!(result.is_err());
    }
}
