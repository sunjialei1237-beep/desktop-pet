//! Environment layer: snapshot-diff events + in-memory activity history
//! (plan 2026-08-17 §2.1, P0/P1).
//!
//! Architecture: NO push-style event bus. A low-frequency poll thread
//! (2–5 s) samples the foreground app/title/presence, and semantic changes
//! are synthesized as `EnvironmentEvent`s by diffing consecutive samples —
//! the same polling-over-hooks philosophy as `cursor.rs` (hooks need a
//! message pump and are killed by Windows on timeout).
//!
//! Privacy: the activity ring buffer lives ONLY in process memory. Window
//! titles / hints are never written to the DB (perception module invariant)
//! and reach the LLM only through the relevance-gated [Environment] section
//! (plan §2.4). Cold start = empty history; consumers must tolerate that.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::perception::presence::{self, PresenceState};
use crate::perception::{title, window};

/// Ring-buffer capacity (plan §2.1: last ~10 semantic changes).
const RING_CAP: usize = 10;
/// Observer sample interval (plan: 2–5 s; 3 s chosen).
const POLL_INTERVAL_SECS: u64 = 3;
/// How many file transitions the `Recently:` summary keeps.
const SUMMARY_MAX_ITEMS: usize = 5;

/// A semantic change between two consecutive environment samples.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EnvironmentEvent {
    AppChanged { app: String },
    FileHintChanged {
        from: Option<String>,
        to: Option<String>,
    },
    ProjectHintChanged { project: String },
    /// LongAway → Active; the actionable welcome-back path already lives in
    /// the life loop, this only records the fact for the activity summary.
    PresenceReturned { away_secs: u64 },
}

/// One environment sample (the diff-able subset of the perception snapshot).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSample {
    pub app: Option<String>,
    pub title: Option<String>,
    pub file_hint: Option<String>,
    pub project_hint: Option<String>,
    pub presence: PresenceState,
}

/// Pure diff: which semantic events separate `prev` from `cur`?
/// `away_secs` is supplied by the observer (it owns the away-period clock).
pub fn diff(prev: &EnvSample, cur: &EnvSample, away_secs: u64) -> Vec<EnvironmentEvent> {
    let mut events = Vec::new();
    if prev.app != cur.app {
        if let Some(app) = &cur.app {
            events.push(EnvironmentEvent::AppChanged { app: app.clone() });
        }
    }
    if prev.file_hint != cur.file_hint {
        events.push(EnvironmentEvent::FileHintChanged {
            from: prev.file_hint.clone(),
            to: cur.file_hint.clone(),
        });
    }
    if prev.project_hint != cur.project_hint {
        if let Some(project) = &cur.project_hint {
            events.push(EnvironmentEvent::ProjectHintChanged {
                project: project.clone(),
            });
        }
    }
    if let Some(presence::Transition::ReturnedBack { .. }) =
        presence::classify_transition(prev.presence, cur.presence, away_secs)
    {
        events.push(EnvironmentEvent::PresenceReturned { away_secs });
    }
    events
}

// --- Ring buffer (process memory only) ----------------------------------------

fn ring() -> &'static Mutex<VecDeque<EnvironmentEvent>> {
    static RING: OnceLock<Mutex<VecDeque<EnvironmentEvent>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn push_event(event: EnvironmentEvent) {
    if let Ok(mut q) = ring().lock() {
        q.push_back(event);
        while q.len() > RING_CAP {
            q.pop_front();
        }
    }
}

/// Clone of the current activity history (oldest first). Debug Panel use.
pub fn recent_events() -> Vec<EnvironmentEvent> {
    ring().lock().map(|q| q.iter().cloned().collect()).unwrap_or_default()
}

/// `Recently: grounding.rs → planner.rs → agent.rs` — last few file/page
/// transitions, consecutive duplicates collapsed. None when no history.
/// Only injected when the relevance gate fires (plan §2.3/§2.4).
pub fn recent_summary() -> Option<String> {
    let events = recent_events();
    let mut names: Vec<String> = Vec::new();
    for ev in events.iter().rev() {
        if let EnvironmentEvent::FileHintChanged {
            to: Some(name), ..
        } = ev
        {
            if names.last().map(|n| n != name).unwrap_or(true) {
                names.push(name.clone());
            }
        }
        if names.len() >= SUMMARY_MAX_ITEMS {
            break;
        }
    }
    if names.is_empty() {
        return None;
    }
    names.reverse(); // oldest → newest
    Some(names.join(" → "))
}

// --- Latest sample (single source for snapshot hints) --------------------------

fn last_sample() -> &'static Mutex<Option<EnvSample>> {
    static LAST: OnceLock<Mutex<Option<EnvSample>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

/// Hints for the perception snapshot / Debug Panel (plan P1 acceptance).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct EnvHints {
    pub title: Option<String>,
    pub file_hint: Option<String>,
    pub project_hint: Option<String>,
}

/// Current hints from the observer's latest sample. All None before the
/// first tick (~3 s after launch) — callers must tolerate that.
pub fn current_hints() -> EnvHints {
    last_sample()
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref().map(|s| EnvHints {
                title: s.title.clone(),
                file_hint: s.file_hint.clone(),
                project_hint: s.project_hint.clone(),
            })
        })
        .unwrap_or_default()
}

// --- Observer thread ------------------------------------------------------------

/// Start the environment observer. Sampling is entirely local; no LLM is
/// involved (plan §38-3). When window perception is off (Principle 6) the
/// thread never starts — titles are not even collected.
pub fn start(enable_window: bool) {
    if !enable_window {
        log::info!("[environment] observer disabled (perception.enable_window off)");
        return;
    }
    std::thread::spawn(move || {
        log::info!("[environment] observer started ({}s interval)", POLL_INTERVAL_SECS);
        let mut prev: Option<EnvSample> = None;
        // Away-period clock: when presence first left Active (for ReturnedBack).
        let mut away_since: Option<Instant> = None;

        loop {
            std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));

            let (proc, title_text) = window::foreground_info();
            let hints = title::parse_title(
                title_text.as_deref().unwrap_or(""),
                proc.as_deref(),
            );
            let presence_now = presence::current_presence();

            // Track the away period across samples; `away_secs` only matters
            // on the LongAway → Active transition.
            let mut away_secs = 0;
            if presence_now != PresenceState::Active {
                if away_since.is_none() {
                    away_since = Some(Instant::now());
                }
            } else if let Some(since) = away_since.take() {
                away_secs = since.elapsed().as_secs();
            }

            let cur = EnvSample {
                app: proc,
                title: title_text,
                file_hint: hints.file,
                project_hint: hints.project,
                presence: presence_now,
            };

            if let Some(p) = &prev {
                for ev in diff(p, &cur, away_secs) {
                    log::debug!("[environment] event: {:?}", ev);
                    push_event(ev);
                }
            }
            if let Ok(mut g) = last_sample().lock() {
                *g = Some(cur.clone());
            }
            prev = Some(cur);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(app: Option<&str>, file: Option<&str>, project: Option<&str>) -> EnvSample {
        EnvSample {
            app: app.map(|s| s.to_string()),
            title: None,
            file_hint: file.map(|s| s.to_string()),
            project_hint: project.map(|s| s.to_string()),
            presence: PresenceState::Active,
        }
    }

    #[test]
    fn diff_app_change() {
        let a = sample(Some("cursor.exe"), Some("agent.rs"), Some("liri"));
        let b = sample(Some("chrome.exe"), None, None);
        let evs = diff(&a, &b, 0);
        assert!(evs.contains(&EnvironmentEvent::AppChanged { app: "chrome.exe".into() }));
        assert!(evs.contains(&EnvironmentEvent::FileHintChanged {
            from: Some("agent.rs".into()),
            to: None
        }));
    }

    #[test]
    fn diff_file_switch_same_app() {
        let a = sample(Some("cursor.exe"), Some("grounding.rs"), Some("liri"));
        let b = sample(Some("cursor.exe"), Some("planner.rs"), Some("liri"));
        let evs = diff(&a, &b, 0);
        assert_eq!(
            evs,
            vec![EnvironmentEvent::FileHintChanged {
                from: Some("grounding.rs".into()),
                to: Some("planner.rs".into())
            }]
        );
    }

    #[test]
    fn diff_no_change_no_events() {
        let a = sample(Some("cursor.exe"), Some("agent.rs"), Some("liri"));
        let b = sample(Some("cursor.exe"), Some("agent.rs"), Some("liri"));
        assert!(diff(&a, &b, 0).is_empty());
    }

    #[test]
    fn diff_presence_returned() {
        let mut a = sample(Some("cursor.exe"), None, None);
        a.presence = PresenceState::LongAway;
        let b = sample(Some("cursor.exe"), None, None);
        let evs = diff(&a, &b, 600);
        assert_eq!(evs, vec![EnvironmentEvent::PresenceReturned { away_secs: 600 }]);
    }

    #[test]
    fn ring_buffer_caps_at_capacity() {
        // Reset the global ring for a clean test.
        if let Ok(mut q) = ring().lock() {
            q.clear();
        }
        for i in 0..(RING_CAP + 5) {
            push_event(EnvironmentEvent::AppChanged { app: format!("app{i}") });
        }
        assert_eq!(recent_events().len(), RING_CAP);
        // Oldest entries evicted — the newest is last.
        assert_eq!(
            recent_events().last(),
            Some(&EnvironmentEvent::AppChanged { app: format!("app{}", RING_CAP + 4) })
        );
    }

    #[test]
    fn summary_collapses_consecutive_duplicates() {
        if let Ok(mut q) = ring().lock() {
            q.clear();
        }
        push_event(EnvironmentEvent::FileHintChanged {
            from: Some("a.rs".into()),
            to: Some("grounding.rs".into()),
        });
        // Same file re-detected (title flicker) — must collapse.
        push_event(EnvironmentEvent::FileHintChanged {
            from: Some("grounding.rs".into()),
            to: Some("grounding.rs".into()),
        });
        push_event(EnvironmentEvent::FileHintChanged {
            from: Some("grounding.rs".into()),
            to: Some("planner.rs".into()),
        });
        push_event(EnvironmentEvent::FileHintChanged {
            from: Some("planner.rs".into()),
            to: Some("agent.rs".into()),
        });
        assert_eq!(
            recent_summary().as_deref(),
            Some("grounding.rs → planner.rs → agent.rs")
        );
    }

    #[test]
    fn summary_none_when_no_file_events() {
        if let Ok(mut q) = ring().lock() {
            q.clear();
        }
        push_event(EnvironmentEvent::AppChanged { app: "x.exe".into() });
        assert_eq!(recent_summary(), None);
    }
}
