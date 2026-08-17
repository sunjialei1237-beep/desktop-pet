//! Window perception: detects the foreground application category.
//!
//! Privacy: Only the process name is extracted for category mapping. Window
//! titles ARE sampled locally for the environment hints (plan 2026-08-17),
//! but they reach the LLM only through the relevance-gated `[Environment]`
//! section after control-char stripping + length caps, with a fixed
//! untrusted-data declaration appended (environment.rs §8.2-C2). They are
//! never stored in the DB — perception module invariant.

/// Application category for context-aware behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AppCategory {
    Work,
    Entertainment,
    Social,
    Browsing,
    #[default]
    Other,
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

    let work_apps = [
        "code", "devenv", "idea", "pycharm", "webstorm", "clion",
        "rustrover", "goland", "eclipse", "netbeans", "vim", "neovim",
        "sublime_text", "atom", "notepad++", "word", "excel", "powerpnt",
        "outlook", "onenote", "teams", "slack", "zoom", "terminal",
        "windowsterminal", "powershell", "cmd", "gitkraken", "postman",
        "dbeaver", "ssms", "rstudio", "windsurf", "cursor",
        "zcode", "opencode",
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

    if work_apps.contains(&name) {
        AppCategory::Work
    } else if entertainment_apps.contains(&name) {
        AppCategory::Entertainment
    } else if social_apps.contains(&name) {
        AppCategory::Social
    } else if browsing_apps.contains(&name) {
        AppCategory::Browsing
    } else {
        AppCategory::Other
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
) -> &'static std::sync::Mutex<Option<(String, Option<String>)>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<(String, Option<String>)>>> =
        std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// Pure decision: when the sampled pid is our own process, the effective
/// foreground sample is the last non-pet sample (fall through to None on a
/// fresh start). Any other pid uses the live sample.
#[doc(hidden)]
pub fn resolve_own_window_sample(
    pid: u32,
    own_pid: u32,
    live: Option<&(String, Option<String>)>,
    last: Option<&(String, Option<String>)>,
) -> Option<(String, Option<String>)> {
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
#[cfg(target_os = "windows")]
pub fn foreground_info() -> (Option<String>, Option<String>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return (None, None);
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return (None, None);
        }

        // Title: 512 wchars is the conventional window-title budget.
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        let title = if len > 0 {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            None
        };

        let live = cached_process_name(pid).map(|name| (name, title));
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
            .map(|(name, title)| (Some(name), title))
            .unwrap_or((None, None))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn foreground_info() -> (Option<String>, Option<String>) {
    (None, None)
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
    fn test_classify_other() {
        assert_eq!(classify_process("explorer.exe"), AppCategory::Other);
        assert_eq!(classify_process("unknown_app.exe"), AppCategory::Other);
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
        let liri = ("desktop-pet.exe".to_string(), Some("璃".to_string()));
        let code = ("Code.exe".to_string(), Some("agent.rs — Liri".to_string()));
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
