use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionState {
    pub mood: f64,
    pub mood_label: String,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
    pub rest_need: f64,
    pub bl_mood: f64,
    pub bl_energy: f64,
    pub bl_social: f64,
    pub bl_stress: f64,
    pub last_homeostasis_at: String,
    pub updated_at: String,
}

/// Gets the singleton emotion state.
pub fn get(conn: &Connection) -> Result<EmotionState, String> {
    conn.query_row(
        "SELECT mood, mood_label, physical_energy, social_battery, stress,
                loneliness, rest_need, bl_mood, bl_energy, bl_social, bl_stress,
                last_homeostasis_at, updated_at
         FROM emotion_state WHERE id = 1",
        [],
        |row| {
            Ok(EmotionState {
                mood: row.get(0)?,
                mood_label: row.get(1)?,
                physical_energy: row.get(2)?,
                social_battery: row.get(3)?,
                stress: row.get(4)?,
                loneliness: row.get(5)?,
                rest_need: row.get(6)?,
                bl_mood: row.get(7)?,
                bl_energy: row.get(8)?,
                bl_social: row.get(9)?,
                bl_stress: row.get(10)?,
                last_homeostasis_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        },
    )
    .map_err(|e| format!("Failed to get emotion state: {}", e))
}

/// Updates emotion fields. Only provided (Some) fields are changed.
pub fn update_fields(
    conn: &Connection,
    mood: Option<f64>,
    mood_label: Option<&str>,
    physical_energy: Option<f64>,
    social_battery: Option<f64>,
    stress: Option<f64>,
    loneliness: Option<f64>,
    rest_need: Option<f64>,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE emotion_state SET
            mood = COALESCE(?1, mood),
            mood_label = COALESCE(?2, mood_label),
            physical_energy = COALESCE(?3, physical_energy),
            social_battery = COALESCE(?4, social_battery),
            stress = COALESCE(?5, stress),
            loneliness = COALESCE(?6, loneliness),
            rest_need = COALESCE(?7, rest_need),
            updated_at = ?8
         WHERE id = 1",
        params![
            mood, mood_label, physical_energy, social_battery,
            stress, loneliness, rest_need, now,
        ],
    )
    .map_err(|e| format!("Failed to update emotion: {}", e))?;
    Ok(())
}

/// Applies homeostasis: drifts all values toward their baselines.
/// rate is 0..1, where 1 = instantly snap to baseline, 0 = no movement.
pub fn apply_homeostasis(conn: &Connection, rate: f64, now: &str) -> Result<(), String> {
    let before = get(conn).ok();
    conn.execute(
        "UPDATE emotion_state SET
            mood = mood + (bl_mood - mood) * ?1,
            physical_energy = physical_energy + (bl_energy - physical_energy) * ?1,
            social_battery = social_battery + (bl_social - social_battery) * ?1,
            stress = stress + (bl_stress - stress) * ?1,
            last_homeostasis_at = ?2,
            updated_at = ?2
         WHERE id = 1",
        params![rate, now],
    )
    .map_err(|e| format!("Failed to apply homeostasis: {}", e))?;
    if let Some(b) = before {
        let _ = crate::db::changelog::append(
            conn, "emotion", "homeostasis", Some("emotion_state"), None,
            Some(&format!("mood={:.3},stress={:.3}", b.mood, b.stress)),
            Some(&format!("rate={:.3}", rate)),
            Some("medium-loop tick"),
        );
    }
    Ok(())
}

/// Time constants (tau) for exponential drift toward baseline, in seconds.
/// Matches emotion::homeostasis design-doc values.
const TAU_MOOD: f64 = 300.0;
const TAU_STRESS: f64 = 7200.0;
const TAU_ENERGY: f64 = 1800.0;
const TAU_SOCIAL: f64 = 600.0;

/// Maximum elapsed time (seconds) we compensate in one tick.
/// Prevents runaway drift after very long suspends.
const MAX_CATCHUP_SECS: f64 = 86400.0; // 24 hours

/// Threshold (seconds) above which we consider a gap a suspend/resume event.
pub const SUSPEND_THRESHOLD_SECS: f64 = 300.0; // 5 minutes

/// Applies time-aware homeostatic drift.
/// Computes actual elapsed seconds since last_homeostasis_at and uses exponential
/// interpolation toward baselines. Naturally handles suspend/resume: if the system
/// slept for hours, the drift formula accounts for the full elapsed time.
///
/// Returns the elapsed seconds (capped at MAX_CATCHUP_SECS) so callers can detect
/// suspend events (elapsed > SUSPEND_THRESHOLD_SECS).
pub fn apply_homeostasis_time_aware(conn: &Connection, now: &str) -> Result<f64, String> {
    let current = get(conn)?;
    let elapsed = compute_elapsed_secs(&current.last_homeostasis_at, now);

    let new_mood = drift_toward(current.mood, current.bl_mood, elapsed, TAU_MOOD);
    let new_energy = drift_toward(current.physical_energy, current.bl_energy, elapsed, TAU_ENERGY);
    let new_social = drift_toward(current.social_battery, current.bl_social, elapsed, TAU_SOCIAL);
    let new_stress = drift_toward(current.stress, current.bl_stress, elapsed, TAU_STRESS);

    // Rest need evolves via the shared needs rule (grows when energy is low,
    // recovers when rested). The in-memory `tick_needs` was never wired here, so
    // rest_need was previously frozen at its seed value -- exposing it to the
    // frontend had no visible effect. Activating it now lets droopy-tired eyes
    // actually appear (Architecture Principle #10: liveliness).
    let new_rest_need =
        crate::emotion::tick_rest_need(current.rest_need, current.physical_energy, elapsed);

    // Loneliness drifts up over idle time via the shared needs rule. The
    // interaction drop is applied as react deltas during conversation
    // (mind::converse), so homeostasis only models the growth term. Previously
    // loneliness was never updated here -> it froze at its seed value and the
    // planner's "high loneliness -> accompany" rule was unreachable in
    // production. Activating it lets her actually miss the user (Architecture
    // Principle #1: pure rule; #10: liveliness).
    let new_loneliness = crate::emotion::tick_loneliness(current.loneliness, elapsed);

    let new_label = crate::emotion::state::label_for_mood(new_mood);

    conn.execute(
        "UPDATE emotion_state SET
            mood = ?1, mood_label = ?2,
            physical_energy = ?3, social_battery = ?4, stress = ?5,
            rest_need = ?6, loneliness = ?7,
            last_homeostasis_at = ?8, updated_at = ?8
         WHERE id = 1",
        params![
            new_mood,
            new_label,
            new_energy,
            new_social,
            new_stress,
            new_rest_need,
            new_loneliness,
            now
        ],
    )
    .map_err(|e| format!("Failed to apply time-aware homeostasis: {}", e))?;

    if elapsed > SUSPEND_THRESHOLD_SECS {
        let _ = crate::db::changelog::append(
            conn, "emotion", "resume", Some("emotion_state"), None,
            Some(&format!("last_at={}", current.last_homeostasis_at)),
            Some(&format!("elapsed_secs={:.0}", elapsed)),
            Some("suspend/resume detected"),
        );
    }

    Ok(elapsed)
}

/// Computes elapsed seconds between two RFC 3339 timestamps, capped at MAX_CATCHUP_SECS.
fn compute_elapsed_secs(last_at: &str, now: &str) -> f64 {
    let elapsed = match (
        chrono::DateTime::parse_from_rfc3339(last_at),
        chrono::DateTime::parse_from_rfc3339(now),
    ) {
        (Ok(last), Ok(n)) => (n.with_timezone(&chrono::Utc) - last.with_timezone(&chrono::Utc)).num_seconds().max(0) as f64,
        _ => 30.0, // fallback: assume one tick
    };
    elapsed.min(MAX_CATCHUP_SECS)
}

/// Exponential interpolation toward a target.
fn drift_toward(value: f64, target: f64, elapsed: f64, tau: f64) -> f64 {
    let rate = 1.0 - (-elapsed / tau).exp();
    value + (target - value) * rate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn test_time_aware_homeostasis_normal() {
        let db = test_db();
        db.with_conn(|conn| {
            conn.execute("UPDATE emotion_state SET last_homeostasis_at = '2026-01-01T00:00:00+00:00' WHERE id = 1", []).map_err(|e| format!("{}", e))?;
            update_fields(conn, Some(0.1), None, None, None, Some(0.8), None, None, "2026-01-01T00:00:00+00:00")?;
            let elapsed = apply_homeostasis_time_aware(conn, "2026-01-01T00:05:00+00:00")?;
            assert!((elapsed - 300.0).abs() < 1.0, "5 min elapsed");
            let emo = get(conn)?;
            assert!(emo.stress < 0.8, "stress should drift toward baseline");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_time_aware_homeostasis_grows_loneliness() {
        // The production homeostasis path must grow loneliness over idle time.
        // Previously loneliness was never updated here -> frozen at its seed
        // value -> planner Rule 4 (high loneliness -> accompany) unreachable.
        let db = test_db();
        db.with_conn(|conn| {
            // Start at loneliness 0, 1h ago.
            update_fields(conn, None, None, None, None, None, Some(0.0), None, "2026-01-01T00:00:00+00:00")?;
            conn.execute("UPDATE emotion_state SET last_homeostasis_at = '2026-01-01T00:00:00+00:00' WHERE id = 1", []).map_err(|e| format!("{}", e))?;
            let _ = apply_homeostasis_time_aware(conn, "2026-01-01T01:00:00+00:00")?;
            let emo = get(conn)?;
            // 1h = 3600s @ 0.0001/s -> +0.36
            assert!(
                (emo.loneliness - 0.36).abs() < 0.01,
                "loneliness should grow ~0.36 over 1h idle, got {}",
                emo.loneliness
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_time_aware_homeostasis_suspend() {
        let db = test_db();
        db.with_conn(|conn| {
            conn.execute("UPDATE emotion_state SET last_homeostasis_at = '2026-01-01T00:00:00+00:00' WHERE id = 1", []).map_err(|e| format!("{}", e))?;
            update_fields(conn, Some(0.1), None, None, None, Some(0.9), None, None, "2026-01-01T00:00:00+00:00")?;
            // 8 hours later (suspend/resume)
            let elapsed = apply_homeostasis_time_aware(conn, "2026-01-01T08:00:00+00:00")?;
            assert!(elapsed > SUSPEND_THRESHOLD_SECS, "should detect time jump");
            assert!((elapsed - 28800.0).abs() < 1.0, "8h elapsed");
            let emo = get(conn)?;
            // After 8h, stress should be very close to baseline
            assert!((emo.stress - 0.2).abs() < 0.05, "stress fully recovered, got {}", emo.stress);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_emotion_singleton() {
        let db = test_db();
        db.with_conn(|conn| {
            let emo = get(conn)?;
            assert!((emo.mood - 0.5).abs() < 0.001);
            assert!((emo.physical_energy - 0.7).abs() < 0.001);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_update_and_homeostasis() {
        let db = test_db();
        db.with_conn(|conn| {
            // Raise stress to 0.8
            update_fields(conn, None, None, None, None, Some(0.8), None, None, "now")?;
            let emo = get(conn)?;
            assert!((emo.stress - 0.8).abs() < 0.001);

            // Apply homeostasis: should move stress toward baseline 0.2
            apply_homeostasis(conn, 0.5, "now")?;
            let emo = get(conn)?;
            assert!(emo.stress < 0.8, "stress should have drifted toward baseline");
            assert!((emo.stress - 0.5).abs() < 0.001, "halfway between 0.8 and 0.2");
            Ok(())
        })
        .unwrap();
    }
}
