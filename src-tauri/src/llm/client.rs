use futures_util::StreamExt;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::error::LlmError;

// ===== Tool calling types (OpenAI-compatible function-calling format) =====
// Added for the Tool Layer (Phase 1): non-streaming tool rounds carry these in
// both the request (ToolDef, advertised `tools`) and the response (ToolCall).
// Streaming (chat_stream) deliberately omits tools — DeepSeek's stream Delta has
// no tool_calls field (silently dropped), so tool rounds always go through the
// non-streaming chat(); only the final answer round is streamed.

/// A tool definition advertised to the LLM in a request (`tools` array entry).
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    type_: &'static str,
    pub function: ToolFunction,
}

impl ToolDef {
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            type_: "function",
            function: ToolFunction {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// A tool call the LLM wants to make (returned in a response, echoed back in the
/// assistant message of the next round). `arguments` is a JSON-encoded string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// Always "function" for function-calling.
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON-encoded arguments string (OpenAI convention), NOT a parsed object.
    pub arguments: String,
}

/// Message in a chat conversation (OpenAI-compatible format).
///
/// `content` is `Option<String>` because a tool-request round (assistant asking
/// to call a tool) carries `content: null` + `tool_calls`. Plain user/system/
/// assistant messages always have `Some(content)`. Built via the helper
/// constructors below (`ChatMessage::user`, `::system`, …) so call sites never
/// hand-write the full struct literal — this also keeps the Phase-1
/// String→Option migration contained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Assistant tool-call request round (role:"assistant"). Absent elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// role:"tool" result message: the id of the tool_call this answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// role:"tool" result message: the tool's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn user(s: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(s.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn system(s: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(s.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn assistant(s: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: Some(s.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    /// Assistant round that requests tool calls. `content` is None for a pure
    /// tool-request round (DeepSeek emits content:null here).
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { role: "assistant".into(), content, tool_calls: Some(tool_calls), tool_call_id: None, name: None }
    }
    /// role:"tool" result message answering a specific tool_call_id.
    pub fn tool_result(tool_call_id: &str, name: &str, content: &str) -> Self {
        Self { role: "tool".into(), content: Some(content.into()), tool_calls: None, tool_call_id: Some(tool_call_id.into()), name: Some(name.into()) }
    }
    /// Content as &str, empty if None (tool-call request rounds have null
    /// content). Convenience for token estimation / logging that treats a
    /// missing body as empty.
    pub fn content_str(&self) -> &str {
        self.content.as_deref().unwrap_or("")
    }
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
    /// DeepSeek v4 thinking-mode control (top-level `thinking` field). When
    /// `disabled`, the model skips `reasoning_content` entirely — used on the
    /// gate/extractor steps (pure classification) to cut per-turn reasoning
    /// latency and root-fix 踩坑#3 (reasoning ate the completion budget → empty
    /// content). The main reply is also `disabled` — sub-5s latency; reliability
    /// comes from the grounding layer, not reasoning (see converse.rs step 9).
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
    /// DeepSeek v4 reasoning depth (`reasoning_effort`), only meaningful with
    /// `thinking:{enabled}`. Dormant: `converse` passes `None` on every call
    /// (main reply is thinking-off). A "low"-effort main reply was tested but
    /// broke the 5s gate with no quality gain; kept as reserved plumbing.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    /// Tools advertised to the LLM (function-calling). Only set on non-streaming
    /// tool rounds (chat_with_model); chat_stream always leaves this None.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef>>,
    /// "auto" (LLM decides) or "none". Sent only when `tools` is Some.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

/// DeepSeek v4 `thinking` parameter: `{"type": "enabled" | "disabled"}`.
/// Verified on deepseek-v4-flash: `{type:disabled}` → 200, no reasoning_content.
#[derive(Debug, Serialize, Clone)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    type_: String,
}

impl ThinkingConfig {
    /// Disable the reasoning step. Used on every LLM call (gate, extractor,
    /// and now the main reply) so content streams without a reasoning_content
    /// preamble — cutting first-token latency on the reasoning model.
    pub fn disabled() -> Self {
        Self { type_: "disabled".to_string() }
    }

    /// Enable the reasoning step. Currently unused — every call is `disabled()`
    /// for latency; kept to pair with `reasoning_effort` if we revisit it.
    pub fn enabled() -> Self {
        Self { type_: "enabled".to_string() }
    }
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
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    /// DeepSeek prefix-cache accounting (0 when the provider omits them).
    #[serde(default)]
    prompt_cache_hit_tokens: Option<u32>,
    #[serde(default)]
    prompt_cache_miss_tokens: Option<u32>,
}

/// Result of a chat completion call.
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// DeepSeek prefix-cache hit/miss for this call (None when unsupported).
    /// Soul v2 L2a observability: the near-end split keeps the static system
    /// prefix stable so these hits should rise vs the v1 layout.
    pub prompt_cache_hit_tokens: Option<u32>,
    pub prompt_cache_miss_tokens: Option<u32>,
    /// Tool calls the LLM requested this round (non-streaming only). `Some` +
    /// non-empty + `finish_reason == "tool_calls"` means the agent loop must
    /// execute tools and re-prompt; `None`/empty means this is a final answer.
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
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
    ///
    /// `tools`: advertised tool definitions for function-calling. `None` = plain
    /// reply (no tool round); `Some(defs)` enables tool-calling with
    /// `tool_choice:"auto"` (the LLM decides whether to call). Tool rounds are
    /// always non-streaming — see `chat_with_model`.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDef]>,
    ) -> Result<ChatResult, LlmError> {
        self.chat_with_model(&self.main_model, messages, temperature, max_tokens, None, tools)
            .await
    }

    /// Sends a chat completion request using the reflection model (cheaper/faster).
    /// The reflection model powers the gate + extractor steps (steps 1-2 of each
    /// turn) — pure classification, no reasoning needed. Thinking is disabled
    /// here to remove 2/3 of per-turn reasoning latency and root-fix 踩坑#3
    /// (reasoning ate the completion budget → empty content → parse crash).
    /// The main reply (step 3, `chat_stream`) is also thinking-off for latency
    /// — see converse.rs step 9.
    pub async fn chat_reflection(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResult, LlmError> {
        let no_thinking = ThinkingConfig { type_: "disabled".to_string() };
        self.chat_with_model(
            &self.reflection_model,
            messages,
            temperature,
            max_tokens,
            Some(&no_thinking),
            None,
        )
        .await
    }

    async fn chat_with_model(
        &self,
        model: &str,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        thinking: Option<&ThinkingConfig>,
        tools: Option<&[ToolDef]>,
    ) -> Result<ChatResult, LlmError> {
        let url = self.build_url();

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            temperature,
            max_tokens,
            stream: Some(false),
            stream_options: None,
            thinking: thinking.cloned(),
            reasoning_effort: None,
            tools: tools.map(|t| t.to_vec()),
            tool_choice: tools.map(|_| "auto".to_string()),
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

        let choice = chat_resp.choices.into_iter().next();
        let (content, tool_calls, finish_reason) = match choice {
            Some(c) => (
                c.message.content.unwrap_or_default(),
                c.message.tool_calls,
                c.finish_reason,
            ),
            None => (String::new(), None, None),
        };
        // Only warn when BOTH content is empty AND no tool calls — a tool-request
        // round legitimately has content:null + tool_calls (not an error).
        if content.trim().is_empty() && tool_calls.is_none() {
            log::warn!(
                "[llm-empty-content] model={} body_len={} body={}",
                model,
                body.len(),
                body
            );
        }

        let usage = chat_resp.usage.unwrap_or(Usage {
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        let result = ChatResult {
            content,
            prompt_cache_hit_tokens: usage.prompt_cache_hit_tokens,
            prompt_cache_miss_tokens: usage.prompt_cache_miss_tokens,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            tool_calls,
            finish_reason,
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
        thinking: Option<&ThinkingConfig>,
        reasoning_effort: Option<&str>,
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
            thinking: thinking.cloned(),
            reasoning_effort: reasoning_effort.map(|s| s.to_string()),
            tools: None,
            tool_choice: None,
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
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });
        ChatResult {
            content,
            prompt_cache_hit_tokens: usage.prompt_cache_hit_tokens,
            prompt_cache_miss_tokens: usage.prompt_cache_miss_tokens,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            tool_calls: None,
            finish_reason: None,
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
    fn test_chat_request_serializes_tools() {
        // A tool-calling request must serialize a `tools` array + `tool_choice`.
        let req_with_tools = ChatRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage::user("hi")],
            temperature: None,
            max_tokens: None,
            stream: Some(false),
            stream_options: None,
            thinking: None,
            reasoning_effort: None,
            tools: Some(vec![ToolDef::new(
                "get_time",
                "Get the current local time",
                serde_json::json!({"type": "object", "properties": {}}),
            )]),
            tool_choice: Some("auto".to_string()),
        };
        let json = serde_json::to_string(&req_with_tools).unwrap();
        assert!(json.contains("\"tools\""), "tools field missing: {}", json);
        assert!(json.contains("\"get_time\""));
        assert!(json.contains("\"tool_choice\":\"auto\""));

        // A plain request (no tools) must omit tools/tool_choice entirely so the
        // provider treats it as a normal completion.
        let plain = ChatRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage::user("hi")],
            temperature: None,
            max_tokens: None,
            stream: Some(false),
            stream_options: None,
            thinking: None,
            reasoning_effort: None,
            tools: None,
            tool_choice: None,
        };
        let plain_json = serde_json::to_string(&plain).unwrap();
        assert!(!plain_json.contains("tools"), "plain request should omit tools: {}", plain_json);
        assert!(!plain_json.contains("tool_choice"));
    }

    #[test]
    fn test_parse_tool_call_response() {
        // A tool-request round: content is null, tool_calls present,
        // finish_reason == "tool_calls" — the agent loop keys off this.
        let body = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {"name": "search_web", "arguments": "{\"query\":\"AI news\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        }"#;
        let resp: ChatResponse = serde_json::from_str(body).unwrap();
        let choice = resp.choices.into_iter().next().unwrap();
        assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
        assert!(
            choice.message.content.is_none(),
            "tool-request round content should be null"
        );
        let tc = choice.message.tool_calls.expect("tool_calls missing");
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "call_abc");
        assert_eq!(tc[0].function.name, "search_web");
        // arguments is a JSON string, not a parsed object.
        assert_eq!(tc[0].function.arguments, r#"{"query":"AI news"}"#);
    }

    #[test]
    fn test_parse_plain_response_no_tools() {
        // A normal answer round: content present, no tool_calls, finish stop.
        let body = r#"{
            "choices": [{
                "message": {"role": "assistant", "content": "你好呀"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }"#;
        let resp: ChatResponse = serde_json::from_str(body).unwrap();
        let choice = resp.choices.into_iter().next().unwrap();
        assert_eq!(choice.finish_reason.as_deref(), Some("stop"));
        assert_eq!(choice.message.content.as_deref(), Some("你好呀"));
        assert!(choice.message.tool_calls.is_none());
    }

    #[test]
    fn test_chat_message_helpers() {
        let u = ChatMessage::user("hello");
        assert_eq!(u.role, "user");
        assert_eq!(u.content.as_deref(), Some("hello"));
        assert!(u.tool_calls.is_none());

        let s = ChatMessage::system("sys");
        assert_eq!(s.content_str(), "sys");

        let tool_msg = ChatMessage::tool_result("call_1", "get_time", "14:00");
        assert_eq!(tool_msg.role, "tool");
        assert_eq!(tool_msg.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(tool_msg.name.as_deref(), Some("get_time"));
        assert_eq!(tool_msg.content_str(), "14:00");

        // assistant_with_tool_calls: null content round.
        let tc = ToolCall {
            id: "x".to_string(),
            type_: "function".to_string(),
            function: ToolCallFunction {
                name: "search_web".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let asst = ChatMessage::assistant_with_tool_calls(None, vec![tc]);
        assert_eq!(asst.content_str(), ""); // None → empty
        assert!(asst.tool_calls.is_some());
    }

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
