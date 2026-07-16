//! Presence detection: determines if the user is at the computer.
//!
//! Uses Windows GetLastInputInfo to detect keyboard/mouse activity.
//! No keystroke content is recorded (privacy: Principle 14).

/// User presence state, derived from idle time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    /// Active in the last 30 seconds.
    Active,
    /// No activity for 30s - 5min (briefly away).
    BriefAway,
    /// No activity for > 5 minutes (long away).
    LongAway,
}

impl std::fmt::Display for PresenceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::BriefAway => write!(f, "brief_away"),
            Self::LongAway => write!(f, "long_away"),
        }
    }
}

/// Thresholds in seconds.
const BRIEF_AWAY_SECS: u64 = 30;
const LONG_AWAY_SECS: u64 = 300;

/// Classifies presence from idle seconds.
pub fn classify(idle_secs: u64) -> PresenceState {
    if idle_secs < BRIEF_AWAY_SECS {
        PresenceState::Active
    } else if idle_secs < LONG_AWAY_SECS {
        PresenceState::BriefAway
    } else {
        PresenceState::LongAway
    }
}

/// Gets the current idle time in seconds using Windows API.
/// Returns 0 on non-Windows platforms or if the API call fails.
#[cfg(target_os = "windows")]
pub fn idle_seconds() -> u64 {
    use windows::Win32::System::SystemInformation::GetTickCount64;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };

        if GetLastInputInfo(&mut info).as_bool() {
            let now = GetTickCount64();
            let last = info.dwTime as u64;
            // GetTickCount64 returns milliseconds since boot.
            // dwTime is also in milliseconds since boot.
            let idle_ms = now.saturating_sub(last);
            return idle_ms / 1000;
        }
    }
    0
}

#[cfg(not(target_os = "windows"))]
pub fn idle_seconds() -> u64 {
    0
}

/// Gets the current presence state.
pub fn current_presence() -> PresenceState {
    classify(idle_seconds())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_active() {
        assert_eq!(classify(10), PresenceState::Active);
        assert_eq!(classify(29), PresenceState::Active);
    }

    #[test]
    fn test_classify_brief_away() {
        assert_eq!(classify(30), PresenceState::BriefAway);
        assert_eq!(classify(120), PresenceState::BriefAway);
    }

    #[test]
    fn test_classify_long_away() {
        assert_eq!(classify(300), PresenceState::LongAway);
        assert_eq!(classify(3600), PresenceState::LongAway);
    }
}
