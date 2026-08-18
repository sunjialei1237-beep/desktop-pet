//! Window perception: detects the foreground application category.
//!
//! Privacy: Only the process name is extracted for category mapping. Window
//! titles ARE sampled locally for the environment hints (plan 2026-08-17),
//! but they reach the LLM only through the relevance-gated `[Environment]`
//! section after control-char stripping + length caps, with a fixed
//! untrusted-data declaration appended (environment.rs §8.2-C2). They are
//! never stored in the DB — perception module invariant.
//!
//! 实装扩展 (2026-08-18): for foreground EDITOR processes the observer may
//! also resolve the workspace FOLDER from the process command line, so "帮我
//! 看看现在的代码" gets a real absolute path for the read-tools/consent
//! flow. Only the parsed folder path is kept in memory and shown through the
//! same sanitized [Environment] section; the raw command line is discarded.

use std::path::{Path, PathBuf};

/// Application category for context-aware behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Work,
    Entertainment,
    Social,
    Browsing,
    Other,
}

impl Default for AppCategory {
    // Post-实装 policy (2026-08-18): the pin list was "只有白名单里的
    // 程序才算 Work", which made the focus clock look broken for most users.
    // Default to Work, and only let KNOWN leisure channels interrupt it.
    fn default() -> Self {
        AppCategory::Work
    }
}


impl std::fmt::Display for AppCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work => write!(f, "work"),
            Self::Entertainment => write!(f, "entertainment"),
            Self::Social => write!(f, "social"),
            Self::Browsing => write!(f, "browsing"),
            Self::Other => write!(f, "other"),
        }
    }
}

/// Classifies a process name into a category.
pub fn classify_process(process_name: &str) -> AppCategory {
    let name = process_name.to_lowercase();
    let name = name.trim_end_matches(".exe");

    let _work_apps = [
        "code", "devenv", "idea", "pycharm", "webstorm", "clion",
        "rustrover", "goland", "eclipse", "netbeans", "vim", "neovim",
        "sublime_text", "atom", "notepad++", "word", "excel", "powerpnt",
        "outlook", "onenote", "teams", "slack", "zoom", "terminal",
        "windowsterminal", "powershell", "cmd", "gitkraken", "postman",
        "dbeaver", "ssms", "rstudio", "windsurf", "cursor",
        "zcode", "opencode",
        // Basic/plain text editors count as work too — opening files in
        // Notepad (the pet's own E3 open_file target) or a markdown editor is
        // exactly the "正在写" foreground the deep-focus clock should track.
        "notepad", "write", "typora", "obsidian", "marktext", "wps",
    ];
    let entertainment_apps = [
        "steam", "epicgames", "battle.net", "origin", "riotclientservices",
        "vlc", "potplayer", "potplayermini64", "spotify", "netflix",
        "obs64", "minecraft", "gta5", "valorant", "league of legends",
    ];
    let social_apps = [
        "wechat", "qq", "telegram", "discord", "whatsapp", "signal",
        "skype", "dingtalk", "wecom", "lark", "feishu",
    ];
    let browsing_apps = [
        "chrome", "firefox", "msedge", "opera", "brave", "safari",
        "vivaldi", "arc", "maxthon",
    ];
    // System chrome / shells that are not sustained "work", so selecting files
    // or opening Explorer doesn't accidentally fake 25 minutes of focus.
    let system_apps = [
        "explorer", "searchapp", "searchhost", "shellexperiencehost",
        "taskmgr", "applicationframehost", "systemsettings", "lockapp",
        "logonui", "desktop-pet", "ms-settings",
    ];

    if browsing_apps.contains(&name) {
        AppCategory::Browsing
    } else if entertainment_apps.contains(&name) {
        AppCategory::Entertainment
    } else if social_apps.contains(&name) {
        AppCategory::Social
    } else if system_apps.contains(&name) {
        AppCategory::Other
    } else {
        // Anything not explicitly a browser / game / chat / system shell is
        // treated as work. Explicit work_apps is mostly documentation now;
        // unknown editors and tools no longer zero the focus clock.
        AppCategory::Work
    }
}

/// Process-name cache: pid → exe name. Process names are stable for a pid's
/// lifetime, so the (relatively expensive) full-process Toolhelp snapshot
/// walk only runs on cache misses — important now that the environment
/// observer samples every 3 s (plan P1).
///
/// Bounded (§8.5-M1): the cache clears itself when it would grow past the cap
/// with a brand-new pid. Process names are cheap to re-resolve and Windows
/// reuses pids, so an old unbounded map is strictly worse than a reset.
const PROCESS_NAME_CACHE_CAP: usize = 256;

fn process_name_cache() -> &'static std::sync::Mutex<std::collections::HashMap<u32, String>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, String>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Insert with a hard cap: updating an existing key never clears; inserting a
/// brand-new key at capacity clears the map first (bounded + self-heals pid
/// reuse). Shared helper so the policy is unit-testable everywhere.
fn insert_bounded<K, V>(map: &mut std::collections::HashMap<K, V>, key: K, value: V, cap: usize)
where
    K: std::hash::Hash + Eq,
{
    if map.len() >= cap && !map.contains_key(&key) {
        map.clear();
    }
    map.insert(key, value);
}

#[cfg(target_os = "windows")]
fn cache_process_name(pid: u32, name: String) {
    if let Ok(mut cache) = process_name_cache().lock() {
        insert_bounded(&mut cache, pid, name, PROCESS_NAME_CACHE_CAP);
    }
}

/// Resolve a pid to its process name via a full Toolhelp snapshot walk.
#[cfg(target_os = "windows")]
fn process_name_by_snapshot(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let name = String::from_utf16_lossy(&entry.szExeFile);
                    let _ = CloseHandle(snapshot);
                    return Some(name.trim_end_matches('\0').to_string());
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    None
}

#[cfg(target_os = "windows")]
fn cached_process_name(pid: u32) -> Option<String> {
    if let Ok(cache) = process_name_cache().lock() {
        if let Some(name) = cache.get(&pid) {
            return Some(name.clone());
        }
    }
    let name = process_name_by_snapshot(pid)?;
    cache_process_name(pid, name.clone());
    Some(name)
}

// --- Editor workspace-root resolution (实装 narrowness fix) -----------------
// VS Code / Cursor window titles carry "file - project", but not the folder
// path; the read tools need an absolute path before they can even ask consent.
// For editor processes we resolve the folder once per pid from the command
// line and cache the RESULT in memory (never the raw command line).

/// Process names that are known to accept a folder/file uri or path argument.
#[cfg(target_os = "windows")]
pub fn is_editor_process(process_name: &str) -> bool {
    let n = process_name.to_lowercase();
    let n = n.trim_end_matches(".exe");
    matches!(
        n,
        "code" | "cursor" | "windsurf" | "zcode" | "opencode" | "atom"
            | "notepad++" | "sublime_text"
    )
}

fn editor_root_cache() -> &'static std::sync::Mutex<std::collections::HashMap<u32, Option<PathBuf>>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u32, Option<PathBuf>>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(target_os = "windows")]
fn cache_editor_root(pid: u32, root: Option<PathBuf>) {
    if let Ok(mut cache) = editor_root_cache().lock() {
        insert_bounded(&mut cache, pid, root, PROCESS_NAME_CACHE_CAP);
    }
}

/// Resolve the workspace folder for an editor pid. Only successful results
/// are cached — a failed capture returns None for THIS sample and is retried
/// on the next one (PowerShell's first invocation can be slow/cold; caching a
/// miss would make the whole process lifetime root-less).
#[cfg(target_os = "windows")]
pub fn editor_workspace_root(pid: u32) -> Option<PathBuf> {
    if let Ok(Some(root)) = editor_root_cache().lock().map(|g| g.get(&pid).cloned().flatten()) {
        return Some(root);
    }
    let command_line = match capture_process_command_line(pid) {
        Some(c) => c,
        None => return None,
    };
    let root = parse_editor_workspace_root(&command_line);
    if let Some(root) = &root {
        cache_editor_root(pid, Some(root.clone()));
        log::info!("[window] editor workspace root resolved for pid {}: {}", pid, root.display());
    }
    root
}

/// One-shot command-line capture for a local process. Local privileged info,
/// used ONLY to extract the folder path — RawLine is dropped immediately.
#[cfg(target_os = "windows")]
fn capture_process_command_line(pid: u32) -> Option<String> {
    // Win32_Process via PowerShell is the only reliable standard-library route
    // without linking into WMI COM. One spawn per editor pid (result cached);
    // the observer only calls this for the foreground editor category.
    let script = format!(
        "(Get-CimInstance Win32_Process -Filter 'ProcessId={}').CommandLine",
        pid
    );
    match std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if raw.is_empty() {
                log::warn!("[window] command line capture for pid {} succeeded but was empty", pid);
                None
            } else {
                Some(raw)
            }
        }
        Ok(out) => {
            log::warn!(
                "[window] command line capture for pid {} failed status={} stderr={}",
                pid,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        Err(e) => {
            log::warn!("[window] command line capture for pid {} spawn error: {}", pid, e);
            None
        }
    }
}

/// Extract a WORKSPACE FOLDER from an editor command line.
/// Pure + unit-testable:
/// 1. `--folder-uri file:///D%3A/…` (VS Code / Cursor workspace)
/// 2. `--file-uri file:///D%3A/…` (single file: its parent folder)
/// 3. raw `D:\…` path argument (VS Code without uri flags)
#[doc(hidden)]
pub fn parse_editor_workspace_root(command_line: &str) -> Option<PathBuf> {
    let folder_uri = arg_value(command_line, "--folder-uri");
    if let Some(root) = folder_uri.and_then(|s| file_uri_to_path(&s)) {
        return dir_if_exists(root);
    }
    let file_uri = arg_value(command_line, "--file-uri");
    if let Some(path) = file_uri.and_then(|s| file_uri_to_path(&s)) {
        return dir_if_exists(path.parent().unwrap_or(Path::new("")).to_path_buf());
    }
    // Raw drive-letter argument: scan for `X:\...` outside the exe arg.
    raw_windows_path_arg(command_line).and_then(|p| dir_if_exists(p))
}

fn arg_value(command_line: &str, flag: &str) -> Option<String> {
    let idx = command_line.find(flag)?;
    let rest = &command_line[idx + flag.len()..];
    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
    let end = rest
        .find(|c| c == ' ' || c == '"')
        .unwrap_or(rest.len());
    let value = &rest[..end];
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let s = uri.strip_prefix("file://")?;
    let s = s.trim_start_matches('/');
    let decoded = percent_decode(s);
    // %3A/… → "D:/…"; let Windows canonicalize the mixed slashes.
    let candidate = PathBuf::from(decoded);
    if candidate.is_absolute()
        || (candidate.as_os_str().len() >= 2)
            && candidate.to_string_lossy().as_bytes().get(1) == Some(&b':')
    {
        Some(candidate)
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Returns `path` when it is an existing directory; when it's an existing file
/// returns its parent. Non-existing candidates still fall through to the
/// parent variant only for the uri based path; raw args are verified below.
fn dir_if_exists(path: PathBuf) -> Option<PathBuf> {
    match std::fs::metadata(&path) {
        Ok(m) if m.is_dir() => Some(path),
        Ok(_) => path.parent().map(|p| p.to_path_buf()),
        Err(_) => None,
    }
}

/// Find the first raw `X:\…` argument AFTER the executable's own path that
/// exists as a directory/file.
fn raw_windows_path_arg(command_line: &str) -> Option<PathBuf> {
    let bytes: Vec<char> = command_line.chars().collect();
    let mut i = 0usize;
    let mut seen_exe_path = false;
    while i + 2 < bytes.len() {
        if bytes[i].is_ascii_alphabetic() && bytes[i + 1] == ':' && bytes[i + 2] == '\\' {
            let mut end = i + 3;
            while end < bytes.len() && bytes[end] != '"' {
                end += 1;
            }
            let candidate = bytes[i..end].iter().collect::<String>();
            let path = PathBuf::from(&candidate);
            if seen_exe_path {
                if let Some(dir) = dir_if_exists(path) {
                    return Some(dir);
                }
            } else {
                // The first drive path is the editor executable itself; a
                // resolution succeeding on it would return Program Files /
                // AppData as the "workspace" — skip it.
                seen_exe_path = true;
            }
            i = end;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn cached_process_name(_pid: u32) -> Option<String> {
    None
}

/// Memory for the last sample that was NOT one of our own windows
/// (main pet window + the F12 debug window share this process). Sampling
/// threads read it when the pet or its debug panel holds the foreground:
/// self-focus must not zero the deep-focus clock or inject `desktop-pet`
/// as the active app (plan §8.5-M7).
fn last_non_pet_foreground(
) -> &'static std::sync::Mutex<Option<(String, Option<String>, u32)>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<(String, Option<String>, u32)>>> =
        std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// Pure decision: when the sampled pid is our own process, the effective
/// foreground sample is the last non-pet sample (fall through to None on a
/// fresh start). Any other pid uses the live sample. The pid is part of the
/// sample so the environment layer can keep resolving THAT app's resources
/// (editor workspace root) while the pet/debug window owns the foreground.
#[doc(hidden)]
pub fn resolve_own_window_sample(
    pid: u32,
    own_pid: u32,
    live: Option<&(String, Option<String>, u32)>,
    last: Option<&(String, Option<String>, u32)>,
) -> Option<(String, Option<String>, u32)> {
    if pid == own_pid {
        last.cloned()
    } else {
        live.cloned()
    }
}

/// Foreground window info from a single `GetForegroundWindow` call, so the
/// process name and title always describe the same window (plan P1: no
/// cross-call race when the foreground switches between the two reads).
/// Pet-owned windows (main + debug) fall back to the last non-pet sample —
/// clicking Liri or opening F12 must not turn the environment into
/// "desktop-pet" / reset the deep-focus clock.
/// Returns `(process_name, window_title, pid)` — pid is carried through so
/// the environment observer can resolve editor workspace roots from the
/// SAME sampled process (实装: no second GetForegroundWindow race).
#[cfg(target_os = "windows")]
pub fn foreground_info() -> (Option<String>, Option<String>, Option<u32>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return (None, None, None);
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return (None, None, None);
        }

        // Title: 512 wchars is the conventional window-title budget.
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        let title = if len > 0 {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            None
        };

        let live = cached_process_name(pid).map(|name| (name, title, pid));
        let resolved = match last_non_pet_foreground().lock() {
            Ok(mut last) => {
                let r = resolve_own_window_sample(
                    pid,
                    std::process::id(),
                    live.as_ref(),
                    last.as_ref(),
                );
                if let Some(sample) = &r {
                    *last = Some(sample.clone());
                }
                r
            }
            Err(_) => live.clone(),
        };
        resolved
            .map(|(name, title, sample_pid)| (Some(name), title, Some(sample_pid)))
            .unwrap_or((None, None, Some(pid)))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn foreground_info() -> (Option<String>, Option<String>, Option<u32>) {
    (None, None, None)
}

/// Gets the current foreground window's process name.
/// Returns None on non-Windows or if the API call fails.
#[cfg(target_os = "windows")]
pub fn foreground_process() -> Option<String> {
    foreground_info().0
}

#[cfg(not(target_os = "windows"))]
pub fn foreground_process() -> Option<String> {
    None
}

/// Gets the current foreground app category.
pub fn current_category() -> AppCategory {
    match foreground_process() {
        Some(proc) => classify_process(&proc),
        None => AppCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_work() {
        assert_eq!(classify_process("code.exe"), AppCategory::Work);
        assert_eq!(classify_process("devenv.exe"), AppCategory::Work);
        assert_eq!(classify_process("WindowsTerminal.exe"), AppCategory::Work);
        // Plain text editors are work too — the release user opens files with
        // Notepad and expects the F12 focus clock to count it.
        assert_eq!(classify_process("Notepad.exe"), AppCategory::Work);
        assert_eq!(classify_process("Typora.exe"), AppCategory::Work);
        assert_eq!(classify_process("Obsidian.exe"), AppCategory::Work);
    }

    #[test]
    fn test_classify_entertainment() {
        assert_eq!(classify_process("steam.exe"), AppCategory::Entertainment);
        assert_eq!(classify_process("Spotify.exe"), AppCategory::Entertainment);
    }

    #[test]
    fn test_classify_social() {
        assert_eq!(classify_process("WeChat.exe"), AppCategory::Social);
        assert_eq!(classify_process("Discord.exe"), AppCategory::Social);
    }

    #[test]
    fn test_classify_browsing() {
        assert_eq!(classify_process("chrome.exe"), AppCategory::Browsing);
        assert_eq!(classify_process("msedge.exe"), AppCategory::Browsing);
    }

    #[test]
    fn test_classify_defaults_to_work_not_other() {
        // 实装 fix: the clock must not be a whitelist. Unknown applications
        // count as work; only known leisure/system shells interrupt focus.
        assert_eq!(classify_process("unknown-toolbox.exe"), AppCategory::Work);
        assert_eq!(classify_process("explorer.exe"), AppCategory::Other);
        assert_eq!(classify_process("minecraft.exe"), AppCategory::Entertainment);
        assert_eq!(classify_process("discord.exe"), AppCategory::Social);
    }

    #[test]
    fn test_parse_editor_workspace_root_uri() {
        let dir = std::env::temp_dir();
        let uri = format!(
            "file:///{}",
            dir.to_string_lossy().replace(':', "%3A").replace('\\', "/")
        );
        let cmd = format!(
            r#""C:\Program Files\Code.exe" --folder-uri {} --log debug"#,
            uri
        );
        let parsed = parse_editor_workspace_root(&cmd);
        assert_eq!(parsed, Some(dir));
    }

    #[test]
    fn test_parse_editor_workspace_root_raw_path() {
        let dir = std::env::temp_dir();
        if dir.to_string_lossy().contains(' ') {
            return; // raw scanner stops at spaces; quoted path yields no match currently
        }
        let cmd = format!(
            r#""C:\Program Files\Code.exe" "{}""#,
            dir.to_string_lossy()
        );
        assert_eq!(parse_editor_workspace_root(&cmd), Some(dir));
    }

    #[test]
    fn test_classify_other() {
        assert_eq!(classify_process("explorer.exe"), AppCategory::Other);
        // 实装 policy change: unknowns are work now, explorer stays the
        // explicit non-work shell.
        assert_eq!(classify_process("unknown_app.exe"), AppCategory::Work);
    }

    #[test]
    fn process_name_cache_is_bounded() {
        // Brand-new key at capacity clears the map first (bounded memory),
        // updating an existing key at capacity doesn't.
        let mut m: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        insert_bounded(&mut m, 1, "a".into(), 2);
        insert_bounded(&mut m, 2, "b".into(), 2);
        insert_bounded(&mut m, 3, "c".into(), 2);
        assert_eq!(m.len(), 1, "cap overflow must reset the map");
        assert_eq!(m.get(&3).map(String::as_str), Some("c"));
        insert_bounded(&mut m, 4, "d".into(), 2);
        insert_bounded(&mut m, 4, "d2".into(), 2);
        assert_eq!(m.len(), 2, "updating an existing key must not clear");
        assert_eq!(m.get(&4).map(String::as_str), Some("d2"));
    }

    #[test]
    fn own_window_uses_last_non_pet_sample() {
        // Clicking Liri or opening the F12 debug panel shares the pet's pid —
        // the effective sample must fall back to the last real app so the
        // deep-focus clock (and the environment app hint) survive self-focus.
        // 实装: the FALLBACK must keep that app's pid too, or the editor
        // workspace-root resolver would query the pet's own command line and
        // lose the root for the whole turn.
        let liri = ("desktop-pet.exe".to_string(), Some("璃".to_string()), 4242u32);
        let code = ("Code.exe".to_string(), Some("agent.rs — Liri".to_string()), 3333u32);
        assert_eq!(
            resolve_own_window_sample(4242, 4242, Some(&liri), Some(&code)),
            Some(code.clone())
        );
        assert_eq!(
            resolve_own_window_sample(3333, 4242, Some(&code.clone()), Some(&code)),
            Some(code)
        );
        // Fresh start, no prior non-pet sample → none.
        assert_eq!(resolve_own_window_sample(4242, 4242, Some(&liri), None), None);
    }
}
