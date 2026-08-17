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

/// Extensions `open_file` may hand to the shell. Everything immediately
/// executable by association (.bat/.cmd/.vbs/.exe/.msi/.lnk/.ps1) and
/// macro-capable Office formats (.docm/.xlsm) is absent BY DESIGN — the
/// association is the attack surface (plan §3.5 FS-A2).
const LAUNCHABLE_EXTENSIONS: &[&str] = &[
    // text / config / notes
    "txt", "md", "markdown", "rtf", "log", "csv", "json", "toml", "yaml", "yml", "ini", "conf",
    // documents / images
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff",
    // audio / video
    "mp3", "wav", "m4a", "flac", "mp4", "mov", "mkv", "avi",
    // source code (default association is an editor, never an interpreter)
    "rs", "ts", "tsx", "py", "rb", "java", "c", "cpp", "h", "hpp", "cs", "go", "kt", "swift",
    "sql",
];

/// Pure extension gate (lowercased input), public so execute-time can report
/// the same deny reason without duplicating the list.
pub fn is_launchable_extension(ext: &str) -> bool {
    LAUNCHABLE_EXTENSIONS.contains(&ext)
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
            // No static whitelist: the tool discovers apps by scanning Desktop
            // + Start Menu shortcuts (system.rs::scan_apps) and fuzzy-matches.
            // Policy here only gates config + blocks path traversal.
            PolicyDecision::Allow
        }
        // open_file = code-execution vector via explorer file association
        // (plan §3.5 FS-A2). Hard policy: canonicalize-first, must be an
        // existing FILE, sensitive names / pet's own AppData are hard-denied,
        // and only safe-launch extensions pass.
        super::ToolKind::OpenFile => {
            if !cfg.enable_open_application {
                return PolicyDecision::Deny("open_file_disabled");
            }
            let raw = match args.get("path").and_then(|p| p.as_str()) {
                Some(p) if !p.trim().is_empty() => p,
                _ => return PolicyDecision::Deny("invalid_path"),
            };
            let canonical = match dunce::canonicalize(raw) {
                Ok(c) => c,
                Err(_) => return PolicyDecision::Deny("path_not_found"),
            };
            if !canonical.is_file() {
                return PolicyDecision::Deny("not_a_file");
            }
            if crate::tools::path::root_contains(&crate::config::app_data_dir(), &canonical) {
                return PolicyDecision::Deny("sensitive_file");
            }
            let name = canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if crate::tools::path::is_sensitive_name(&name) {
                return PolicyDecision::Deny("sensitive_file");
            }
            let ext = canonical
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if is_launchable_extension(&ext) {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Deny("extension_not_allowed")
            }
        }
        super::ToolKind::OpenUrl => match args.get("url").and_then(|u| u.as_str()) {
            Some(u) if u.starts_with("https://") => PolicyDecision::Allow,
            Some(_) => PolicyDecision::Deny("non_https_blocked"),
            None => PolicyDecision::Deny("invalid_url"),
        },
        // create_note writes ONLY into .liri/NOTES — policy checks the schema +
        // filename rules; quota + atomic write live in fs::create_note.
        super::ToolKind::CreateNote => {
            if !cfg.enable_fs_mutate {
                return PolicyDecision::Deny("fs_mutate_disabled");
            }
            let filename = match args.get("filename").and_then(|f| f.as_str()) {
                Some(f) => f,
                None => return PolicyDecision::Deny("invalid_filename"),
            };
            if super::fs::validate_note_filename(filename).is_err() {
                return PolicyDecision::Deny("invalid_filename");
            }
            match args.get("content").and_then(|c| c.as_str()) {
                Some(c) if !c.is_empty() && c.len() <= super::fs::NOTE_MAX_BYTES => {
                    PolicyDecision::Allow
                }
                Some(c) if c.len() > super::fs::NOTE_MAX_BYTES => {
                    PolicyDecision::Deny("note_too_large")
                }
                _ => PolicyDecision::Deny("invalid_content"),
            }
        }
        // Filesystem Observe tools: schema sanity ONLY here. Path
        // authorization (canonicalize + grants + denylist) runs at execute
        // time in path.rs — policy is stateless and has no DB/fs access.
        super::ToolKind::ReadTextFile => {
            if !cfg.enable_fs_observe {
                return PolicyDecision::Deny("fs_observe_disabled");
            }
            match args.get("path").and_then(|p| p.as_str()) {
                Some(p) if !p.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_path"),
            }
        }
        super::ToolKind::SearchFiles => {
            if !cfg.enable_fs_observe {
                return PolicyDecision::Deny("fs_observe_disabled");
            }
            match args.get("query").and_then(|q| q.as_str()) {
                Some(q) if !q.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_query"),
            }
        }
        super::ToolKind::ListDirectory => {
            if !cfg.enable_fs_observe {
                return PolicyDecision::Deny("fs_observe_disabled");
            }
            match args.get("path").and_then(|p| p.as_str()) {
                Some(p) if !p.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_path"),
            }
        }
        super::ToolKind::GetFileMetadata => {
            if !cfg.enable_fs_observe {
                return PolicyDecision::Deny("fs_observe_disabled");
            }
            match args.get("path").and_then(|p| p.as_str()) {
                Some(p) if !p.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_path"),
            }
        }
        super::ToolKind::GetGitContext => {
            if !cfg.enable_fs_observe {
                return PolicyDecision::Deny("fs_observe_disabled");
            }
            match args.get("project_id").and_then(|s| s.as_str()) {
                Some(s) if !s.trim().is_empty() => PolicyDecision::Allow,
                _ => PolicyDecision::Deny("invalid_project"),
            }
        }
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
            enable_fs_observe: false,
            enable_fs_mutate: false,
        }
    }

    fn cfg_fs() -> ToolsConfig {
        ToolsConfig {
            enable_search_web: false,
            enable_open_application: false,
            enable_fs_observe: true,
            enable_fs_mutate: false,
        }
    }

    fn cfg_mutate() -> ToolsConfig {
        ToolsConfig {
            enable_search_web: false,
            enable_open_application: false,
            enable_fs_observe: false,
            enable_fs_mutate: true,
        }
    }

    #[test]
    fn create_note_schema_and_switch() {
        assert!(matches!(
            check(
                ToolKind::CreateNote,
                &serde_json::json!({"filename": "体检提醒.md", "content": "下周三复查"}),
                &cfg_mutate()
            ),
            PolicyDecision::Allow
        ));
        for bad in [
            serde_json::json!({"filename": "..\\x.md", "content": "a"}),
            serde_json::json!({"filename": "a/b", "content": "a"}),
            serde_json::json!({"filename": ".env", "content": "a"}),
            serde_json::json!({"filename": "ok.md", "content": ""}),
            serde_json::json!({"content": "a"}),
        ] {
            assert!(
                matches!(check(ToolKind::CreateNote, &bad, &cfg_mutate()), PolicyDecision::Deny(_)),
                "should deny: {bad}"
            );
        }
        assert!(matches!(
            check(
                ToolKind::CreateNote,
                &serde_json::json!({"filename": "ok.md", "content": "a"}),
                &cfg_fs()
            ),
            PolicyDecision::Deny("fs_mutate_disabled")
        ));
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
    fn open_app_allows_any_bare_name() {
        // No whitelist now — policy allows any bare name; matching against
        // discovered Desktop/Start-Menu shortcuts happens at execute time.
        // "malware" passes policy but will fail at execute (no shortcut matches).
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"code"}), &cfg(true, true)),
            PolicyDecision::Allow
        ));
        assert!(matches!(
            check(ToolKind::OpenApplication, &serde_json::json!({"app":"网易云音乐"}), &cfg(true, true)),
            PolicyDecision::Allow
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
    fn open_file_policy_extension_allowlist() {
        // Real temp files so canonicalize + is_file behave like production.
        let dir = std::env::temp_dir().join(format!(
            "pet_open_file_policy_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ok_exts = ["txt", "rs", "TS", "mp4", "pdf"];
        for (i, ext) in ok_exts.iter().enumerate() {
            let f = dir.join(format!("doc_{i}.{ext}"));
            std::fs::write(&f, "x").unwrap();
            let verdict = check(
                ToolKind::OpenFile,
                &serde_json::json!({"path": f.to_string_lossy()}),
                &cfg(true, true),
            );
            assert!(
                matches!(verdict, PolicyDecision::Allow),
                ".{ext} should be launchable, got {verdict:?}"
            );
        }
        let danger_exts = ["bat", "exe", "lnk", "ps1", "vbs", "msi", "reg", "xlsm", "docm"];
        for (i, ext) in danger_exts.iter().enumerate() {
            let f = dir.join(format!("evil_{i}.{ext}"));
            std::fs::write(&f, "x").unwrap();
            let verdict = check(
                ToolKind::OpenFile,
                &serde_json::json!({"path": f.to_string_lossy()}),
                &cfg(true, true),
            );
            assert!(
                matches!(verdict, PolicyDecision::Deny("extension_not_allowed")),
                ".{ext} must be blocked, got {verdict:?}"
            );
        }
        // Missing path / directory / inside pet's own AppData.
        let missing = dir.join("no_such.txt");
        assert!(matches!(
            check(ToolKind::OpenFile, &serde_json::json!({"path": missing}), &cfg(true, true)),
            PolicyDecision::Deny("path_not_found")
        ));
        assert!(matches!(
            check(ToolKind::OpenFile, &serde_json::json!({"path": dir}), &cfg(true, true)),
            PolicyDecision::Deny("not_a_file")
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn open_file_policy_blocks_appdata_and_sensitive_names() {
        let app = crate::config::app_data_dir();
        std::fs::create_dir_all(&app).unwrap();
        let secret = app.join("notes.txt");
        std::fs::write(&secret, "x").unwrap();
        assert!(matches!(
            check(ToolKind::OpenFile, &serde_json::json!({"path": secret}), &cfg(true, true)),
            PolicyDecision::Deny("sensitive_file")
        ));
    }

    #[test]
    fn open_file_denied_when_disabled() {
        assert!(matches!(
            check(ToolKind::OpenFile, &serde_json::json!({"path": "D:\\a.txt"}), &cfg(false, false)),
            PolicyDecision::Deny("open_file_disabled")
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

    #[test]
    fn fs_tools_denied_when_observe_disabled() {
        let args = serde_json::json!({"path": "D:\\some\\file.rs"});
        assert!(matches!(
            check(ToolKind::ReadTextFile, &args, &cfg(true, true)),
            PolicyDecision::Deny("fs_observe_disabled")
        ));
        assert!(matches!(
            check(ToolKind::GetGitContext, &serde_json::json!({"project_id": "x"}), &cfg(true, true)),
            PolicyDecision::Deny("fs_observe_disabled")
        ));
    }

    #[test]
    fn fs_tools_schema_checked_when_enabled() {
        assert!(matches!(
            check(ToolKind::ReadTextFile, &serde_json::json!({"path": "D:\\a.rs"}), &cfg_fs()),
            PolicyDecision::Allow
        ));
        assert!(matches!(
            check(ToolKind::ReadTextFile, &serde_json::json!({"path": "  "}), &cfg_fs()),
            PolicyDecision::Deny("invalid_path")
        ));
        assert!(matches!(
            check(ToolKind::SearchFiles, &serde_json::json!({"query": "fn main"}), &cfg_fs()),
            PolicyDecision::Allow
        ));
        assert!(matches!(
            check(ToolKind::SearchFiles, &serde_json::json!({}), &cfg_fs()),
            PolicyDecision::Deny("invalid_query")
        ));
    }
}
