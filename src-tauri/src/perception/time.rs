//! Time perception: current time-of-day, time since last interaction.

use chrono::{Timelike, Utc};

/// Time of day categories affecting behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    LateNight,
    DeepNight,
}

impl std::fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Morning => write!(f, "morning"),
            Self::Afternoon => write!(f, "afternoon"),
            Self::Evening => write!(f, "evening"),
            Self::LateNight => write!(f, "late_night"),
            Self::DeepNight => write!(f, "deep_night"),
        }
    }
}

/// Determines the time-of-day from a local hour (0-23).
pub fn time_of_day(local_hour: u32) -> TimeOfDay {
    match local_hour {
        6..=10 => TimeOfDay::Morning,
        11..=16 => TimeOfDay::Afternoon,
        17..=22 => TimeOfDay::Evening,
        23 => TimeOfDay::LateNight,
        0..=2 => TimeOfDay::LateNight,
        _ => TimeOfDay::DeepNight,
    }
}

/// Returns the current local time-of-day.
pub fn current_time_of_day() -> TimeOfDay {
    let local = chrono::Local::now();
    time_of_day(local.hour())
}

/// Seconds since the given RFC3339 timestamp. Returns 0 if parsing fails.
pub fn seconds_since(timestamp: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|dt| (Utc::now() - dt.with_timezone(&Utc)).num_seconds().max(0) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_of_day_classification() {
        assert_eq!(time_of_day(8), TimeOfDay::Morning);
        assert_eq!(time_of_day(14), TimeOfDay::Afternoon);
        assert_eq!(time_of_day(20), TimeOfDay::Evening);
        assert_eq!(time_of_day(23), TimeOfDay::LateNight);
        assert_eq!(time_of_day(1), TimeOfDay::LateNight);
        assert_eq!(time_of_day(3), TimeOfDay::DeepNight);
        assert_eq!(time_of_day(5), TimeOfDay::DeepNight);
    }

    #[test]
    fn test_seconds_since_valid() {
        let ts = (Utc::now() - chrono::Duration::seconds(100)).to_rfc3339();
        let secs = seconds_since(&ts);
        assert!(secs >= 99 && secs <= 101, "expected ~100, got {}", secs);
    }

    #[test]
    fn test_seconds_since_invalid() {
        assert_eq!(seconds_since("invalid"), 0);
    }
}
