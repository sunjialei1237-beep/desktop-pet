//! Tool Policy: the hard safety gate (铁律 #1 — LLM 权限只缩小不扩大).
//!
//! `check()` runs BEFORE `execute()`: whitelist / config switch / schema
//! sanity. It is stateless — duplicate-query detection lives in the agent
//! loop (Phase 5), which needs cross-call history.

use crate::config::ToolsConfig;
use serde_json::Value;

/// Policy verdict for a single tool call.
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow,
    /// Denied, with a short machine reason key (e.g. "app_not_whitelisted").
    Deny(&'static str),
}

/// Outcome status of a tool execution (audit log + LLM feedback).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Success,
    /// Policy denied (whitelist / schema / config).
    Rejected,
    /// Exceeded the per-tool timeout.
    Timeout,
    /// Tool ran but errored (network / parse / provider down).
    Failed,
    /// Agent loop cancelled (stale run / hit max rounds).
    Cancelled,
}

/// Whitelisted applications for `open_application`. Bare exe names (no path) —
/// the OS PATH resolves them. Mirrors the classify_process array style in
/// `perception/window.rs`. Anything not listed here is denied. Deliberately
/// excludes shells (cmd/powershell) so the tool cannot indirectly run
/// arbitrary commands.
pub const ALLOWED_APPS: &[&str] = &[
    "code", "chrome", "msedge", "firefox",
    "explorer", "notepad", "calc", "mspaint", "snippingtool",
    "spotify", "discord", "slack", "wechat", "qq",
    "steam", "epicgameslauncher",
    "terminal", "wt", "devenv",
];

/// Check whether a tool call is permitted. Stateless — config + args only.
/// Duplicate-query detection is in the agent loop (needs history).
pub fn check(kind: super::ToolKind, args: &Value, cfg: &ToolsConfig) -> PolicyDecision {
    match kind {
        super::ToolKind::GetTime => PolicyDecision::Allow,
        super::ToolKind::SearchWeb => {
            if !cfg.enable_search_web {
                return PolicyDecision::Deny("search_disabled");
            }
            match args.get("query").and_then(|q| q.as_str()) {
                Some(q) if !q.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_query"),
            }
        }
        super::ToolKind::OpenApplication => {
            if !cfg.enable_open_application {
                return PolicyDecision::Deny("open_app_disabled");
            }
            let app = match args.get("app").and_then(|a| a.as_str()) {
                Some(a) => a,
                None => return PolicyDecision::Deny("invalid_app"),
            };
            // Reject path traversal / absolute paths — only bare names allowed.
            if app.contains('/') || app.contains('\\') || app.contains("..") {
                return PolicyDecision::Deny("path_traversal_blocked");
            }
            let lower = app.to_lowercase();
            if ALLOWED_APPS.iter().any(|&allowed| allowed == lower) {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny("app_not_whitelisted")
            }
        }
        super::ToolKind::OpenUrl => match args.get("url").and_then(|u| u.as_str()) {
            Some(u) if u.starts_with("https://") => PolicyDecision::Allow,
            Some(_) => PolicyDecision::Deny("non_https_blocked"),
            None => PolicyDecision::Deny("invalid_url"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolKind;

    fn cfg(search: bool, app: bool) -> ToolsConfig {
        ToolsConfig {
            enable_search_web: search,
            enable_open_application: app,
        }
    }

    #[test]
    fn get_time_always_allowed() {
        assert!(matches!(
            check(ToolKind::GetTime, &serde_json::json!({}), &cfg(false, false)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn search_allowed_when_enabled_valid_query() {
        assert!(matches!(
            check(ToolKind::SearchWeb, &serde_json::json!({"query":"AI news"}), &cfg(true, true)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn search_denied_when_disabled() {
        assert!(matches!(
            check(ToolKind::SearchWeb, &serde_json::json!({"query":"x"}), &cfg(false, true)),
            PolicyDecision::Deny("search_disabled")
        ));
    }

    #[test]
    fn search_denied_empty_query() {
        assert!(matches!(
            check(ToolKind::SearchWeb, &serde_json::json!({"query":""}), &cfg(true, true)),
            PolicyDecision::Deny("invalid_query")
        ));
    }

    #[test]
    fn open_app_whitelisted() {
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"code"}), &cfg(true, true)),
            PolicyDecision::Allow
        ));
        // case-insensitive
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"Chrome"}), &cfg(true, true)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn open_app_not_whitelisted() {
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"malware"}), &cfg(true, true)),
            PolicyDecision::Deny("app_not_whitelisted")
        ));
    }

    #[test]
    fn open_app_denied_when_disabled() {
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"code"}), &cfg(true, false)),
            PolicyDecision::Deny("open_app_disabled")
        ));
    }

    #[test]
    fn open_app_blocks_path_traversal() {
        // "../x", "C:\\evil", "a/b" all denied — only bare names.
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"../evil"}), &cfg(true, true)),
            PolicyDecision::Deny("path_traversal_blocked")
        ));
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"C:\\windows\\system32"}), &cfg(true, true)),
            PolicyDecision::Deny("path_traversal_blocked")
        ));
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"foo/bar"}), &cfg(true, true)),
            PolicyDecision::Deny("path_traversal_blocked")
        ));
    }

    #[test]
    fn open_url_https_allowed() {
        assert!(matches!(
            check(ToolKind::OpenUrl, &serde_json::json!({"url":"https://example.com"}), &cfg(true, true)),
            PolicyDecision::Allow
        ));
    }

    #[test]
    fn open_url_non_https_denied() {
        assert!(matches!(
            check(ToolKind::OpenUrl, &serde_json::json!({"url":"http://example.com"}), &cfg(true, true)),
            PolicyDecision::Deny("non_https_blocked")
        ));
        assert!(matches!(
            check(ToolKind::OpenUrl, &serde_json::json!({"url":"file:///etc/passwd"}), &cfg(true, true)),
            PolicyDecision::Deny("non_https_blocked")
        ));
    }

    #[test]
    fn open_url_invalid_denied() {
        assert!(matches!(
            check(ToolKind::OpenUrl, &serde_json::json!({}), &cfg(true, true)),
            PolicyDecision::Deny("invalid_url")
        ));
    }
}
