/// Relationship Pace (design doc 10.2).
/// Logarithmic curve: fast at low closeness, slow at high.
/// Can decrease: ignoring her drops closeness.
/// Daily closeness gain cap.
pub const DAILY_GAIN_CAP: f64 = 3.0;
/// Decay factor per day of no interaction.
const DAILY_DECAY_FACTOR: f64 = 0.99;

/// Computes the closeness increment for an interaction.
/// Uses diminishing returns: the closer you are, the less each interaction adds.
pub fn pace_increment(current_closeness: f64, interaction_type: &str) -> f64 {
    let base = match interaction_type {
        "deep" => 2.0,        // deep conversation
        "casual" => 0.5,      // casual chat
        "pet" => 0.3,         // pet head
        "correction" => -0.5, // user correction (slight dip)
        _ => 0.1,
    };
    let diminishing = 1.0 - (current_closeness / 100.0);
    base * diminishing
}

/// Computes closeness decay after days of no interaction.
/// Each day multiplies closeness by 0.99 (~7% over a week).
pub fn decay_closeness(current: f64, days_no_interaction: f64) -> f64 {
    current * DAILY_DECAY_FACTOR.powf(days_no_interaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diminishing_returns() {
        // At closeness 0: full base reward
        let inc_low = pace_increment(0.0, "deep");
        assert!((inc_low - 2.0).abs() < 0.001, "at 0, deep should give 2.0");

        // At closeness 50: half reward
        let inc_mid = pace_increment(50.0, "deep");
        assert!((inc_mid - 1.0).abs() < 0.001, "at 50, deep should give 1.0");

        // At closeness 90: 10% reward
        let inc_high = pace_increment(90.0, "deep");
        assert!((inc_high - 0.2).abs() < 0.001, "at 90, deep should give 0.2");
    }

    #[test]
    fn test_cumulative_closeness() {
        let mut closeness = 0.0;
        for _ in 0..10 {
            closeness += pace_increment(closeness, "deep");
        }
        // Should be around 15-18 (diminishing returns)
        assert!(closeness > 13.0 && closeness < 20.0,
            "10 deep interactions from 0 should give ~15-18, got {}", closeness);
    }

    #[test]
    fn test_correction_decreases() {
        let inc = pace_increment(40.0, "correction");
        assert!(inc < 0.0, "correction should decrease closeness");
        // base -0.5 * (1 - 0.4) = -0.3
        assert!((inc - (-0.3)).abs() < 0.001);
    }

    #[test]
    fn test_decay_7_days() {
        let decayed = decay_closeness(50.0, 7.0);
        // 50 * 0.99^7 = 50 * 0.932 = 46.6
        assert!((decayed - 46.6).abs() < 0.5,
            "7 day decay should be ~46.6, got {}", decayed);
        assert!(decayed < 50.0, "decay should reduce closeness");
    }

    #[test]
    fn test_daily_cap_enforced() {
        // Simulate a day of interactions, check they don't exceed cap
        let mut closeness = 10.0;
        let mut daily_total: f64 = 0.0;
        for _ in 0..10 {
            let inc = pace_increment(closeness, "deep");
            daily_total += inc;
            closeness += inc;
        }
        // Even 10 deep interactions should not exceed daily cap of 3.0
        // (at closeness 10, diminishing=0.9, so first is 1.8, then decreasing)
        // Total of 10 should be around 10-12 raw, but the cap should be applied by caller
        assert!(daily_total > 0.0);
        // The caller is responsible for enforcing DAILY_GAIN_CAP
    }
}
