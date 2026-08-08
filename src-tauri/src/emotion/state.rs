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
    label_for_mood_full(state.mood, state.stress, state.social_battery)
}

/// Closeness (0..100) below which Liri is still reserved with the user — the
/// "陌生时拘谨" early-relationship demeanor (design §6.2). Mirrors the
/// lonely-nudge / planner-Rule-4 gate (closeness >= 20 to reach out warmly),
/// inverted: below this she holds back.
pub const SHY_CLOSENESS_THRESHOLD: f64 = 20.0;

/// Like [`derive_mood_label`], but factors in relationship closeness. At low
/// closeness the neutral/positive moods surface as 害羞 (shy) instead of the
/// usual 平静/开心/调皮 — she holds back with someone she barely knows (design
/// §6.2 "陌生时拘谨"). Genuine distress (担心/疲惫/难过) is NOT masked: she can
/// be worried, tired, or sad regardless of how close you are. The base label
/// still comes from [`label_for_mood_full`], so there is a single source of
/// truth for the mood bands — this only adds a closeness override on top.
pub fn derive_mood_label_with_closeness(state: &EmotionState, closeness: f64) -> &'static str {
    let base = label_for_mood_full(state.mood, state.stress, state.social_battery);
    if closeness < SHY_CLOSENESS_THRESHOLD && !matches!(base, "担心" | "疲惫" | "难过") {
        "害羞"
    } else {
        base
    }
}

/// Derives a mood label from a single mood value (0..1).
/// Used by the homeostasis tick when only mood is available.
pub fn label_for_mood(mood: f64) -> &'static str {
    label_for_mood_full(mood, 0.0, 1.0)
}

/// Core label logic.
fn label_for_mood_full(mood: f64, stress: f64, social: f64) -> &'static str {
    if stress > 0.7 {
        "担心"
    } else if social < 0.2 {
        "疲惫"
    } else if mood < 0.3 {
        "难过"
    } else if mood < 0.45 {
        "平静"
    } else if mood > 0.7 {
        "开心"
    } else {
        "调皮"
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
        assert_eq!(derive_mood_label(&s), "开心");

        s.mood = 0.6;
        assert_eq!(derive_mood_label(&s), "调皮");

        s.mood = 0.5;
        assert_eq!(derive_mood_label(&s), "调皮");

        s.mood = 0.4;
        assert_eq!(derive_mood_label(&s), "平静");

        s.mood = 0.2;
        assert_eq!(derive_mood_label(&s), "难过");

        s.mood = 0.5;
        s.stress = 0.8;
        assert_eq!(derive_mood_label(&s), "担心");

        s.stress = 0.2;
        s.social_battery = 0.1;
        assert_eq!(derive_mood_label(&s), "疲惫");
    }

    #[test]
    fn test_shy_label_at_low_closeness() {
        let mut s = EmotionState::default();
        // Neutral/positive moods -> 害羞 in the early relationship.
        s.mood = 0.5; // would be 调皮
        assert_eq!(derive_mood_label_with_closeness(&s, 0.0), "害羞");
        assert_eq!(derive_mood_label_with_closeness(&s, 19.9), "害羞");
        s.mood = 0.4; // would be 平静
        assert_eq!(derive_mood_label_with_closeness(&s, 10.0), "害羞");
        s.mood = 0.8; // would be 开心
        assert_eq!(derive_mood_label_with_closeness(&s, 5.0), "害羞");
        // At/above the threshold the usual label returns.
        assert_eq!(derive_mood_label_with_closeness(&s, 20.0), "开心");
        assert_eq!(derive_mood_label_with_closeness(&s, 100.0), "开心");
    }

    #[test]
    fn test_shy_does_not_mask_distress() {
        let mut s = EmotionState::default();
        s.stress = 0.8; // 担心
        assert_eq!(derive_mood_label_with_closeness(&s, 0.0), "担心");
        s.stress = 0.2;
        s.social_battery = 0.1; // 疲惫
        assert_eq!(derive_mood_label_with_closeness(&s, 0.0), "疲惫");
        s.social_battery = 0.8;
        s.mood = 0.2; // 难过
        assert_eq!(derive_mood_label_with_closeness(&s, 0.0), "难过");
    }
}
