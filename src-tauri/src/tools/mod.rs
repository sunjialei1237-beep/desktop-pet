//! Tool Layer registry & dispatch (Phase 2 of the tool-layer plan).
//!
//! Three-gate model:
//!   Brain (Planner) → `CapabilityMode` (what *kind* of help, never a tool name)
//!   → `capability_to_tools` resolves Brain∩Policy (drops config-off tools)
//!   → `tool_defs_for` advertises the subset to the LLM
//!   → `policy::check` is the hard safety gate (whitelist/schema/config)
//!   → `execute` runs the tool
//!
//! 铁律 #1: LLM 权限只缩小不扩大 — the LLM only sees `capability_to_tools`
//! output and may pick from it; it cannot widen the set.
//!
//! Follows the enum+match registry style of `lifecycle/scheduler.rs` (ADR
//! 2026-08-07 rejected trait objects). Adding a tool = add a `ToolKind`
//! variant + a match arm — no dyn indirection.

use crate::config::ToolsConfig;
use crate::llm::client::ToolDef;

pub mod policy;

/// What *category* of external capability the Brain grants this turn. The
/// Planner emits this (not a tool name) — Brain never sees tool names. `None`
/// = plain conversation, no tools advertised (the overwhelming common case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityMode {
    None,
    /// Needs information from the outside world (search).
    ExternalInfo,
    /// Needs to act on the computer (open app / open url).
    ComputerAction,
    // SystemObservation reserved for later (get_cpu/memory etc).
}

impl Default for CapabilityMode {
    fn default() -> Self {
        CapabilityMode::None
    }
}

/// Concrete tool identifiers. Mirror the scheduler.rs enum-registry style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    GetTime,
    SearchWeb,
    OpenApplication,
    OpenUrl,
}

impl ToolKind {
    /// Stable tool name as advertised to the LLM (`function.name`).
    pub fn name(&self) -> &'static str {
        match self {
            ToolKind::GetTime => "get_time",
            ToolKind::SearchWeb => "search_web",
            ToolKind::OpenApplication => "open_application",
            ToolKind::OpenUrl => "open_url",
        }
    }
}

/// Outcome of executing a tool. `content` is the text fed back to the LLM
/// (wrapped in `<tool_result untrusted>` by the agent loop — 铁律 #2).
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub status: policy::ToolStatus,
    pub content: String,
}

/// Brain∩Policy: resolve a capability into the concrete tool subset, dropping
/// any tool whose config switch is off (Architecture #6: every capability
/// disableable). `get_time` / `open_url` are harmless and always included;
/// `search_web` / `open_application` respect their config flags.
pub fn capability_to_tools(cap: CapabilityMode, cfg: &ToolsConfig) -> Vec<ToolKind> {
    match cap {
        CapabilityMode::None => vec![],
        CapabilityMode::ExternalInfo => {
            let mut v = vec![ToolKind::GetTime];
            if cfg.enable_search_web {
                v.push(ToolKind::SearchWeb);
            }
            v
        }
        CapabilityMode::ComputerAction => {
            let mut v = vec![];
            if cfg.enable_open_application {
                v.push(ToolKind::OpenApplication);
            }
            v.push(ToolKind::OpenUrl); // https-only, harmless
            v
        }
    }
}

/// Build the LLM-facing `ToolDef` list for a tool subset. Each schema is kept
/// small (<~150 tokens) per the tool-layer token budget. Tool descriptions
/// embed the privacy constraint (search query must not leak persona/memory).
pub fn tool_defs_for(kinds: &[ToolKind]) -> Vec<ToolDef> {
    kinds.iter().map(|k| tool_def(*k)).collect()
}

fn tool_def(kind: ToolKind) -> ToolDef {
    match kind {
        ToolKind::GetTime => ToolDef::new(
            "get_time",
            "获取当前本地时间（时分、星期、日期）和时段（清晨/上午/下午/晚上/深夜）。",
            serde_json::json!({"type": "object", "properties": {}, "required": []}),
        ),
        ToolKind::SearchWeb => ToolDef::new(
            "search_web",
            "搜索互联网获取外部信息（新闻、天气、百科等）。只用用户当前消息构造搜索词，\
             不要把记忆或个人隐私信息（姓名/学校/公司）加入搜索词。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索关键词"}
                },
                "required": ["query"]
            }),
        ),
        ToolKind::OpenApplication => ToolDef::new(
            "open_application",
            "打开电脑上的应用程序。仅限白名单内的常用程序。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "app": {"type": "string", "description": "程序名（如 code/chrome/notepad）"}
                },
                "required": ["app"]
            }),
        ),
        ToolKind::OpenUrl => ToolDef::new(
            "open_url",
            "在默认浏览器打开一个 https 网址。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "https 网址"}
                },
                "required": ["url"]
            }),
        ),
    }
}

/// Execute a tool by dispatch. Phase 3 wires these to `tools::search` /
/// `tools::system`; until then this is a stub so the registry type-signature
/// is fixed and the agent loop (Phase 5) can compile against it.
pub async fn execute(kind: ToolKind, _args: &serde_json::Value, _cfg: &ToolsConfig) -> ToolResult {
    log::warn!("[tools] execute stub for {} (phase 3 not yet implemented)", kind.name());
    ToolResult {
        status: policy::ToolStatus::Failed,
        content: "此工具尚未实现。".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(search: bool, app: bool) -> ToolsConfig {
        ToolsConfig {
            enable_search_web: search,
            enable_open_application: app,
        }
    }

    #[test]
    fn capability_none_is_empty() {
        assert!(capability_to_tools(CapabilityMode::None, &cfg(true, true)).is_empty());
    }

    #[test]
    fn external_info_includes_search_when_enabled() {
        let tools = capability_to_tools(CapabilityMode::ExternalInfo, &cfg(true, true));
        assert!(tools.contains(&ToolKind::GetTime));
        assert!(tools.contains(&ToolKind::SearchWeb));
        assert!(!tools.contains(&ToolKind::OpenApplication));
    }

    #[test]
    fn external_info_drops_search_when_disabled() {
        // 铁律 #1 / #6: config-off tools never reach the LLM.
        let tools = capability_to_tools(CapabilityMode::ExternalInfo, &cfg(false, true));
        assert!(tools.contains(&ToolKind::GetTime));
        assert!(!tools.contains(&ToolKind::SearchWeb));
    }

    #[test]
    fn computer_action_includes_apps_and_url() {
        let tools = capability_to_tools(CapabilityMode::ComputerAction, &cfg(true, true));
        assert!(tools.contains(&ToolKind::OpenApplication));
        assert!(tools.contains(&ToolKind::OpenUrl));
        // get_time / search not in a computer-action turn.
        assert!(!tools.contains(&ToolKind::SearchWeb));
    }

    #[test]
    fn computer_action_drops_apps_when_disabled() {
        let tools = capability_to_tools(CapabilityMode::ComputerAction, &cfg(true, false));
        assert!(!tools.contains(&ToolKind::OpenApplication));
        assert!(tools.contains(&ToolKind::OpenUrl)); // url always on
    }

    #[test]
    fn tool_defs_names_match() {
        let defs = tool_defs_for(&[ToolKind::GetTime, ToolKind::SearchWeb]);
        assert_eq!(defs.len(), 2);
    }

    #[tokio::test]
    async fn execute_stub_returns_failed() {
        let r = execute(ToolKind::GetTime, &serde_json::json!({}), &cfg(true, true)).await;
        assert_eq!(r.status, policy::ToolStatus::Failed);
    }
}
