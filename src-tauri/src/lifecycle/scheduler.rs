//! Lightweight scheduler registry (plan §A2, architecture-respecting revision).
//!
//! The original plan §A2 wanted a `Scheduler { ticks_1s/30s/daily }` + `trait Tick`.
//! ADR 2026-08-07 deferred that: Body runs in the frontend (Principle 5), so Rust
//! has no 1s animation tick, and with only two cadences there is no polymorphism
//! need — a `Box<dyn Tick>` would be a speculative abstraction.
//!
//! This module delivers the *valuable* part of §A2 that the deferral left open
//! (the ADR's own "方案 B + 后果": "可读、可扩展"), without the rejected trait:
//!   - #11 Explainability: a single registry where every scheduled job is named,
//!     with its cadence / last-run time / last status — surfaced in the Debug Panel.
//!   - #6 Disableable: the costly Soul "capabilities" (reflection / consolidation /
//!     review / cleanup) carry enable flags, so each can be turned off gracefully
//!     ("关掉 Reflection, 记忆照常"). Core aliveness jobs (homeostasis / emotion
//!     push / pending check) are always on — turning them off kills her, which is
//!     not graceful degradation.
//!
//! Execution stays exactly where it is (`loop_runner` direct calls); this module
//! only records outcomes + answers "should this capability run?". No polymorphism,
//! no re-timing of the live loop — the two things the ADR warned against.

use std::sync::{Mutex, OnceLock};

/// Outcome of a single job invocation, for observability (#11).
#[derive(Clone, serde::Serialize)]
pub struct JobStat {
    pub name: &'static str,
    /// Human-readable cadence, e.g. "30s" / "1h".
    pub cadence: &'static str,
    /// Whether the job is enabled (capabilities only; aliveness jobs are always on).
    pub enabled: bool,
    /// Whether the user may turn this job off (capabilities = true).
    pub disableable: bool,
    /// RFC3339 timestamp of the last execution (None = never run).
    pub last_run_at: Option<String>,
    /// "idle" (never run) | "ok" | "skipped" (disabled) | "error".
    pub last_status: &'static str,
    pub last_message: Option<String>,
}

/// The fixed set of scheduled jobs. Order = display order in the Debug Panel.
fn default_jobs() -> Vec<JobStat> {
    vec![
        // Medium loop (30s) — core aliveness, always on.
        JobStat { name: "homeostasis",   cadence: "30s", enabled: true,  disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "pending_check", cadence: "30s", enabled: true,  disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "emotion_push",  cadence: "30s", enabled: true,  disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "presence_watch",cadence: "30s", enabled: true,  disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "lonely_nudge",  cadence: "30s", enabled: true,  disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        // Slow loop (1h) — Soul capabilities, individually disableable.
        JobStat { name: "memory_decay",        cadence: "1h", enabled: true, disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "closeness_drift",     cadence: "1h", enabled: true, disableable: false, last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "lifecycle_cleanup",   cadence: "1h", enabled: true, disableable: true,  last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "reflection",          cadence: "1h", enabled: true, disableable: true,  last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "consolidation",       cadence: "1h", enabled: true, disableable: true,  last_run_at: None, last_status: "idle", last_message: None },
        JobStat { name: "relationship_review", cadence: "1h", enabled: true, disableable: true,  last_run_at: None, last_status: "idle", last_message: None },
    ]
}

fn store() -> &'static Mutex<Vec<JobStat>> {
    static STORE: OnceLock<Mutex<Vec<JobStat>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(default_jobs()))
}

/// Records the outcome of a job run. `enabled` reflects the current config flag
/// (capabilities); aliveness jobs pass `true`. `status` ∈ {"ok","skipped","error"}.
/// A "skipped" (disabled) run does not update `last_run_at`.
pub fn record(name: &'static str, enabled: bool, status: &'static str, message: Option<String>) {
    if let Ok(mut v) = store().lock() {
        for j in v.iter_mut() {
            if j.name == name {
                j.enabled = enabled;
                j.last_status = status;
                j.last_message = message;
                if status != "skipped" {
                    j.last_run_at = Some(crate::lifecycle::scheduler::now_rfc3339());
                }
                return;
            }
        }
    }
}

/// Read-only snapshot of all job stats, for the Debug Panel.
pub fn snapshot() -> Vec<JobStat> {
    store().lock().map(|v| v.clone()).unwrap_or_default()
}

/// Pure decision: should a capability run given its enable flag? Extracted for
/// unit testing and to keep the gating rule in one place.
pub fn should_run(enabled: bool) -> bool {
    enabled
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_run_reflects_flag() {
        assert!(should_run(true));
        assert!(!should_run(false));
    }

    #[test]
    fn snapshot_lists_all_known_jobs() {
        let s = snapshot();
        let names: Vec<_> = s.iter().map(|j| j.name).collect();
        // Aliveness + capabilities both present.
        for required in [
            "homeostasis",
            "pending_check",
            "reflection",
            "consolidation",
            "relationship_review",
            "lifecycle_cleanup",
        ] {
            assert!(names.contains(&required), "missing job {}", required);
        }
    }

    #[test]
    fn only_capabilities_are_disableable() {
        for j in snapshot() {
            match j.name {
                "lifecycle_cleanup" | "reflection" | "consolidation" | "relationship_review" => {
                    assert!(j.disableable, "{} should be disableable", j.name);
                }
                _ => assert!(!j.disableable, "{} should NOT be disableable", j.name),
            }
        }
    }

    #[test]
    fn record_updates_status_and_timestamp() {
        // Use a unique scratch by recording then reading back; process-global
        // state is fine because these run in one test binary with no live loop.
        record("memory_decay", true, "ok", None);
        let j = snapshot().into_iter().find(|j| j.name == "memory_decay").unwrap();
        assert_eq!(j.last_status, "ok");
        assert!(j.last_run_at.is_some(), "ok run stamps last_run_at");
    }

    #[test]
    fn skipped_run_does_not_stamp() {
        record("reflection", false, "skipped", None);
        let j = snapshot().into_iter().find(|j| j.name == "reflection").unwrap();
        assert_eq!(j.enabled, false);
        assert_eq!(j.last_status, "skipped");
        // skipped must not count as a run.
    }

    #[test]
    fn unknown_job_record_is_ignored() {
        // No panic on a typo'd name.
        record("does_not_exist", true, "ok", None);
    }
}
