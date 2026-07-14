use super::state::EmotionState;

/// Loneliness growth rate: ~0.0001 per second (~2.5 hours from 0 to 1).
const LONELINESS_RATE: f64 = 0.0001;
/// Rest need growth rate when energy is low.
const REST_NEED_RATE: f64 = 0.0002;
/// Energy threshold below which rest need starts growing.
const LOW_ENERGY_THRESHOLD: f64 = 0.3;

/// Ticks the needs system (design doc 7.8).
/// Need -> Behavior -> Emotion (endogenous drive, not reactive).
/// MVP only implements Loneliness + Rest.
pub fn tick_needs(state: &mut EmotionState, elapsed_secs: f64, is_interacting: bool) {
    // Loneliness grows with time, drops sharply during interaction
    if is_interacting {
        state.loneliness = (state.loneliness * 0.5).max(0.0);
    } else {
        state.loneliness = (state.loneliness + elapsed_secs * LONELINESS_RATE).min(1.0);
    }

    // Rest need grows when energy is low
    if state.physical_energy < LOW_ENERGY_THRESHOLD {
        state.rest_need = (state.rest_need + elapsed_secs * REST_NEED_RATE).min(1.0);
    }
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
}
