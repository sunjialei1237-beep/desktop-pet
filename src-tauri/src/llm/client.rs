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
    /// OpenAI-compatible relays (e.g. Agnes) report cache hits as
    /// `prompt_tokens_details.cached_tokens` instead of DeepSeek's flat
    /// fields — normalized into the same observability in `chat_with_model`.
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Debug, Deserialize)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
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
    /// Prefix-cache hit/miss input tokens (DeepSeek reports them per call; a
    /// hit costs ~1/31 of a miss on v4-flash, so this is THE cost lever).
    /// Absent when the provider omits the fields — both stay 0 then.
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
}

impl Default for LlmCostStats {
    fn default() -> Self {
        Self {
            date: local_today(),
            calls: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cache_hit_tokens: 0,
            cache_miss_tokens: 0,
        }
    }
}

impl LlmCostStats {
    /// Records one successful call's usage, resetting totals if the local day
    /// rolled over since the last record.
    fn record(
        &mut self,
        prompt_tokens: u32,
        completion_tokens: u32,
        cache_hit: Option<u32>,
        cache_miss: Option<u32>,
    ) {
        let today = local_today();
        if self.date != today {
            *self = LlmCostStats::default();
        }
        self.calls += 1;
        self.prompt_tokens += prompt_tokens as u64;
        self.completion_tokens += completion_tokens as u64;
        if let (Some(h), Some(m)) = (cache_hit, cache_miss) {
            self.cache_hit_tokens += h as u64;
            self.cache_miss_tokens += m as u64;
        }
    }

    /// Today's prefix-cache hit rate over reported cache tokens, 0..=1.
    /// 1.0 = every input token was a cache hit. None when no cache usage was
    /// reported yet (provider omission or zero calls).
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let total = self.cache_hit_tokens + self.cache_miss_tokens;
        if total == 0 {
            None
        } else {
            Some(self.cache_hit_tokens as f64 / total as f64)
        }
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
    /// accounting, never breaks the call. Also logs the per-call prefix-cache
    /// split — cache discipline observability (a 31x price gap between a
    /// cache hit and a miss on v4-flash makes this the #1 cost lever).
    fn track_usage(&self, result: &ChatResult) {
        if let (Some(hit), Some(miss)) = (result.prompt_cache_hit_tokens, result.prompt_cache_miss_tokens) {
            let total = hit + miss;
            if total > 0 {
                log::info!(
                    "[llm-cache] hit={} miss={} rate={:.1}% (prompt {})",
                    hit,
                    miss,
                    hit as f64 / total as f64 * 100.0,
                    result.prompt_tokens
                );
            }
        }
        if let Ok(mut stats) = self.cost.lock() {
            stats.record(
                result.prompt_tokens,
                result.completion_tokens,
                result.prompt_cache_hit_tokens,
                result.prompt_cache_miss_tokens,
            );
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
    ///
    /// Thinking is explicitly disabled here, matching every other production
    /// call: otherwise DeepSeek-v4 returns `reasoning_content` instead of
    /// `content` and requires the dropped reasoning to be echoed back on the
    /// next tool round (HTTP 400). The same fix also lets tool-call rounds get
    /// real answer text under a tight max_tokens budget.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDef]>,
    ) -> Result<ChatResult, LlmError> {
        let no_thinking = ThinkingConfig::disabled();
        self.chat_with_model(
            &self.main_model,
            messages,
            temperature,
            max_tokens,
            Some(&no_thinking),
            tools,
            false,
        )
        .await
    }

    /// Tool loop with `tool_choice:"required"` for exactly one deterministic
    /// continuation round (FS consent follow-up). Same model, same thinking-off.
    pub async fn chat_required_tools(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: &[ToolDef],
    ) -> Result<ChatResult, LlmError> {
        let no_thinking = ThinkingConfig::disabled();
        self.chat_with_model(
            &self.main_model,
            messages,
            temperature,
            max_tokens,
            Some(&no_thinking),
            Some(tools),
            true,
        )
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
            false,
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
        force_tool_call: bool,
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
            // "auto" lets the LLM answer without tools; "required" is used for
            // one deterministic continuation (FS consent follow-up "可以" —
            // live testing showed DeepSeek otherwise waits to be asked twice).
            tool_choice: tools.map(|_| {
                if force_tool_call {
                    "required".to_string()
                } else {
                    "auto".to_string()
                }
            }),
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

        let mut usage = chat_resp.usage.unwrap_or(Usage {
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens_details: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });
        // Normalize relay-style cache accounting onto the DeepSeek field so
        // cost observability works across providers.
        if usage.prompt_cache_hit_tokens.is_none() {
            if let Some(cached) = usage.prompt_tokens_details.as_ref().and_then(|d| d.cached_tokens) {
                usage.prompt_cache_hit_tokens = Some(cached);
                usage.prompt_cache_miss_tokens =
                    Some(usage.prompt_tokens.saturating_sub(cached));
            }
        }

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
        // Multi-provider URL assembly (provider-matrix compat, 2026-08-24).
        // Accepted base_url shapes:
        //   "https://api.deepseek.com/v1"                    → +/chat/completions
        //   "https://api.deepseek.com"      (bare host)      → +/v1/chat/completions
        //   "https://open.bigmodel.cn/api/paas/v4" (version) → +/chat/completions
        //   "https://apihub.agnes-ai.com/v1/chat/completions" → as-is (full endpoint;
        //     some relays tolerate a doubled path, most providers 404 on it)
        let url = self.base_url.trim_end_matches('/');
        if url.ends_with("chat/completions") {
            return url.to_string();
        }
        // Path presence: anything after the host (and port) begins at the
        // first '/' following "://".
        let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        let has_path = after_scheme.contains('/');
        if !has_path || url.ends_with("/v1") {
            format!("{}/v1/chat/completions", url).replace("/v1/v1/", "/v1/")
        } else {
            format!("{}/chat/completions", url)
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
        let mut usage = usage.unwrap_or(Usage {
            prompt_cache_hit_tokens: None,
            prompt_cache_miss_tokens: None,
            prompt_tokens_details: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });
        // Relay-style cache accounting normalized here too (streaming path).
        if usage.prompt_cache_hit_tokens.is_none() {
            if let Some(cached) = usage.prompt_tokens_details.as_ref().and_then(|d| d.cached_tokens) {
                usage.prompt_cache_hit_tokens = Some(cached);
                usage.prompt_cache_miss_tokens = Some(usage.prompt_tokens.saturating_sub(cached));
            }
        }
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

    fn client_for(base: &str) -> LlmClient {
        LlmClient::new(base, "test-key", "m", "m").expect("client")
    }

    #[test]
    fn build_url_accepts_all_provider_shapes() {
        // DeepSeek classic versioned base.
        assert_eq!(
            client_for("https://api.deepseek.com/v1").build_url(),
            "https://api.deepseek.com/v1/chat/completions"
        );
        // Bare host (OpenAI style) gets /v1 inserted.
        assert_eq!(
            client_for("https://api.openai.com").build_url(),
            "https://api.openai.com/v1/chat/completions"
        );
        // Non-/v1 versioned path (Zhipu GLM official) appends directly —
        // the old code produced /api/paas/v4/v1/chat/completions (404).
        assert_eq!(
            client_for("https://open.bigmodel.cn/api/paas/v4").build_url(),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        // Full endpoint (relay style) is used verbatim — the old code
        // doubled the path (/v1/chat/completions/v1/chat/completions).
        assert_eq!(
            client_for("https://apihub.agnes-ai.com/v1/chat/completions").build_url(),
            "https://apihub.agnes-ai.com/v1/chat/completions"
        );
        // Trailing slash tolerated everywhere.
        assert_eq!(
            client_for("https://api.deepseek.com/v1/").build_url(),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn usage_normalizes_relay_cached_tokens() {
        // Agnes-style usage: prompt_tokens_details.cached_tokens instead of
        // DeepSeek's flat prompt_cache_hit_tokens.
        let body = r#"{
            "prompt_tokens": 400, "completion_tokens": 30, "total_tokens": 430,
            "prompt_tokens_details": {"cached_tokens": 256}
        }"#;
        let u: Usage = serde_json::from_str(body).unwrap();
        assert_eq!(u.prompt_cache_hit_tokens, None); // DeepSeek field absent
        let mut u = u;
        if u.prompt_cache_hit_tokens.is_none() {
            if let Some(c) = u.prompt_tokens_details.as_ref().and_then(|d| d.cached_tokens) {
                u.prompt_cache_hit_tokens = Some(c);
                u.prompt_cache_miss_tokens = Some(u.prompt_tokens.saturating_sub(c));
            }
        }
        assert_eq!(u.prompt_cache_hit_tokens, Some(256));
        assert_eq!(u.prompt_cache_miss_tokens, Some(144));
        // DeepSeek-native shape still parses unchanged.
        let ds: Usage = serde_json::from_str(
            r#"{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12,
                "prompt_cache_hit_tokens":8,"prompt_cache_miss_tokens":2}"#,
        )
        .unwrap();
        assert_eq!(ds.prompt_cache_hit_tokens, Some(8));
    }

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
        stats.record(10, 5, Some(6), Some(4));
        stats.record(20, 8, Some(15), Some(5));
        assert_eq!(stats.calls, 2);
        assert_eq!(stats.prompt_tokens, 30);
        assert_eq!(stats.completion_tokens, 13);
        assert_eq!(stats.cache_hit_tokens, 21);
        assert_eq!(stats.cache_miss_tokens, 9);
        assert!((stats.cache_hit_rate().unwrap() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn test_cost_record_resets_on_day_rollover() {
        // A stats block stamped to a long-past date must reset before recording.
        let mut stats = LlmCostStats {
            date: "2020-01-01".to_string(),
            calls: 99,
            prompt_tokens: 999,
            completion_tokens: 999,
            cache_hit_tokens: 999,
            cache_miss_tokens: 999,
        };
        stats.record(10, 4, Some(7), Some(3));
        assert_eq!(stats.calls, 1); // reset, then +1
        assert_eq!(stats.prompt_tokens, 10);
        assert_eq!(stats.completion_tokens, 4);
        assert_eq!(stats.cache_hit_tokens, 7);
        assert_eq!(stats.cache_miss_tokens, 3);
        assert_eq!(stats.date, local_today());
    }

    #[test]
    fn test_cost_snapshot_zeroes_when_stale() {
        let stats = LlmCostStats {
            date: "2020-01-01".to_string(),
            calls: 99,
            prompt_tokens: 999,
            completion_tokens: 999,
            cache_hit_tokens: 999,
            cache_miss_tokens: 999,
        };
        let snap = stats.snapshot_today();
        assert_eq!(snap.calls, 0);
        assert_eq!(snap.prompt_tokens, 0);
        assert_eq!(snap.date, local_today());
    }

    #[test]
    fn test_cost_cache_rate_none_without_usage() {
        let stats = LlmCostStats::default();
        assert!(stats.cache_hit_rate().is_none(), "no cache usage reported → no rate");
    }
}
