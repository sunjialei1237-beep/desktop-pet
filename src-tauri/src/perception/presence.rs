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

/// An actionable presence transition — a state change the system should react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    /// User returned after being away; `away_secs` is how long they were idle.
    ReturnedBack { away_secs: u64 },
}

/// Pure: classify the transition between two consecutive presence samples.
///
/// The only actionable transition is `LongAway -> Active` — the user was away
/// for >5 minutes and just came back. `BriefAway -> Active` (back from the
/// bathroom) intentionally does NOT trigger, to avoid nagging (Principle 10:
/// life-feel, not a notification firehose).
///
/// `away_secs` is provided by the caller (which tracks when the away period
/// began) and carried through so the welcome generator can scale its tone.
pub fn classify_transition(
    prev: PresenceState,
    now: PresenceState,
    away_secs: u64,
) -> Option<Transition> {
    match (prev, now) {
        (PresenceState::LongAway, PresenceState::Active) => {
            Some(Transition::ReturnedBack { away_secs })
        }
        _ => None,
    }
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

    #[test]
    fn test_transition_long_away_to_active() {
        // LongAway -> Active fires, carrying the away duration through.
        let t = classify_transition(PresenceState::LongAway, PresenceState::Active, 600);
        assert_eq!(t, Some(Transition::ReturnedBack { away_secs: 600 }));
    }

    #[test]
    fn test_transition_brief_away_to_active_no_fire() {
        // BriefAway -> Active (back from the bathroom) must NOT fire.
        assert_eq!(
            classify_transition(PresenceState::BriefAway, PresenceState::Active, 120),
            None
        );
    }

    #[test]
    fn test_transition_still_active_no_fire() {
        assert_eq!(
            classify_transition(PresenceState::Active, PresenceState::Active, 0),
            None
        );
    }

    #[test]
    fn test_transition_long_away_to_brief_away_no_fire() {
        // Still away (mouse nudged but not really back) must not fire.
        assert_eq!(
            classify_transition(PresenceState::LongAway, PresenceState::BriefAway, 400),
            None
        );
    }

    #[test]
    fn test_transition_still_long_away_no_fire() {
        assert_eq!(
            classify_transition(PresenceState::LongAway, PresenceState::LongAway, 700),
            None
        );
    }
}
