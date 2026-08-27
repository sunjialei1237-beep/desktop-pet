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
    dedup_first_seen(apps)
}

/// 首见优先去重（大小写不敏感）。原 `dedup_by` 只合并**相邻**重复——同名
/// lnk 分布在不同扫描根（如桌面 vs C:\ProgramData）时两个都会留下；虽然
/// `fuzzy_match_app` 取首个匹配把它掩盖了，但列表语义应当唯一。顺序保持
/// 不变（先 Desktop 后 Start Menu），首见者胜。
fn dedup_first_seen(apps: Vec<AppEntry>) -> Vec<AppEntry> {
    let mut seen = std::collections::HashSet::with_capacity(apps.len());
    apps.into_iter()
        .filter(|a| seen.insert(a.name.to_ascii_lowercase()))
        .collect()
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

/// Windows housekeeping processes that may show up spontaneously right after
/// an explorer launch — never counted as "the app the user asked for". All
/// entries lowercase (compared via to_lowercase).
const LAUNCH_NOISE_PROCESSES: &[&str] = &[
    "explorer.exe", "conhost.exe", "dllhost.exe", "sihost.exe", "runtimebroker.exe",
    "cmd.exe", "cscript.exe", "wscript.exe", "searchprotocolhost.exe",
    "searchfilterhost.exe", "applicationframehost.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "textinputhost.exe", "smartscreen.exe",
    "backgroundtaskhost.exe", "taskhostw.exe", "svchost.exe",
];

/// Snapshot every running process exe name (ToolHelp walk, same API as
/// perception::window). Empty on non-Windows so the diff degrades to "no new
/// process detected" instead of breaking.
#[cfg(target_os = "windows")]
fn snapshot_process_names() -> std::collections::HashSet<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut names = std::collections::HashSet::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return names;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile);
                names.insert(name.trim_end_matches('\0').to_lowercase());
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    names
}

#[cfg(not(target_os = "windows"))]
fn snapshot_process_names() -> std::collections::HashSet<String> {
    std::collections::HashSet::new()
}

/// First process in `now` that is new (not in `before`) and not Windows noise.
fn first_new_process(
    before: &std::collections::HashSet<String>,
    now: &std::collections::HashSet<String>,
) -> Option<String> {
    now.iter()
        .find(|n| !before.contains(*n) && !LAUNCH_NOISE_PROCESSES.contains(&n.as_str()))
        .cloned()
}

/// `open_application`: discover and launch an app by spoken name. Scans the
/// user's Desktop + Start Menu shortcuts (`.lnk`) and fuzzy-matches the
/// requested name, then hands the resolved shortcut to `explorer` (which opens
/// the real target through the shell). No static whitelist — the pet finds what
/// the user actually has installed.
///
/// 2026-08-26 live miss (「帮我打开抖音」→「开了」→ 桌面没窗口，重试才出现)：
/// explorer 的 spawn 成功只代表"shell 收到了命令"，不代表程序起来了——
/// 抖音这类启动器+守护架构的应用窗口要几十秒才出现。所以 spawn 后做差分
/// 进程校验（≤2.5s 轮询新进程），按实际检测结果汇报，模型不再凭空打包票。
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

    // Differential baseline BEFORE the spawn: a verified launch = a process
    // that did not exist before the command.
    let before = snapshot_process_names();

    // Open the .lnk through explorer (the shell resolves the real target).
    // No CREATE_NO_WINDOW needed: explorer is a GUI app, no console spawned.
    if let Err(e) = std::process::Command::new("explorer").arg(&target.path).spawn() {
        log::warn!("[tools] open_application {} failed: {}", target.path, e);
        return ToolResult {
            status: ToolStatus::Failed,
            content: format!("没能打开 {}：{}", target.name, e),
        };
    }
    log::info!(
        "[tools] open_application: launched {} via {}",
        target.name,
        target.path
    );

    // Poll ≤2.5s for a new non-noise process. A launcher exe shows up within a
    // second even when the real window takes much longer (抖音 cold start).
    let mut detected: Option<String> = None;
    for _ in 0..5 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let now = snapshot_process_names();
        if let Some(name) = first_new_process(&before, &now) {
            detected = Some(name);
            break;
        }
    }
    match detected {
        Some(proc) => {
            log::info!(
                "[tools] open_application: verified new process {} for {}",
                proc,
                target.name
            );
            ToolResult {
                status: ToolStatus::Success,
                content: format!(
                    "已启动 {}（检测到新进程 {}）。如果几秒后窗口还没出现，程序可能还在慢启动，让用户等一下；还没有就告诉我，我再开一次。",
                    target.name, proc
                ),
            }
        }
        None => {
            log::info!(
                "[tools] open_application: no new process within 2.5s for {}",
                target.name
            );
            ToolResult {
                status: ToolStatus::Success,
                content: format!(
                    "已向系统发出启动 {} 的指令，但 2 秒内没检测到新进程——它可能已在后台运行，或正在慢启动。请如实告诉用户：如果几秒后窗口没出现，就再说一声，我重新开一次。",
                    target.name
                ),
            }
        }
    }
}

/// `open_file` (plan §3.5 / §8.3-E3): launch an EXISTING local file through the
/// shell's default association. The dangerous part — which extensions may reach
/// the association — is enforced in policy; this only re-checks existence and
/// hands the canonical path to explorer (explorer returns 1 by design, spawn
/// success is the only useful signal).
pub fn open_file(args: &serde_json::Value) -> ToolResult {
    let raw = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
    if raw.trim().is_empty() {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "没有指定要打开的文件。".to_string(),
        };
    }
    let canonical = match dunce::canonicalize(raw) {
        Ok(c) if c.is_file() => c,
        _ => {
            return ToolResult {
                status: ToolStatus::Rejected,
                content: "这个文件不存在，我没法打开。".to_string(),
            }
        }
    };
    match std::process::Command::new("explorer").arg(&canonical).spawn() {
        Ok(_) => {
            let name = canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            log::info!("[tools] open_file: launched {}", canonical.display());
            ToolResult {
                status: ToolStatus::Success,
                content: format!("已经帮你打开 {} 了。", name),
            }
        }
        Err(e) => {
            log::warn!("[tools] open_file {} failed: {}", canonical.display(), e);
            ToolResult {
                status: ToolStatus::Failed,
                content: format!("没能打开这个文件：{}", e),
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

    #[test]
    fn test_noise_list_is_lowercase_for_diff_comparison() {
        // first_new_process compares lowercase snapshots — a mixed-case entry
        // here would silently never match. Lock the invariant.
        assert!(LAUNCH_NOISE_PROCESSES
            .iter()
            .all(|n| *n == n.to_lowercase()));
        assert!(LAUNCH_NOISE_PROCESSES.contains(&"explorer.exe"));
    }

    #[test]
    fn test_first_new_process_ignores_noise_and_known() {
        let mut before = std::collections::HashSet::new();
        before.insert("douyin.exe".to_string());
        before.insert("desktop-pet.exe".to_string());

        let mut now = before.clone();
        now.insert("explorer.exe".to_string()); // noise: explorer spawn echo
        now.insert("conhost.exe".to_string()); // noise
        assert_eq!(first_new_process(&before, &now), None);

        now.insert("zcode.exe".to_string()); // the real launch
        assert_eq!(first_new_process(&before, &now), Some("zcode.exe".to_string()));
    }

    #[test]
    fn test_dedup_first_seen_across_scan_roots() {
        // 同名 lnk 分布在不同根（桌面 vs ProgramData）不相邻——原 dedup_by
        // 只去相邻重复，两个都会留下。锁死「首见优先、顺序不变」语义。
        let apps = vec![
            AppEntry { name: "微信".to_string(), path: "desktop.lnk".to_string() },
            AppEntry { name: "Chrome".to_string(), path: "a.lnk".to_string() },
            AppEntry { name: "微信".to_string(), path: "programdata.lnk".to_string() },
            AppEntry { name: "WeChat".to_string(), path: "c.lnk".to_string() },
        ];
        let out = dedup_first_seen(apps);
        let names: Vec<&str> = out.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["微信", "Chrome", "WeChat"]);
        assert_eq!(out[0].path, "desktop.lnk", "first-seen entry must win");
    }

    #[test]
    fn test_dedup_first_seen_keeps_unique_apps() {
        let apps = vec![
            AppEntry { name: "VS Code".to_string(), path: "a.lnk".to_string() },
            AppEntry { name: "vs code".to_string(), path: "b.lnk".to_string() },
            AppEntry { name: "Notion".to_string(), path: "c.lnk".to_string() },
        ];
        let out = dedup_first_seen(apps);
        assert_eq!(out.len(), 2, "case-insensitive duplicate must collapse");
        assert_eq!(out[0].path, "a.lnk");
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
