//! Window-title parser: derives best-effort file/project hints from the
//! foreground window title + process name (plan P1, 2026-08-17).
//!
//! Granularity beyond "app + title" on Windows is heuristic: editors put
//! `{file} - {project} - {app}` in the title, JetBrains uses the reversed
//! order with an en-dash, browsers show `{page} - {browser}`. The process
//! name (already collected) disambiguates the layout — parsing on title
//! alone cannot. Hints are conservative: unknown layout → None, and every
//! downstream consumer must tolerate None (localized titles, multi-tab
//! editors, UWP fullscreen all break the heuristic).

/// Best-effort hints parsed from a window title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TitleHints {
    /// File being edited (editor) or page being viewed (browser).
    pub file: Option<String>,
    /// Project / folder name (editors only).
    pub project: Option<String>,
}

/// Title layouts keyed by process name (lowercase, `.exe` stripped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// `{file} - {project} - {app}` (VSCode / Cursor / Windsurf / Sublime).
    FileFirst,
    /// `{project} – {file} – {app}` (JetBrains IDEs, en-dash separator).
    ProjectFirst,
    /// `{page} - {browser}` — page title becomes the file hint.
    Browser,
    /// `{file} - {app}` with nothing else (Notepad).
    Single,
}

/// Separator variants seen across apps: hyphen (VSCode/browsers), en-dash
/// (JetBrains), em-dash (Firefox). Order irrelevant — parts are split by all.
const SEPARATORS: [&str; 3] = [" - ", " – ", " — "];

const VSCODE_FAMILY: &[&str] = &["code", "cursor", "windsurf", "sublime_text", "zcode"];
const VSCODE_TOKENS: &[&str] = &["Visual Studio Code", "Cursor", "Windsurf", "Sublime Text", "ZCode"];
const JETBRAINS_FAMILY: &[&str] = &[
    "idea", "pycharm", "webstorm", "clion", "rustrover", "goland",
    "datagrip", "rider", "phpstorm", "android studio",
];
const JETBRAINS_TOKENS: &[&str] = &[
    "IntelliJ IDEA", "PyCharm", "WebStorm", "CLion", "RustRover", "GoLand",
    "DataGrip", "Rider", "PhpStorm", "Android Studio",
];
const BROWSER_FAMILY: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc",
];
const BROWSER_TOKENS: &[&str] = &[
    "Google Chrome", "Microsoft Edge", "Mozilla Firefox", "Brave", "Opera",
    "Vivaldi", "Arc",
];
const SINGLE_FAMILY: &[&str] = &["notepad", "notepad++"];
const SINGLE_TOKENS: &[&str] = &["记事本", "Notepad++", "Notepad"];

/// Filenames that carry no information as a hint.
const EMPTY_FILE_NAMES: &[&str] = &["无标题", "Untitled", "New Tab", "新标签页"];

fn layout_for(process: &str) -> Option<Layout> {
    let name = process.trim().to_lowercase();
    let name = name.trim_end_matches(".exe");
    if VSCODE_FAMILY.contains(&name) {
        Some(Layout::FileFirst)
    } else if JETBRAINS_FAMILY.contains(&name) {
        Some(Layout::ProjectFirst)
    } else if BROWSER_FAMILY.contains(&name) {
        Some(Layout::Browser)
    } else if SINGLE_FAMILY.contains(&name) {
        Some(Layout::Single)
    } else {
        None
    }
}

/// Strip a trailing dirty-marker (VSCode "● " unsaved dot, "✎", "*").
fn strip_dirty_markers(title: &str) -> &str {
    let mut s = title.trim();
    loop {
        let stripped = s
            .strip_prefix("●")
            .or_else(|| s.strip_prefix("✎"))
            .or_else(|| s.strip_prefix("*"))
            .map(|rest| rest.trim_start());
        match stripped {
            Some(rest) => s = rest,
            None => return s,
        }
    }
}

/// Split a title into separator-delimited parts (all separator variants).
fn split_title(s: &str) -> Vec<String> {
    let mut parts = vec![s.to_string()];
    for sep in SEPARATORS {
        parts = parts
            .iter()
            .flat_map(|p| p.split(sep))
            .map(|p| p.trim().to_string())
            .collect();
    }
    parts.retain(|p| !p.is_empty());
    parts
}

/// Strip the known app token (and the separator before it) off the title end.
/// Returns None when no token matches — the caller then refuses to guess.
fn strip_app_suffix<'a>(title: &'a str, tokens: &[&str]) -> Option<&'a str> {
    for token in tokens {
        if let Some(rest) = title.strip_suffix(token) {
            // Swallow the separator directly before the app token. Strip it
            // BEFORE trimming — trailing spaces belong to the separator
            // (" - "), trimming first would break the suffix match.
            let rest = SEPARATORS
                .iter()
                .find_map(|sep| rest.strip_suffix(sep))
                .unwrap_or(rest);
            return Some(rest.trim_end());
        }
    }
    None
}

fn meaningful_file_name(name: &str) -> Option<String> {
    if name.is_empty() || EMPTY_FILE_NAMES.contains(&name) {
        None
    } else {
        Some(name.to_string())
    }
}

/// Parse file/project hints from a window title. `process_name` (without
/// `.exe`, case-insensitive) selects the layout; `None` or an unknown app
/// yields no hints — never guess on the title alone.
pub fn parse_title(title: &str, process_name: Option<&str>) -> TitleHints {
    let layout = match process_name.and_then(layout_for) {
        Some(l) => l,
        None => return TitleHints::default(),
    };
    let cleaned = strip_dirty_markers(title);
    if cleaned.is_empty() {
        return TitleHints::default();
    }

    match layout {
        Layout::FileFirst => match strip_app_suffix(cleaned, VSCODE_TOKENS) {
            Some(rest) => {
                let parts = split_title(rest);
                let file = parts.first().and_then(|p| meaningful_file_name(p));
                let project = parts.get(1).map(|p| p.to_string()).filter(|p| !p.is_empty());
                TitleHints { file, project }
            }
            None => TitleHints::default(),
        },
        Layout::ProjectFirst => match strip_app_suffix(cleaned, JETBRAINS_TOKENS) {
            Some(rest) => {
                let parts = split_title(rest);
                // JetBrains order: {project} – {file}. With only one part it is
                // the project (project tool window focused, no file open).
                let project = parts.first().map(|p| p.to_string()).filter(|p| !p.is_empty());
                let file = parts.get(1).and_then(|p| meaningful_file_name(p));
                TitleHints { file, project }
            }
            None => TitleHints::default(),
        },
        Layout::Browser => match strip_app_suffix(cleaned, BROWSER_TOKENS) {
            Some(rest) => TitleHints {
                file: meaningful_file_name(rest),
                project: None,
            },
            None => TitleHints::default(),
        },
        Layout::Single => match strip_app_suffix(cleaned, SINGLE_TOKENS) {
            Some(rest) => TitleHints {
                file: meaningful_file_name(rest),
                project: None,
            },
            None => TitleHints::default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vscode_family_file_first() {
        let h = parse_title("agent.rs - liri - Cursor", Some("Cursor.exe"));
        assert_eq!(h.file.as_deref(), Some("agent.rs"));
        assert_eq!(h.project.as_deref(), Some("liri"));
    }

    #[test]
    fn vscode_dirty_marker_stripped() {
        let h = parse_title("● agent.rs - liri - Visual Studio Code", Some("code"));
        assert_eq!(h.file.as_deref(), Some("agent.rs"));
        assert_eq!(h.project.as_deref(), Some("liri"));
    }

    #[test]
    fn jetbrains_project_first_en_dash() {
        let h = parse_title("桌宠 – lib.rs – RustRover", Some("rustrover.exe"));
        assert_eq!(h.project.as_deref(), Some("桌宠"));
        assert_eq!(h.file.as_deref(), Some("lib.rs"));
    }

    #[test]
    fn browser_page_title_keeps_inner_separators() {
        let h = parse_title(
            "How to do X - Stack Overflow - Google Chrome",
            Some("chrome.exe"),
        );
        assert_eq!(h.file.as_deref(), Some("How to do X - Stack Overflow"));
        assert_eq!(h.project, None);
    }

    #[test]
    fn firefox_em_dash() {
        let h = parse_title("某页面标题 — Mozilla Firefox", Some("firefox.exe"));
        assert_eq!(h.file.as_deref(), Some("某页面标题"));
    }

    #[test]
    fn unknown_app_no_hints() {
        let h = parse_title("agent.rs - liri - Whatever", Some("unknownapp.exe"));
        assert_eq!(h, TitleHints::default());
    }

    #[test]
    fn no_process_no_hints() {
        let h = parse_title("agent.rs - liri - Cursor", None);
        assert_eq!(h, TitleHints::default());
    }

    #[test]
    fn notepad_untitled_filtered() {
        let h = parse_title("无标题 - 记事本", Some("notepad.exe"));
        assert_eq!(h.file, None);
        let h = parse_title("笔记.txt - 记事本", Some("notepad.exe"));
        assert_eq!(h.file.as_deref(), Some("笔记.txt"));
    }

    #[test]
    fn vscode_single_part_file_only() {
        let h = parse_title("README.md - Visual Studio Code", Some("code.exe"));
        assert_eq!(h.file.as_deref(), Some("README.md"));
        assert_eq!(h.project, None);
    }

    #[test]
    fn title_without_app_token_no_hints() {
        // Process says Cursor but title lost the app token (custom title bar).
        let h = parse_title("agent.rs - liri", Some("cursor.exe"));
        assert_eq!(h, TitleHints::default());
    }
}
