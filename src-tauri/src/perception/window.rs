//! Window perception: detects the foreground application category.
//!
//! Privacy: Only the process name is extracted and mapped to a category.
//! Window titles are never stored in the DB or sent to the LLM.
//! The LLM only sees high-level descriptions like "working" or "gaming".

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

/// Gets the current foreground window's process name.
/// Returns None on non-Windows or if the API call fails.
#[cfg(target_os = "windows")]
pub fn foreground_process() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
        PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let name = String::from_utf16_lossy(&entry.szExeFile);
                    let name = name.trim_end_matches('\0').to_string();
                    let _ = CloseHandle(snapshot);
                    return Some(name);
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
}
