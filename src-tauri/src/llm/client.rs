use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::error::LlmError;

/// Message in a chat conversation (OpenAI-compatible format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request body for /v1/chat/completions.
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// Non-streaming response from /v1/chat/completions.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Result of a chat completion call.
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// OpenAI-compatible LLM client. Works with DeepSeek, OpenAI, Moonshot, Ollama, vLLM, etc.
pub struct LlmClient {
    http: HttpClient,
    base_url: String,
    api_key: String,
    main_model: String,
    reflection_model: String,
}

impl LlmClient {
    /// Creates a new LLM client from configuration.
    /// Returns Err if api_key is empty (LLM not configured).
    pub fn new(
        base_url: &str,
        api_key: &str,
        main_model: &str,
        reflection_model: &str,
    ) -> Result<Self, LlmError> {
        if api_key.is_empty() {
            return Err(LlmError::NotConfigured);
        }

        let http = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| LlmError::Network)?;

        Ok(LlmClient {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            main_model: main_model.to_string(),
            reflection_model: reflection_model.to_string(),
        })
    }

    /// Sends a chat completion request using the main model.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResult, LlmError> {
        self.chat_with_model(&self.main_model, messages, temperature, max_tokens)
            .await
    }

    /// Sends a chat completion request using the reflection model (cheaper/faster).
    pub async fn chat_reflection(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResult, LlmError> {
        self.chat_with_model(&self.reflection_model, messages, temperature, max_tokens)
            .await
    }

    async fn chat_with_model(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResult, LlmError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            temperature,
            max_tokens,
            stream: Some(false),
        };

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network
                }
            })?;

        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(LlmError::Auth);
        }
        if status.as_u16() == 429 {
            return Err(LlmError::RateLimit);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Server(format!("HTTP {}: {}", status, body)));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;

        let content = chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        let usage = chat_resp.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(ChatResult {
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        })
    }

    /// Returns true if the client is configured (has an API key).
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_configured() {
        let result = LlmClient::new("https://api.test.com/v1", "", "model", "model");
        assert!(matches!(result, Err(LlmError::NotConfigured)));
    }

    #[test]
    fn test_configured_ok() {
        let client = LlmClient::new(
            "https://api.test.com/v1/",
            "sk-test",
            "gpt-4o-mini",
            "gpt-4o-mini",
        );
        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.is_configured());
    }
}
