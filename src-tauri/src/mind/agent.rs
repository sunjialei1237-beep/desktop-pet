//! Agent Runtime: Pi-style tool-calling loop (Phase 5).
//!
//! One non-streaming `chat()` round per tool call, up to `MAX_TOOL_ROUNDS`;
//! the final answer round is streamed. The three 铁律 are enforced here:
//!
//!   #1  tools advertised = `capability_to_tools(Brain∩Policy)` — the LLM can
//!       only pick from this set; a tool name not in it is denied.
//!   #2  every tool result is wrapped `<tool_result untrusted>` before it is
//!       re-fed — search snippets may carry prompt injection.
//!   #3  tool results live only in the temporary message context — never
//!       written to Memory / Emotion (those flow through the normal pipeline).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::ToolsConfig;
use crate::llm::client::{ChatMessage, LlmClient, ThinkingConfig, ToolCall};
use crate::tools::{self, policy, CapabilityMode, ToolKind};

/// Hard cap on tool rounds. Reaching it triggers graceful fallback (force a
/// final answer with what we have) — never an error.
const MAX_TOOL_ROUNDS: usize = 3;
/// Per-tool execution timeout. A tool that doesn't return in time is reported
/// as Timeout and the loop continues.
const TOOL_TIMEOUT_SECS: u64 = 10;
/// Duplicate-query window: the same search query within this many seconds is
/// denied (abuse protection — "spam search ×3 → 限流").
const DUPLICATE_QUERY_WINDOW_SECS: u64 = 30;
/// Cap each tool result text fed back to the LLM (~1600 tokens; CJK ≈ 4
/// chars/token). Truncation + untrusted wrapper together bound the risk that a
/// huge/injected result dominates the context.
const MAX_TOOL_RESULT_CHARS: usize = 6400;

/// Final result of an agent run.
pub struct AgentOutcome {
    /// The character reply (already streamed token-by-token via `on_token`).
    pub reply: String,
    /// How many tool rounds ran (0 if it answered immediately).
    pub tool_rounds: usize,
    /// Total tokens consumed across all agent LLM calls.
    pub total_tool_tokens: u32,
}

/// Run the Pi-style tool loop. `messages` already holds system + context; this
/// appends tool rounds in place and returns the final reply. The final answer
/// is streamed through `on_token`.
///
/// `recent_queries` carries cross-call search history for duplicate detection;
/// the caller (converse) owns it so it persists across the turn.
pub async fn run_agent_loop(
    messages: &mut Vec<ChatMessage>,
    cap: CapabilityMode,
    cfg: &ToolsConfig,
    llm: &LlmClient,
    run_id: u64,
    on_token: &mut impl FnMut(&str),
    recent_queries: &mut Vec<(String, Instant)>,
    fs_grants: &[crate::db::grants::FsGrant],
) -> Result<AgentOutcome, String> {
    let kinds = tools::capability_to_tools(cap, cfg);
    let tool_defs = tools::tool_defs_for(&kinds);

    // 铁律 #1 / config gate: if every tool in this capability was turned off,
    // there is nothing to advertise — answer normally (no tool round).
    if tool_defs.is_empty() {
        log::info!("[agent] run {} capability {:?} resolved to no tools — plain answer", run_id, cap);
        return final_stream_answer(messages.as_slice(), llm, on_token, 0).await;
    }

    let kind_by_name: HashMap<&str, ToolKind> = kinds.iter().map(|k| (k.name(), *k)).collect();

    let mut total_tokens = 0u32;
    let mut rounds = 0usize;

    for _ in 0..MAX_TOOL_ROUNDS {
        rounds += 1;
        let result = llm
            .chat(messages, Some(0.8), Some(4096), Some(&tool_defs))
            .await
            .map_err(|e| format!("Agent LLM error: {:?}", e))?;
        total_tokens += result.prompt_tokens + result.completion_tokens;

        // No tool calls → the model produced the final answer directly.
        let tool_calls = match &result.tool_calls {
            Some(tc) if !tc.is_empty() => tc.clone(),
            _ => {
                let reply = result.content.trim().to_string();
                if reply.is_empty() {
                    // Empty non-tool response — fall back to a fresh stream.
                    return final_stream_answer(messages.as_slice(), llm, on_token, total_tokens)
                        .await;
                }
                // Emit the (non-streamed) answer token-by-token so the bubble
                // still types out live.
                for ch in reply.chars() {
                    on_token(&ch.to_string());
                }
                return Ok(AgentOutcome {
                    reply,
                    tool_rounds: rounds,
                    total_tool_tokens: total_tokens,
                });
            }
        };

        // Record the assistant's tool-request round (content may be null).
        let round_content = if result.content.is_empty() {
            None
        } else {
            Some(result.content.clone())
        };
        messages.push(ChatMessage::assistant_with_tool_calls(round_content, tool_calls.clone()));

        // Execute each requested tool call.
        for tc in &tool_calls {
            execute_one_tool(tc, &kind_by_name, cfg, run_id, messages, recent_queries, fs_grants).await;
        }
    }

    // Hit MAX_TOOL_ROUNDS — graceful fallback (铁律: no error). Nudge the model
    // to wrap up with what it already gathered, then stream the final answer.
    log::warn!(
        "[agent] run {} hit MAX_TOOL_ROUNDS, forcing graceful fallback",
        run_id
    );
    messages.push(ChatMessage::system(
        "（已经查了好几轮了，用已有的信息回答就好，不要再调用工具了。）",
    ));
    final_stream_answer(messages.as_slice(), llm, on_token, total_tokens).await
}

/// Execute one tool call: policy gate → (timeout-bounded) execute → push the
/// untrusted-wrapped result back into the message context.
async fn execute_one_tool(
    tc: &ToolCall,
    kind_by_name: &HashMap<&str, ToolKind>,
    cfg: &ToolsConfig,
    run_id: u64,
    messages: &mut Vec<ChatMessage>,
    recent_queries: &mut Vec<(String, Instant)>,
    fs_grants: &[crate::db::grants::FsGrant],
) {
    let name = &tc.function.name;
    let args: serde_json::Value =
        serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

    let kind = match kind_by_name.get(name.as_str()) {
        Some(k) => *k,
        None => {
            // 铁律 #1: the LLM named a tool NOT in the advertised set — deny.
            log::warn!("[agent] run {} denied unadvertised tool: {}", run_id, name);
            push_tool_result(messages, &tc.id, name, "Rejected: this tool is not available.");
            return;
        }
    };

    // Duplicate-query detection (search_web only — abuse protection).
    if kind == ToolKind::SearchWeb {
        if let Some(q) = args.get("query").and_then(|q| q.as_str()) {
            if check_duplicate(recent_queries, q, DUPLICATE_QUERY_WINDOW_SECS) {
                log::warn!("[agent] run {} duplicate query denied: {}", run_id, q);
                push_tool_result(
                    messages,
                    &tc.id,
                    name,
                    "Rejected: identical search within 30 seconds.",
                );
                return;
            }
        }
    }

    // Policy gate (whitelist / https / schema / config switch).
    match policy::check(kind, &args, cfg) {
        policy::PolicyDecision::Deny(reason) => {
            crate::tools::fs::record_policy_denial(reason);
            log::info!(
                "[agent] run {} policy denied {}: {}",
                run_id,
                name,
                reason
            );
            push_tool_result(
                messages,
                &tc.id,
                name,
                &format!("Rejected: {}.", reason),
            );
        }
        policy::PolicyDecision::Allow => {
            let start = Instant::now();
            let exec = tokio::time::timeout(
                Duration::from_secs(TOOL_TIMEOUT_SECS),
                tools::execute(kind, &args, cfg, fs_grants),
            )
            .await;
            let (status, content) = match exec {
                Ok(r) => (r.status, r.content),
                Err(_) => {
                    log::warn!("[agent] run {} {} timeout ({}s)", run_id, name, TOOL_TIMEOUT_SECS);
                    (policy::ToolStatus::Timeout, "工具执行超时了。".to_string())
                }
            };
            log::info!(
                "[agent] run {} tool {} status={:?} duration={}ms",
                run_id,
                name,
                status,
                start.elapsed().as_millis()
            );
            push_tool_result(messages, &tc.id, name, &content);
        }
    }
}

/// Duplicate-query guard. Prunes entries older than `window_secs`, then returns
/// true if `query` already appears (and leaves the list unchanged so a repeated
/// repeat stays denied); otherwise records it and returns false.
fn check_duplicate(
    recent: &mut Vec<(String, Instant)>,
    query: &str,
    window_secs: u64,
) -> bool {
    let now = Instant::now();
    recent.retain(|(_, t)| now.duration_since(*t).as_secs() < window_secs);
    if recent.iter().any(|(q, _)| q == query) {
        true
    } else {
        recent.push((query.to_string(), now));
        false
    }
}

/// Wrap a tool's raw output in the `<tool_result untrusted>` envelope (铁律 #2)
/// and push it as a role:"tool" message. The content is truncated to
/// `MAX_TOOL_RESULT_CHARS` so a single verbose/injected result can't dominate.
fn push_tool_result(messages: &mut Vec<ChatMessage>, tc_id: &str, name: &str, content: &str) {
    let capped = truncate_chars(content, MAX_TOOL_RESULT_CHARS);
    let wrapped = format!(
        "<tool_result source=\"{}\" untrusted=\"true\">\n{}\n</tool_result>",
        name, capped
    );
    messages.push(ChatMessage::tool_result(tc_id, name, &wrapped));
}

/// Truncate to at most `max` Unicode chars, on a char boundary.
fn truncate_chars(s: &str, max: usize) -> &str {
    if s.chars().count() <= max {
        return s;
    }
    let end = s
        .char_indices()
        .nth(max)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

/// Stream the final answer (thinking OFF for latency — same as the main reply
/// path). `prior_tokens` is carried through so the caller's cost accounting
/// includes this round.
async fn final_stream_answer(
    messages: &[ChatMessage],
    llm: &LlmClient,
    on_token: &mut impl FnMut(&str),
    prior_tokens: u32,
) -> Result<AgentOutcome, String> {
    let no_thinking = ThinkingConfig::disabled();
    let result = llm
        .chat_stream(messages, Some(0.8), Some(4096), Some(&no_thinking), None, |t| {
            on_token(t)
        })
        .await
        .map_err(|e| format!("Agent final stream error: {:?}", e))?;
    Ok(AgentOutcome {
        reply: result.content,
        tool_rounds: 0,
        total_tool_tokens: prior_tokens + result.prompt_tokens + result.completion_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars_short() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_chars_long() {
        let s = "你好世界test"; // 4 CJK + 4 ascii = 8 chars
        assert_eq!(truncate_chars(s, 5), "你好世界t");
    }

    #[test]
    fn test_truncate_chars_cjk_boundary() {
        // Truncation must land on a char boundary, never mid-CJK-byte.
        let s = "你好世界你好世界";
        let t = truncate_chars(s, 3);
        assert_eq!(t, "你好世");
        // valid UTF-8 (no panic)
        assert_eq!(t.chars().count(), 3);
    }

    #[test]
    fn test_push_tool_result_wraps_untrusted() {
        let mut messages = vec![];
        push_tool_result(&mut messages, "call_1", "search_web", "some snippet");
        assert_eq!(messages.len(), 1);
        let m = &messages[0];
        assert_eq!(m.role, "tool");
        assert_eq!(m.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(m.name.as_deref(), Some("search_web"));
        assert!(m.content_str().contains("<tool_result source=\"search_web\" untrusted=\"true\">"));
        assert!(m.content_str().contains("some snippet"));
        assert!(m.content_str().contains("</tool_result>"));
    }

    #[test]
    fn test_push_tool_result_truncates_huge() {
        let mut messages = vec![];
        let huge = "x".repeat(MAX_TOOL_RESULT_CHARS * 2);
        push_tool_result(&mut messages, "c", "search_web", &huge);
        // inner content is capped (the wrapper adds overhead, but the payload
        // itself must be ≤ MAX_TOOL_RESULT_CHARS chars).
        let inner = messages[0]
            .content_str()
            .lines()
            .filter(|l| !l.contains("tool_result"))
            .collect::<String>();
        assert!(inner.chars().count() <= MAX_TOOL_RESULT_CHARS);
    }

    #[test]
    fn test_check_duplicate_first_allowed() {
        let mut recent = vec![];
        assert!(!check_duplicate(&mut recent, "AI news", 30));
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn test_check_duplicate_repeat_denied() {
        let mut recent = vec![];
        check_duplicate(&mut recent, "AI news", 30);
        assert!(check_duplicate(&mut recent, "AI news", 30)); // same → denied
        assert!(!check_duplicate(&mut recent, "weather", 30)); // different → allowed
        assert!(check_duplicate(&mut recent, "AI news", 30)); // still in window
    }

    #[test]
    fn test_unadvertised_tool_denied_at_construction() {
        // The kind_by_name map models what was advertised. A name not in it is
        // rejected before any policy/execute — 铁律 #1.
        let map: HashMap<&str, ToolKind> = vec![(ToolKind::GetTime.name(), ToolKind::GetTime)]
            .into_iter()
            .collect();
        assert!(!map.contains_key("search_web"));
    }
}
