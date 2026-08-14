//! System-interaction tools: `get_time`, `open_application`, `open_url`.
//!
//! `get_time` is the Agent-Runtime smoke test (终局是 prompt 直接注入时间，
//! see Phase 6 — but the tool stays as a runtime verification of the loop).
//! `open_application` / `open_url` act on the user's computer, so they are
//! defense-in-depth: policy.rs checks the whitelist/https first, and these
//! functions re-verify before spawning a process.

use chrono::Datelike;

use crate::perception::time::{current_time_of_day, TimeOfDay};

use super::policy::{ToolStatus, ALLOWED_APPS};
use super::ToolResult;

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

/// `open_application`: launch a whitelisted app by bare name. Uses
/// `cmd /C start` so Windows' App-Paths registry resolves names like `chrome`
/// (not on PATH). The app arg is policy-validated as a bare whitelist name, so
/// it contains no shell metacharacters — safe to pass through `start`.
pub async fn open_application(args: &serde_json::Value) -> ToolResult {
    let app = args.get("app").and_then(|a| a.as_str()).unwrap_or("");
    // Defense-in-depth: re-check the whitelist (policy already did).
    let lower = app.to_lowercase();
    if lower.is_empty() {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "没有指定要打开的程序。".to_string(),
        };
    }
    if !ALLOWED_APPS.iter().any(|&allowed| allowed == lower) {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: format!("「{}」不在允许打开的程序列表里。", app),
        };
    }

    match std::process::Command::new("cmd")
        .args(["/C", "start", "", app])
        .spawn()
    {
        Ok(_) => {
            log::info!("[tools] open_application: launched {}", app);
            ToolResult {
                status: ToolStatus::Success,
                content: format!("已经帮你打开 {} 了。", app),
            }
        }
        Err(e) => {
            log::warn!("[tools] open_application {} failed: {}", app, e);
            ToolResult {
                status: ToolStatus::Failed,
                content: format!("没能打开 {}：{}", app, e),
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
    async fn test_open_application_rejects_not_whitelisted() {
        // Defense-in-depth: policy is the real gate, but the tool re-checks.
        let r = open_application(&serde_json::json!({"app": "definitely_not_an_app"})).await;
        assert_eq!(r.status, ToolStatus::Rejected);
        assert!(r.content.contains("不在允许"));
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
