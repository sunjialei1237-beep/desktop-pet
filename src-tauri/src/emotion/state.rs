/// Multi-dimensional emotion state (design doc 11.1).
/// Not discrete labels but a continuous vector for Live2D parameter interpolation.
#[derive(Debug, Clone)]
pub struct EmotionState {
    pub mood: f64,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
    pub rest_need: f64,
}

impl Default for EmotionState {
    fn default() -> Self {
        EmotionState {
            mood: 0.5,
            physical_energy: 0.7,
            social_battery: 0.8,
            stress: 0.2,
            loneliness: 0.0,
            rest_need: 0.0,
        }
    }
}

/// Derives a human-readable mood label from the emotion vector.
/// Used by the chat bubble system and debug panel.
pub fn derive_mood_label(state: &EmotionState) -> &'static str {
    if state.stress > 0.7 {
        "dan xin"
    } else if state.social_battery < 0.2 {
        "pi bei"
    } else if state.mood < 0.3 {
        "nan guo"
    } else if state.mood < 0.45 {
        "ping jing"
    } else if state.mood > 0.7 {
        "kai xin"
    } else {
        "tiao pi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let s = EmotionState::default();
        assert!((s.mood - 0.5).abs() < 0.001);
        assert!((s.stress - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_mood_labels() {
        let mut s = EmotionState::default();

        s.mood = 0.8;
        assert_eq!(derive_mood_label(&s), "kai xin");

        s.mood = 0.6;
        assert_eq!(derive_mood_label(&s), "tiao pi");

        s.mood = 0.5;
        assert_eq!(derive_mood_label(&s), "tiao pi");

        s.mood = 0.4;
        assert_eq!(derive_mood_label(&s), "ping jing");

        s.mood = 0.2;
        assert_eq!(derive_mood_label(&s), "nan guo");

        s.mood = 0.5;
        s.stress = 0.8;
        assert_eq!(derive_mood_label(&s), "dan xin");

        s.stress = 0.2;
        s.social_battery = 0.1;
        assert_eq!(derive_mood_label(&s), "pi bei");
    }
}
