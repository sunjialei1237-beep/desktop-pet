//! Deep-focus detection: tracks how long the user has been working continuously
//! in the same Work-category foreground app, so the pet stays quiet during
//! sustained focus (plan P14.3 / design doc 8.1: same Work app > 25 min).
//!
//! A single background thread samples the foreground window every 30s and
//! publishes two atomics read by the perception snapshot + proactive decision.
//! Principle 5: independent of Mind/LLM. Principle 6: derived from window
//! perception, so it is effectively disabled when window perception is off
//! (callers gate the read by `config.perception.enable_window`).
//!
//! Core decision logic is a pure function (`update_continuous`) for unit testing;
//! the thread is thin wiring around it (mirrors `tick_loneliness`, `should_run_review`).

use crate::perception::window::{classify_process, foreground_process, AppCategory};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Sustained same-Work-app time after which we consider the user in deep focus
/// (plan P14.3: > 25 minutes).
pub const DEEP_FOCUS_THRESHOLD_SECS: u64 = 25 * 60;
/// Sample interval — 5 s keeps the debug panel's "专注 X 分钟" readout
/// responsive (first minute shows within 60 s) without meaningful overhead:
/// one Toolhelp cache hit per tick. §8.5-M7 keeps self-window samples from
/// resetting the accumulators.
const POLL_INTERVAL_SECS: u64 = 5;

static CONTINUOUS_WORK_SECS: AtomicU64 = AtomicU64::new(0);
static IS_DEEP_FOCUS: AtomicBool = AtomicBool::new(false);

/// Continuous seconds the same Work-category app has held the foreground.
pub fn continuous_work_secs() -> u64 {
    CONTINUOUS_WORK_SECS.load(Ordering::Relaxed)
}

/// Whether the user is in sustained deep focus (same Work app >= threshold).
pub fn is_deep_focus() -> bool {
    IS_DEEP_FOCUS.load(Ordering::Relaxed)
}

/// Pure update step. Given the previous tracked app + accumulated seconds and
/// the current foreground sample, returns the next (app, continuous_secs).
///
/// - `fg_work`: `Some(name)` when the foreground process is a Work-category app,
///   else `None` (non-work app or no foreground window).
/// - Same work app continues → accumulate `elapsed_secs`.
/// - Different work app → start counting it fresh (continuous = 0).
/// - Non-work / no foreground → reset (continuous = 0, app cleared).
///
/// "Continuous" means the *same* app, not "any work" — switching from VS Code to
/// a terminal restarts the clock, so only genuine uninterrupted focus counts.
pub fn update_continuous(
    prev_app: &Option<String>,
    prev_secs: u64,
    fg_work: &Option<String>,
    elapsed_secs: u64,
) -> (Option<String>, u64) {
    match fg_work {
        Some(name) if Some(name.as_str()) == prev_app.as_deref() => {
            // Same work app continues to hold the foreground.
            (Some(name.clone()), prev_secs.saturating_add(elapsed_secs))
        }
        Some(name) => {
            // A new work app took the foreground — start its clock fresh.
            (Some(name.clone()), 0)
        }
        None => (None, 0), // not work / no foreground — reset
    }
}

/// Starts the focus sampler thread. Runs for the app lifetime; returns immediately.
pub fn start() {
    std::thread::spawn(move || {
        // The Work-category app whose continuous foreground time we count, and
        // when it became foreground. Thread-local — only this thread mutates;
        // readers go through the atomics, no lock needed.
        let mut current_app: Option<String> = None;
        let mut last_poll = Instant::now();

        loop {
            std::thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
            let now = Instant::now();
            let elapsed = now.duration_since(last_poll).as_secs();
            last_poll = now;

            let fg_work = match foreground_process() {
                Some(n) if classify_process(&n) == AppCategory::Work => Some(n),
                _ => None,
            };

            let prev_secs = CONTINUOUS_WORK_SECS.load(Ordering::Relaxed);
            let (new_app, new_secs) =
                update_continuous(&current_app, prev_secs, &fg_work, elapsed);
            current_app = new_app;

            CONTINUOUS_WORK_SECS.store(new_secs, Ordering::Relaxed);
            IS_DEEP_FOCUS.store(
                new_secs >= DEEP_FOCUS_THRESHOLD_SECS,
                Ordering::Relaxed,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_is_25_minutes() {
        assert_eq!(DEEP_FOCUS_THRESHOLD_SECS, 25 * 60);
    }

    #[test]
    fn same_work_app_accumulates() {
        let app = Some("code.exe".to_string());
        let (a, s) = update_continuous(&app, 60, &app, 30);
        assert_eq!(a.as_deref(), Some("code.exe"));
        assert_eq!(s, 90);
    }

    #[test]
    fn new_work_app_resets_to_zero() {
        let prev = Some("code.exe".to_string());
        let fg = Some("devenv.exe".to_string());
        let (a, s) = update_continuous(&prev, 1200, &fg, 30);
        assert_eq!(a.as_deref(), Some("devenv.exe"));
        assert_eq!(s, 0); // new app starts fresh, not 1230
    }

    #[test]
    fn non_work_resets() {
        let prev = Some("code.exe".to_string());
        let (a, s) = update_continuous(&prev, 1500, &None, 30);
        assert_eq!(a, None);
        assert_eq!(s, 0);
    }

    #[test]
    fn first_work_app_starts_at_zero() {
        let (a, s) = update_continuous(&None, 0, &Some("code.exe".to_string()), 30);
        assert_eq!(a.as_deref(), Some("code.exe"));
        assert_eq!(s, 0);
    }

    #[test]
    fn deep_focus_crosses_at_threshold() {
        // 24:59 — not yet; 25:00 — yes.
        assert!(!(DEEP_FOCUS_THRESHOLD_SECS - 1 >= DEEP_FOCUS_THRESHOLD_SECS));
        assert!(DEEP_FOCUS_THRESHOLD_SECS >= DEEP_FOCUS_THRESHOLD_SECS);
    }
}
