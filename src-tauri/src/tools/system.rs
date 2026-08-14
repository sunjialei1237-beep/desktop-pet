//! System-interaction tools: `get_time`, `open_application`, `open_url`.
//!
//! `get_time` is the Agent-Runtime smoke test (终局是 prompt 直接注入时间，
//! see Phase 6 — but the tool stays as a runtime verification of the loop).
//! `open_application` / `open_url` act on the user's computer, so they are
//! defense-in-depth: policy.rs checks the whitelist/https first, and these
//! functions re-verify before spawning a process.

use chrono::Datelike;
use std::path::{Path, PathBuf};

use crate::perception::time::{current_time_of_day, TimeOfDay};

use super::policy::ToolStatus;
use super::ToolResult;

/// One launchable app discovered on the user's machine (a Desktop / Start Menu
/// shortcut). `name` is the friendly name (filename without `.lnk`); `path` is
/// the full path to the `.lnk` we hand to the shell.
struct AppEntry {
    name: String,
    path: String,
}

/// Scan Desktop + Start Menu shortcuts for launchable apps. These are apps the
/// USER placed or installed — a trusted surface, so any of them may be opened.
/// This REPLACES the static whitelist: the pet discovers what's available,
/// nothing needs pre-configuring.
fn scan_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dirs_to_scan: Vec<PathBuf> = vec![
        home.join("Desktop"),
        home.join("OneDrive").join("Desktop"), // synced setups
        // %APPDATA%\Microsoft\Windows\Start Menu\Programs (per-user)
        dirs::data_dir()
            .map(|d| d.join("Microsoft").join("Windows").join("Start Menu").join("Programs"))
            .unwrap_or_default(),
        // C:\ProgramData\...\Start Menu\Programs (all-users)
        PathBuf::from("C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs"),
        // %PUBLIC%\Desktop (all-users desktop)
        dirs::public_dir().map(|d| d.join("Desktop")).unwrap_or_default(),
    ];
    for dir in &dirs_to_scan {
        scan_dir(dir, &mut apps);
    }
    // Dedup by name (case-insensitive), keeping the first occurrence.
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

fn scan_dir(dir: &Path, apps: &mut Vec<AppEntry>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, apps); // recurse into program groups
        } else if path.extension().and_then(|e| e.to_str()) == Some("lnk") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                apps.push(AppEntry {
                    name: stem.to_string(),
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
    }
}

/// Match a user-spoken app name against discovered shortcuts. Order: exact
/// (ignoring spaces/case) → shortcut name contains query → query contains
/// shortcut name. Returns the best guess, or None if nothing plausible.
fn fuzzy_match_app<'a>(query: &str, apps: &'a [AppEntry]) -> Option<&'a AppEntry> {
    let q = query.to_lowercase();
    let q_nospace: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    // 1. Exact match (ignoring spaces/case).
    if let Some(a) = apps.iter().find(|a| {
        a.name
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            == q_nospace
    }) {
        return Some(a);
    }
    // 2. Shortcut name contains the query ("网易云" ⊂ "网易云音乐").
    if let Some(a) = apps.iter().find(|a| a.name.to_lowercase().contains(&q)) {
        return Some(a);
    }
    // 3. Query contains the shortcut name ("打开网易云音乐" ⊃ "网易云音乐").
    apps.iter()
        .find(|a| q.contains(&a.name.to_lowercase()))
}

/// `get_time`: current local time + weekday + date + time-of-day bucket.
/// Reuses `perception::time` so the bucket matches the rest of the system.
pub async fn get_time(_args: &serde_json::Value) -> ToolResult {
    let now = chrono::Local::now();
    let weekday_cn = [
        "周一", "周二", "周三", "周四", "周五", "周六", "周日",
    ][now.weekday().num_days_from_monday() as usize];
    let content = format!(
        "现在是 {} {} {}\n时段：{}",
        now.format("%H:%M"),
        weekday_cn,
        now.format("%Y-%m-%d"),
        time_of_day_cn(current_time_of_day()),
    );
    ToolResult {
        status: ToolStatus::Success,
        content,
    }
}

fn time_of_day_cn(tod: TimeOfDay) -> &'static str {
    match tod {
        TimeOfDay::Morning => "上午",
        TimeOfDay::Afternoon => "下午",
        TimeOfDay::Evening => "晚上",
        TimeOfDay::LateNight => "深夜",
        TimeOfDay::DeepNight => "凌晨",
    }
}

/// `open_application`: discover and launch an app by spoken name. Scans the
/// user's Desktop + Start Menu shortcuts (`.lnk`) and fuzzy-matches the
/// requested name, then hands the resolved shortcut to `explorer` (which opens
/// the real target through the shell). No static whitelist — the pet finds what
/// the user actually has installed.
pub async fn open_application(args: &serde_json::Value) -> ToolResult {
    let app = args.get("app").and_then(|a| a.as_str()).unwrap_or("");
    if app.trim().is_empty() {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "没有指定要打开的程序。".to_string(),
        };
    }
    // Defense-in-depth: policy already blocked path traversal, re-check.
    if app.contains('/') || app.contains('\\') || app.contains("..") {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "只能用程序名，不能用路径。".to_string(),
        };
    }

    // Dynamic discovery: match the requested app against Desktop + Start Menu
    // shortcuts. Replaces the static whitelist — nothing needs pre-configuring.
    let apps = scan_apps();
    log::info!("[tools] open_application: {} shortcuts discovered", apps.len());
    let target = match fuzzy_match_app(app, &apps) {
        Some(t) => t,
        None => {
            log::info!("[tools] open_application: no shortcut matched {:?}", app);
            return ToolResult {
                status: ToolStatus::Failed,
                content: format!("没在桌面或开始菜单找到叫「{}」的程序。", app),
            };
        }
    };
    log::info!(
        "[tools] open_application: {:?} matched shortcut \"{}\"",
        app,
        target.name
    );

    // Open the .lnk through explorer (the shell resolves the real target).
    // No CREATE_NO_WINDOW needed: explorer is a GUI app, no console spawned.
    match std::process::Command::new("explorer").arg(&target.path).spawn() {
        Ok(_) => {
            log::info!(
                "[tools] open_application: launched {} via {}",
                target.name,
                target.path
            );
            ToolResult {
                status: ToolStatus::Success,
                content: format!("已经帮你打开 {} 了。", target.name),
            }
        }
        Err(e) => {
            log::warn!("[tools] open_application {} failed: {}", target.path, e);
            ToolResult {
                status: ToolStatus::Failed,
                content: format!("没能打开 {}：{}", target.name, e),
            }
        }
    }
}

/// `open_url`: open an https URL in the default browser. Bypasses `cmd` (whose
/// `&` splitting mangles query strings) by calling `explorer.exe` directly —
/// `explorer <https-url>` forwards to the default browser, and CreateProcess
/// passes the whole URL as one argv element so `&`/`=` in queries survive.
pub async fn open_url(args: &serde_json::Value) -> ToolResult {
    let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("");
    // Defense-in-depth: re-check https-only.
    if !url.starts_with("https://") {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "只支持 https 开头的网址。".to_string(),
        };
    }

    match std::process::Command::new("explorer").arg(url).spawn() {
        Ok(_) => {
            log::info!("[tools] open_url: {}", url);
            ToolResult {
                status: ToolStatus::Success,
                content: format!("已经在浏览器打开了这个网址。"),
            }
        }
        Err(e) => {
            log::warn!("[tools] open_url {} failed: {}", url, e);
            ToolResult {
                status: ToolStatus::Failed,
                content: format!("打不开这个网址：{}", e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_time_returns_success() {
        let r = get_time(&serde_json::json!({})).await;
        assert_eq!(r.status, ToolStatus::Success);
        assert!(r.content.contains("现在是"));
        assert!(r.content.contains("时段"));
    }

    #[tokio::test]
    async fn test_open_application_rejects_empty() {
        let r = open_application(&serde_json::json!({"app": ""})).await;
        assert_eq!(r.status, ToolStatus::Rejected);
    }

    #[tokio::test]
    async fn test_open_application_rejects_path_traversal() {
        // Defense-in-depth: only names, never paths.
        let r = open_application(&serde_json::json!({"app": "../evil"})).await;
        assert_eq!(r.status, ToolStatus::Rejected);
        assert!(r.content.contains("路径"));
    }

    #[test]
    fn test_fuzzy_match_substring() {
        let apps = vec![
            AppEntry { name: "网易云音乐".to_string(), path: "a.lnk".to_string() },
            AppEntry { name: "Chrome".to_string(), path: "b.lnk".to_string() },
        ];
        // "网易云" ⊂ "网易云音乐"
        assert_eq!(fuzzy_match_app("网易云", &apps).unwrap().name, "网易云音乐");
        // case-insensitive
        assert_eq!(fuzzy_match_app("chrome", &apps).unwrap().name, "Chrome");
    }

    #[test]
    fn test_fuzzy_match_none() {
        let apps = vec![AppEntry {
            name: "Chrome".to_string(),
            path: "b.lnk".to_string(),
        }];
        assert!(fuzzy_match_app("完全不存在的应用xyz", &apps).is_none());
    }

    #[tokio::test]
    async fn test_open_url_rejects_non_https() {
        let r = open_url(&serde_json::json!({"url": "http://example.com"})).await;
        assert_eq!(r.status, ToolStatus::Rejected);
    }

    #[tokio::test]
    async fn test_open_url_rejects_missing() {
        let r = open_url(&serde_json::json!({})).await;
        assert_eq!(r.status, ToolStatus::Rejected);
    }
}
