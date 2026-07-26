use super::state::EmotionState;

/// Time constants (tau) for exponential drift toward baseline, in seconds.
/// Larger tau = slower drift. Design doc 7.7 table.
const TAU_MOOD: f64 = 300.0; // minutes
const TAU_STRESS: f64 = 3600.0; // 1 hour 鈥?was 7200s, halved so absorbed
                                // stress recovers faster (breaks anxiety loop)
const TAU_ENERGY: f64 = 1800.0; // 30 min, halved when sleeping
const TAU_SOCIAL: f64 = 600.0; // 10 min

/// Baselines from design doc 7.7.
const BL_MOOD: f64 = 0.5;
const BL_STRESS: f64 = 0.2;
const BL_ENERGY: f64 = 0.7;
const BL_SOCIAL: f64 = 0.8;

/// Applies homeostatic drift: each dimension moves toward its baseline
/// using exponential interpolation: value += (baseline - value) * (1 - exp(-elapsed/tau)).
/// loneliness and rest_need are NOT drifted here (managed by needs.rs).
pub fn apply_drift(state: &mut EmotionState, elapsed_secs: f64, is_sleeping: bool) {
    state.mood = drift_toward(state.mood, BL_MOOD, elapsed_secs, TAU_MOOD);
    state.stress = drift_toward(state.stress, BL_STRESS, elapsed_secs, TAU_STRESS);

    let energy_tau = if is_sleeping { TAU_ENERGY * 0.5 } else { TAU_ENERGY };
    state.physical_energy =
        drift_toward(state.physical_energy, BL_ENERGY, elapsed_secs, energy_tau);

    let social_tau = if is_sleeping { TAU_SOCIAL * 0.5 } else { TAU_SOCIAL };
    state.social_battery =
        drift_toward(state.social_battery, BL_SOCIAL, elapsed_secs, social_tau);
}

/// Exponential interpolation toward a target.
/// rate = 1 - exp(-elapsed / tau)
fn drift_toward(value: f64, target: f64, elapsed: f64, tau: f64) -> f64 {
    let rate = 1.0 - (-elapsed / tau).exp();
    value + (target - value) * rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stress_recovery() {
        let mut s = EmotionState::default();
        s.stress = 0.9;

        // 1 hour = 3600s, tau = 3600s (now 1 elapsed tau)
        apply_drift(&mut s, 3600.0, false);

        // After 1 tau/half-elapsed, stress should have dropped noticeably
        assert!(s.stress < 0.9, "stress should have dropped");
        // rate = 1 - exp(-3600/3600) = 1 - exp(-1) ~ 0.632
        // new = 0.9 + (0.2 - 0.9) * 0.632 = 0.9 - 0.442 = 0.458
        assert!((s.stress - 0.458).abs() < 0.05, "stress should be ~0.458, got {}", s.stress);
    }

    #[test]
    fn test_mood_recovers_fast() {
        let mut s = EmotionState::default();
        s.mood = 0.1;

        // 5 minutes = 300s = 1 tau
        apply_drift(&mut s, 300.0, false);

        // rate = 1 - exp(-1) ~ 0.632
        // new = 0.1 + (0.5 - 0.1) * 0.632 = 0.1 + 0.253 = 0.353
        assert!(s.mood > 0.1, "mood should have improved");
        assert!((s.mood - 0.353).abs() < 0.02, "mood should be ~0.353, got {}", s.mood);
    }

    #[test]
    fn test_energy_recovers_faster_when_sleeping() {
        let mut s1 = EmotionState::default();
        s1.physical_energy = 0.2;
        let mut s2 = s1.clone();

        // 900s awake
        apply_drift(&mut s1, 900.0, false);
        // 900s sleeping
        apply_drift(&mut s2, 900.0, true);

        assert!(s2.physical_energy > s1.physical_energy, "sleeping should recover energy faster");
    }

    #[test]
    fn test_no_drift_at_baseline() {
        let mut s = EmotionState::default();
        let before = s.mood;
        apply_drift(&mut s, 3600.0, false);
        assert!((s.mood - before).abs() < 0.001, "at baseline, no drift");
    }
}
