use futures_util::StreamExt;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

/// `stream_options` for the chat request — `include_usage` so the final
/// streamed frame carries token counts (Debug Panel / architecture #11).
#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// One streamed chunk (OpenAI-compatible SSE `data:` payload). `reasoning_content`
/// (DeepSeek v4 internal thinking) is deliberately NOT deserialized — only
/// `content` (the reply) is surfaced.
#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Delta,
}

#[derive(Debug, Default, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
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

/// Daily LLM cost accounting for the debug panel (Architecture #8: cost is a
/// design constraint — it must be observable). Shared via `Arc<Mutex<>>` inside
/// `LlmClient`, so every clone (one is taken per conversation turn) reports into
/// the same totals. Resets at the local-day boundary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LlmCostStats {
    /// Local date (YYYY-MM-DD) these counts belong to.
    pub date: String,
    pub calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl Default for LlmCostStats {
    fn default() -> Self {
        Self {
            date: local_today(),
            calls: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
        }
    }
}

impl LlmCostStats {
    /// Records one successful call's usage, resetting totals if the local day
    /// rolled over since the last record.
    fn record(&mut self, prompt_tokens: u32, completion_tokens: u32) {
        let today = local_today();
        if self.date != today {
            *self = LlmCostStats::default();
        }
        self.calls += 1;
        self.prompt_tokens += prompt_tokens as u64;
        self.completion_tokens += completion_tokens as u64;
    }

    /// Returns a snapshot, zeroed if the local day has rolled over (so an
    /// overnight process doesn't display yesterday's totals as "today").
    fn snapshot_today(&self) -> Self {
        if self.date != local_today() {
            LlmCostStats::default()
        } else {
            self.clone()
        }
    }
}

/// Current local date as YYYY-MM-DD (the user perceives cost in their own day).
fn local_today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// OpenAI-compatible LLM client. Works with DeepSeek, OpenAI, Moonshot, Ollama, vLLM, etc.
#[derive(Clone)]
pub struct LlmClient {
    http: HttpClient,
    base_url: String,
    api_key: String,
    main_model: String,
    reflection_model: String,
    /// Shared daily cost counters (Architecture #8). Behind `Arc<Mutex<>>` so
    /// every clone reports into one set of totals; `Arc` keeps `LlmClient`
    /// `Clone` (a fresh client is taken per conversation turn).
    cost: std::sync::Arc<std::sync::Mutex<LlmCostStats>>,
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
            cost: std::sync::Arc::new(std::sync::Mutex::new(LlmCostStats::default())),
        })
    }

    /// Records one successful call's token usage into the shared daily cost
    /// stats (Architecture #8). Lock-poison safe: a failed lock only skips
    /// accounting, never breaks the call.
    fn track_usage(&self, result: &ChatResult) {
        if let Ok(mut stats) = self.cost.lock() {
            stats.record(result.prompt_tokens, result.completion_tokens);
        }
    }

    /// Today's LLM cost snapshot for the debug panel (#8/#11).
    pub fn cost_today(&self) -> LlmCostStats {
        self.cost
            .lock()
            .map(|s| s.snapshot_today())
            .unwrap_or_default()
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
        let url = self.build_url();

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            temperature,
            max_tokens,
            stream: Some(false),
            stream_options: None,
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

        let body = resp
            .text()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;
        let chat_resp: ChatResponse = serde_json::from_str(&body)
            .map_err(|e| LlmError::Parse(format!("{} | body: {}", e, body)))?;

        let content = chat_resp
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        if content.trim().is_empty() {
            log::warn!(
                "[llm-empty-content] model={} body_len={} body={}",
                model,
                body.len(),
                body
            );
        }

        let usage = chat_resp.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        let result = ChatResult {
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        };
        self.track_usage(&result);
        Ok(result)
    }

    /// Build the chat-completions URL from the configured base_url. Shared by
    /// `chat_with_model` (non-streaming) and `chat_stream`.
    fn build_url(&self) -> String {
        // base_url from config includes the API version (e.g. "https://api.deepseek.com/v1").
        // Append "/chat/completions" directly; add "/v1" for base_urls without version.
        if self.base_url.ends_with("/v1") {
            format!("{}/chat/completions", self.base_url)
        } else {
            format!("{}/v1/chat/completions", self.base_url)
        }
    }

    /// Streaming chat completion (architecture #10: tokens flow out as the
    /// model produces them, so the pet appears to "speak live" instead of
    /// popping a full reply after a long wait). DeepSeek v4 is a reasoning
    /// model: it emits `reasoning_content` (internal thinking) and `content`
    /// (the reply) as separate deltas — we forward ONLY `content`, so the
    /// frontend's thinking-dots stay up during reasoning and drop the moment
    /// she actually starts replying (踩坑#3).
    ///
    /// `on_token` is invoked with each non-empty content delta; the fully
    /// accumulated `content` is also returned in `ChatResult` so the existing
    /// grounding / emotion / working-memory steps in `converse` keep working
    /// unchanged (streaming is transparent to them).
    pub async fn chat_stream<F: FnMut(&str)>(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        mut on_token: F,
    ) -> Result<ChatResult, LlmError> {
        let url = self.build_url();
        let request = ChatRequest {
            model: self.main_model.clone(),
            messages: messages.to_vec(),
            temperature,
            max_tokens,
            stream: Some(true),
            stream_options: Some(StreamOptions { include_usage: true }),
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

        // Decode the SSE byte stream line-by-line. Each `data: <json>` line is
        // one chunk; `[DONE]` terminates. `buf` retains a partial line across
        // byte-chunk boundaries (a chunk boundary can split mid-line).
        let mut full = String::new();
        let mut usage: Option<Usage> = None;
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk_res) = stream.next().await {
            let bytes = chunk_res.map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network
                }
            })?;
            buf.push_str(
                std::str::from_utf8(&bytes).map_err(|e| LlmError::Parse(format!("utf8: {}", e)))?,
            );

            while let Some(pos) = buf.find('\n') {
                let line: String = buf[..pos].trim_end_matches('\r').to_string();
                buf = buf[pos + 1..].to_string();
                if Self::feed_sse_line(&line, &mut full, &mut usage, &mut on_token) {
                    let result = Self::finalize_stream(full, usage);
                    self.track_usage(&result);
                    return Ok(result);
                }
            }
        }

        // Stream ended without an explicit [DONE] (some providers omit it).
        let result = Self::finalize_stream(full, usage);
        self.track_usage(&result);
        Ok(result)
    }

    /// Feed one SSE line into the stream accumulator. Returns `true` when the
    /// line is the `[DONE]` sentinel (stream finished). Pure and unit-testable
    /// (#11): extracted from the async HTTP loop so the SSE / reasoning-content
    /// separation can be tested without a network mock. Malformed lines are
    /// skipped, not fatal.
    fn feed_sse_line<F: FnMut(&str)>(
        line: &str,
        full: &mut String,
        usage: &mut Option<Usage>,
        on_token: &mut F,
    ) -> bool {
        let payload = match line.trim().strip_prefix("data:") {
            Some(p) => p.trim(),
            None => return false, // blank line, `event:`/`id:` comments, etc.
        };
        if payload == "[DONE]" {
            return true;
        }
        let parsed: StreamChunk = match serde_json::from_str(payload) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[llm-stream] skip malformed chunk: {} | {}", e, payload);
                return false;
            }
        };
        for choice in parsed.choices {
            if let Some(c) = choice.delta.content {
                if !c.is_empty() {
                    on_token(&c);
                    full.push_str(&c);
                }
            }
        }
        if parsed.usage.is_some() {
            *usage = parsed.usage;
        }
        false
    }

    /// Assemble the final `ChatResult` from a finished stream.
    fn finalize_stream(content: String, usage: Option<Usage>) -> ChatResult {
        if content.trim().is_empty() {
            log::warn!("[llm-stream-empty] no content deltas received");
        }
        let usage = usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });
        ChatResult {
            content,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
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

    #[test]
    fn test_sse_skips_reasoning_forwards_content() {
        // Exercises feed_sse_line directly: the SSE / reasoning-content
        // separation must work without a live HTTP stream (architecture #11).
        let mut full = String::new();
        let mut usage: Option<Usage> = None;
        let mut tokens: Vec<String> = Vec::new();
        let mut on_token = |t: &str| tokens.push(t.to_string());

        // content delta
        assert!(!LlmClient::feed_sse_line(
            r#"data: {"choices":[{"delta":{"content":"你"}}]}"#,
            &mut full,
            &mut usage,
            &mut on_token,
        ));
        // reasoning_content delta — must be ignored (not surfaced, not accumulated)
        assert!(!LlmClient::feed_sse_line(
            r#"data: {"choices":[{"delta":{"reasoning_content":"内部思考..."}}]}"#,
            &mut full,
            &mut usage,
            &mut on_token,
        ));
        // second content delta
        assert!(!LlmClient::feed_sse_line(
            r#"data: {"choices":[{"delta":{"content":"好"}}]}"#,
            &mut full,
            &mut usage,
            &mut on_token,
        ));
        // final frame: empty delta + usage counts
        assert!(!LlmClient::feed_sse_line(
            r#"data: {"choices":[{"delta":{}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}"#,
            &mut full,
            &mut usage,
            &mut on_token,
        ));
        // DONE sentinel terminates
        assert!(LlmClient::feed_sse_line(
            "data: [DONE]",
            &mut full,
            &mut usage,
            &mut on_token,
        ));

        assert_eq!(full, "你好");
        assert_eq!(tokens, vec!["你".to_string(), "好".to_string()]);
        let u = usage.expect("usage should be recorded on the final frame");
        assert_eq!(u.total_tokens, 12);
        assert_eq!(u.completion_tokens, 2);
    }

    #[test]
    fn test_sse_ignores_non_data_lines_and_malformed() {
        let mut full = String::new();
        let mut usage: Option<Usage> = None;
        let mut on_token = |_t: &str| {};

        // blank line, comment, event — all ignored, not "done"
        assert!(!LlmClient::feed_sse_line("", &mut full, &mut usage, &mut on_token));
        assert!(!LlmClient::feed_sse_line(": heartbeat", &mut full, &mut usage, &mut on_token));
        assert!(!LlmClient::feed_sse_line("event: chunk", &mut full, &mut usage, &mut on_token));
        // malformed JSON payload — skipped, not fatal
        assert!(!LlmClient::feed_sse_line("data: {not json", &mut full, &mut usage, &mut on_token));
        assert_eq!(full, "");
    }

    #[test]
    fn test_cost_record_accumulates() {
        let mut stats = LlmCostStats::default();
        assert_eq!(stats.calls, 0);
        stats.record(10, 5);
        stats.record(20, 8);
        assert_eq!(stats.calls, 2);
        assert_eq!(stats.prompt_tokens, 30);
        assert_eq!(stats.completion_tokens, 13);
    }

    #[test]
    fn test_cost_record_resets_on_day_rollover() {
        // A stats block stamped to a long-past date must reset before recording.
        let mut stats = LlmCostStats {
            date: "2020-01-01".to_string(),
            calls: 99,
            prompt_tokens: 999,
            completion_tokens: 999,
        };
        stats.record(10, 4);
        assert_eq!(stats.calls, 1); // reset, then +1
        assert_eq!(stats.prompt_tokens, 10);
        assert_eq!(stats.completion_tokens, 4);
        assert_eq!(stats.date, local_today());
    }

    #[test]
    fn test_cost_snapshot_zeroes_when_stale() {
        let stats = LlmCostStats {
            date: "2020-01-01".to_string(),
            calls: 99,
            prompt_tokens: 999,
            completion_tokens: 999,
        };
        let snap = stats.snapshot_today();
        assert_eq!(snap.calls, 0);
        assert_eq!(snap.prompt_tokens, 0);
        assert_eq!(snap.date, local_today());
    }
}
