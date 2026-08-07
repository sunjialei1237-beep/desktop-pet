use super::state::EmotionState;

/// Loneliness growth rate: ~0.0001 per second (~2.5 hours from 0 to 1).
const LONELINESS_RATE: f64 = 0.0001;
/// Rest need growth rate when energy is low.
const REST_NEED_RATE: f64 = 0.0002;
/// Energy threshold below which rest need starts growing.
const LOW_ENERGY_THRESHOLD: f64 = 0.3;
/// Rest-need recovery time constant (seconds). Mirrors energy's tau (homeostasis)
/// so rest tracks energy recovery: once energy drifts back above the low
/// threshold, accumulated rest need decays away over ~30 min. Without this the
/// in-memory `tick_needs` only ever grew rest_need (monotonic) -- a rested pet
/// would never reopen her eyes. (Architecture Principle #1: pure rule, no LLM.)
const TAU_REST: f64 = 1800.0;

/// Evolve rest_need over `elapsed_secs` given the current energy level.
/// Grows when energy is low (pet tiring -> droopy eyes via emotionDriver),
/// recovers exponentially toward 0 when energy is adequate. Extracted so both
/// the in-memory EmotionState path (`tick_needs`) and the production DB
/// homeostasis path (`db::emotion::apply_homeostasis_time_aware`) share one rule.
pub fn tick_rest_need(rest_need: f64, energy: f64, elapsed_secs: f64) -> f64 {
    if energy < LOW_ENERGY_THRESHOLD {
        (rest_need + elapsed_secs * REST_NEED_RATE).min(1.0)
    } else {
        // Exponential decay toward 0 (equivalent to drift_toward(r, 0, elapsed, TAU_REST)).
        rest_need * (-elapsed_secs / TAU_REST).exp()
    }
}

/// Evolve loneliness over `elapsed_secs` of *idle* time (no interaction).
/// Loneliness climbs slowly toward 1 — she misses the user when they've been
/// away. The interaction drop is handled separately by the `react` deltas
/// applied during each conversation turn (see `mind::converse`), so this pure
/// rule only models the growth term. Extracted so both the in-memory path
/// (`tick_needs`) and the production DB homeostasis path
/// (`db::emotion::apply_homeostasis_time_aware`) share one rule — previously
/// only `tick_needs` grew loneliness and it was never called in production, so
/// loneliness was frozen at its seed value and planner Rule 4 (high loneliness
/// -> proactive accompany) could never fire. (Architecture Principle #1: pure
/// rule, no LLM.)
pub fn tick_loneliness(loneliness: f64, elapsed_secs: f64) -> f64 {
    (loneliness + elapsed_secs * LONELINESS_RATE).min(1.0)
}

/// Ticks the needs system (design doc 7.8).
/// Need -> Behavior -> Emotion (endogenous drive, not reactive).
/// MVP only implements Loneliness + Rest.
pub fn tick_needs(state: &mut EmotionState, elapsed_secs: f64, is_interacting: bool) {
    // Loneliness grows with time, drops sharply during interaction
    if is_interacting {
        state.loneliness = (state.loneliness * 0.5).max(0.0);
    } else {
        state.loneliness = tick_loneliness(state.loneliness, elapsed_secs);
    }

    // Rest need: grows when energy is low, recovers when rested (shared rule).
    state.rest_need = tick_rest_need(state.rest_need, state.physical_energy, elapsed_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loneliness_growth() {
        let mut s = EmotionState::default();
        // 3 hours = 10800s, rate = 0.0001/s -> +1.08 (clamped to 1.0)
        tick_needs(&mut s, 10800.0, false);
        assert!(s.loneliness > 0.5, "loneliness should be high after 3h, got {}", s.loneliness);
        assert!(s.loneliness <= 1.0, "loneliness should be clamped to 1.0");
    }

    #[test]
    fn test_interaction_reduces_loneliness() {
        let mut s = EmotionState::default();
        s.loneliness = 0.8;

        tick_needs(&mut s, 30.0, true);
        assert!(s.loneliness < 0.8, "interaction should reduce loneliness");
        assert!((s.loneliness - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_loneliness_grows_and_clamps() {
        // Pure growth rule, isolated from EmotionState: 1h idle @ 0.0001/s -> +0.36.
        let v = tick_loneliness(0.0, 3600.0);
        assert!((v - 0.36).abs() < 0.001, "1h idle from 0 -> 0.36, got {}", v);
        // Accumulates from a non-zero base.
        let v2 = tick_loneliness(0.5, 3600.0);
        assert!((v2 - 0.86).abs() < 0.001, "0.5 + 1h -> 0.86, got {}", v2);
        // Clamps at 1.0 (3h from 0.5 would be 1.58 -> 1.0).
        let v3 = tick_loneliness(0.5, 10800.0);
        assert_eq!(v3, 1.0, "should clamp at 1.0, got {}", v3);
    }

    #[test]
    fn test_rest_need_grows_when_low_energy() {
        let mut s = EmotionState::default();
        s.physical_energy = 0.15; // below threshold

        tick_needs(&mut s, 3600.0, false); // 1 hour
        // rest_need += 3600 * 0.0002 = 0.72
        assert!(s.rest_need > 0.5, "rest need should be high after 1h low energy, got {}", s.rest_need);
    }

    #[test]
    fn test_rest_need_stable_when_high_energy() {
        let mut s = EmotionState::default();
        s.physical_energy = 0.7; // above threshold

        tick_needs(&mut s, 3600.0, false);
        assert!((s.rest_need - 0.0).abs() < 0.001, "rest need should not grow when energy is fine");
    }

    #[test]
    fn test_rest_need_recovers_when_energy_high() {
        // A tired pet (high rest_need) whose energy has recovered should see her
        // rest need decay back down -- non-monotonic, so droopy eyes reopen.
        let mut s = EmotionState::default();
        s.rest_need = 0.8;
        s.physical_energy = 0.7; // above threshold -> recovery branch

        tick_needs(&mut s, 3600.0, false); // 1 hour rested
        // 0.8 * exp(-3600/1800) = 0.8 * exp(-2) ~ 0.108
        assert!(s.rest_need < 0.2, "rest need should decay when energy is adequate, got {}", s.rest_need);
        assert!(s.rest_need > 0.0, "rest need should not hit zero instantly");
    }
}
