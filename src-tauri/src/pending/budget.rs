//! Global proactive-bubble budget (frequency governor, 2026-08-14).
//!
//! Previously every proactive path gated itself: the 5-min frontend poll checked
//! an in-memory `last_proactive_bubble`, while the backend emitters (pending
//! follow-up / welcome-back / lonely nudge / ritual) gated only on their own
//! conditions — so several bubbles could stack within minutes and every app
//! restart reset the counter. This module is the ONE shared check-and-set:
//! persisted in `app_config` (survives restarts), atomic over the single
//! Mutex'd SQLite connection, and called by every emitter (and the poll) before
//! a bubble may fire. Architecture #6 (tunable via `[proactive] min_interval_secs`)
//! and #12 (silence is also expression: when the budget says no, she stays quiet).

use crate::db::DbState;
use chrono::{DateTime, Utc};

/// app_config key holding the last bubble time (RFC3339 UTC).
pub const LAST_BUBBLE_KEY: &str = "last_proactive_bubble_at";

/// Reads the persisted last-bubble time. None = never bubbled (first bubble allowed).
pub fn read_last_bubble(db: &DbState) -> Option<DateTime<Utc>> {
    db.with_conn(|conn| crate::db::onboarding::get(conn, LAST_BUBBLE_KEY))
        .ok()
        .flatten()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Atomically check-and-occupy the budget: returns true only when at least
/// `min_interval_secs` have passed since the last bubble (or never bubbled),
/// and in that case immediately writes the new timestamp. The winner of the
/// read-modify-write owns the slot — concurrent emitters/poll can't double-fire
/// (the check and the write happen inside one `with_conn` closure over the
/// single Mutex'd connection). Conservative: occupying on greenlight means a
/// later generation failure still consumes the slot (宁少勿突兀), matching the
/// old `check_proactive` semantics.
pub fn try_occupy_budget(db: &DbState, min_interval_secs: i64, now: DateTime<Utc>) -> bool {
    db.with_conn(|conn| {
        let last = crate::db::onboarding::get(conn, LAST_BUBBLE_KEY)
            .ok()
            .flatten()
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc));
        let ok = match last {
            Some(t) => (now - t).num_seconds() >= min_interval_secs,
            None => true,
        };
        if ok {
            let _ = crate::db::onboarding::save(conn, LAST_BUBBLE_KEY, &now.to_rfc3339());
        }
        Ok(ok)
    })
    .unwrap_or(false)
}

/// Occupies the budget unconditionally (writes now). Used by date-driven
/// rituals (早安): they fire regardless of how recent the last bubble was, but
/// they still consume the budget so no other bubble follows within the interval.
pub fn occupy_budget_always(db: &DbState) {
    let now = Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| crate::db::onboarding::save(conn, LAST_BUBBLE_KEY, &now));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn first_bubble_allowed() {
        let db = test_db();
        assert!(try_occupy_budget(&db, 3600, Utc::now()));
        // Second call immediately after must be denied (interval not elapsed).
        assert!(!try_occupy_budget(&db, 3600, Utc::now()));
    }

    #[test]
    fn interval_gate_respected_and_persisted() {
        let db = test_db();
        let now = Utc::now();
        assert!(try_occupy_budget(&db, 3600, now));
        // Simulate a fresh "process": budget survives restart via app_config.
        assert!(!try_occupy_budget(&db, 3600, now + chrono::Duration::minutes(10)));
        // After the interval, allowed again.
        assert!(try_occupy_budget(&db, 3600, now + chrono::Duration::minutes(61)));
        // Persisted value is readable.
        assert!(read_last_bubble(&db).is_some());
    }

    #[test]
    fn occupy_always_writes_regardless() {
        let db = test_db();
        let now = Utc::now();
        assert!(try_occupy_budget(&db, 3600, now));
        // 10s later a ritual wants to fire: occupy_always succeeds anyway.
        occupy_budget_always(&db);
        assert!(!try_occupy_budget(&db, 3600, now + chrono::Duration::seconds(10)));
        // The ritual's occupy moved the clock: allowed only after interval from it.
        assert!(try_occupy_budget(&db, 3600, now + chrono::Duration::minutes(61)));
    }
}
