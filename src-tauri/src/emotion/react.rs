//! Rule-based emotion reactivity (architecture principle #8: pure rules, no LLM).
//!
//! converse() calls `react_to_turn` after a successful reply and writes the
//! delta back to the DB, so the expression (f00-f07) reflects the conversation
//! in near-real-time rather than only the 30s homeostasis drift.
//!
//! `EmotionDelta` here is intentionally small per turn (clamped), so mood
//! accumulates over a conversation but a single off message can't wreck it.

/// Per-turn emotion delta. Default = all zero (no change).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmotionDelta {
    pub mood: f64,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
}

/// Pure: given the user's text this turn and the planner's intent goal, return
/// the emotion delta to apply. No LLM call, no DB access.
///
/// Baseline: any conversation costs a little social energy and eases loneliness
/// (she's keeping the user company). Good news lifts her mood; anxiety absorbs
/// some stress. Intent overlays (celebrate / engage / care / silence) stack on
/// top. Each field is clamped to a small per-turn range so values stay sane.
pub fn react_to_turn(user_text: &str, intent_goal: &str) -> EmotionDelta {
    let mut d = EmotionDelta::default();

    // Baseline: talking to someone = company (less lonely) but a touch draining.
    d.social_battery = -0.03;
    d.loneliness = -0.08;

    // User shares good news -> she's happy too, a little energized.
    if crate::mind::planner::is_good_news(user_text) {
        d.mood += 0.12;
        d.physical_energy += 0.05;
    }

    // User expresses anxiety/stress -> she absorbs some stress, mood dips.
    if crate::mind::planner::is_anxiety_expression(user_text) {
        d.stress += 0.04; // halved: full empathy absorption pushed pet stress
                         // into a runaway loop with anxiety->silence routing
        d.mood -= 0.05;
    }

    // Intent overlays.
    match intent_goal {
        "celebrate" => {
            d.mood += 0.10;
            d.physical_energy += 0.05;
        }
        "engage" => {
            d.mood += 0.04; // sharing-style talk, mildly positive
        }
        "care" => {
            d.social_battery -= 0.03; // emotional labor costs a bit more
        }
        "silence" => {
            d.stress += 0.05; // user is anxious -> she feels it too
            d.mood -= 0.03;
        }
        _ => {}
    }

    // Clamp each field to a sane per-turn range.
    d.mood = d.mood.clamp(-0.15, 0.15);
    d.physical_energy = d.physical_energy.clamp(-0.10, 0.10);
    d.social_battery = d.social_battery.clamp(-0.10, 0.05);
    d.stress = d.stress.clamp(-0.05, 0.08);
    d.loneliness = d.loneliness.clamp(-0.15, 0.0);
    d
}

/// Pure: decide an immediate (transient) expression for THIS turn based on the
/// user's text and the planner's intent goal. Returns a Haru expression id
/// (f00..f07) when a strong signal is detected, or None to keep the accumulated
/// mood label. No LLM call, no DB access (architecture principle #8).
///
/// Priority (first match wins, no stacking):
///   1. Anxiety in the user's text      -> "f04" (Haru worried)
///   2. Good news OR intent "celebrate" -> "f00" (Haru happy)
///   3. Otherwise                       -> None (keep accumulated moodLabel)
pub fn transient_expression(text: &str, intent_goal: &str) -> Option<&'static str> {
    if crate::mind::planner::is_anxiety_expression(text) {
        return Some("f04");
    }
    if crate::mind::planner::is_good_news(text) || intent_goal == "celebrate" {
        return Some("f00");
    }
    None
}

/// Pure: rule-based welcome-back line for the fallback path — used when the
/// LLM is unconfigured or `generate_welcome_back` returned nothing (Principle 8:
/// pure rules, no LLM). Picks by mood bucket and absence length. Mirrors the
/// LLM path's contract: a short, natural, emotional greeting (never a system
/// message).
pub fn welcome_back_canned(mood: f64, away_secs: u64) -> &'static str {
    let long = away_secs >= 3600; // >= 1 hour
    if mood >= 0.65 {
        if long {
            "想你好久啦~你终于回来了！"
        } else {
            "你回来啦~刚刚去哪了呀？"
        }
    } else if mood <= 0.35 {
        if long {
            "……你回来了。我还以为你不来了。"
        } else {
            "嗯……你回来了。"
        }
    } else if long {
        "你回来啦~这一阵子都在忙吗？"
    } else {
        "你回来啦~"
    }
}

/// Rule-based fallback for the loneliness-driven nudge when the LLM is
/// unconfigured or returns nothing (Architecture Principle 8: degrade
/// gracefully). Picks by mood bucket. Mirrors the LLM path's contract: a
/// short, gentle reach-out — never clingy, never a question that demands an
/// answer (respects Liri's non-dependent nature).
pub fn lonely_canned(mood: f64) -> &'static str {
    if mood >= 0.65 {
        "嘿~你还在呀，真好。"
    } else if mood <= 0.35 {
        "……你也在呢吧。"
    } else {
        "突然想跟你说说话~"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_news_lifts_mood() {
        // Real Chinese: 通过 / 开心 are GOOD_NEWS_KEYWORDS.
        let d = react_to_turn("我考试通过了！好开心啊", "converse");
        assert!(
            d.mood > 0.0,
            "good news should raise mood, got mood={}",
            d.mood
        );
        assert!(d.physical_energy >= 0.0, "good news is energizing");
        assert_eq!(d.stress, 0.0, "good news alone shouldn't raise stress");
    }

    #[test]
    fn anxiety_raises_stress_and_drops_mood() {
        // Real Chinese: 焦虑 / 压力 are ANXIETY_KEYWORDS.
        let d = react_to_turn("我好焦虑，压力好大", "converse");
        assert!(d.stress > 0.0, "anxiety should raise stress, got {}", d.stress);
        assert!(d.mood < 0.0, "anxiety should drop mood, got {}", d.mood);
    }

    #[test]
    fn neutral_turn_only_drains_social_and_loneliness() {
        // No keywords -> baseline only: social down, loneliness down, rest zero.
        let d = react_to_turn("今天天气不错", "converse");
        assert!(d.social_battery < 0.0, "any chat drains social a little");
        assert!(d.loneliness < 0.0, "company eases loneliness");
        assert_eq!(d.mood, 0.0, "neutral turn shouldn't move mood");
        assert_eq!(d.stress, 0.0, "neutral turn shouldn't move stress");
        assert_eq!(d.physical_energy, 0.0, "neutral turn shouldn't move energy");
    }

    #[test]
    fn fields_stay_within_clamp_range() {
        // Stack every positive signal and confirm clamps hold.
        let d = react_to_turn("通过了！好开心！太棒了！赢了！", "celebrate");
        assert!(d.mood <= 0.15, "mood clamp violated: {}", d.mood);
        assert!(d.physical_energy <= 0.10, "energy clamp violated: {}", d.physical_energy);
        // Baseline drain + care overlay shouldn't exceed the social floor.
        let dc = react_to_turn("wo hen dan xin ni", "care");
        assert!(dc.social_battery >= -0.10, "social clamp violated: {}", dc.social_battery);
        // Loneliness can only go down.
        assert!(dc.loneliness <= 0.0 && dc.loneliness >= -0.15, "loneliness clamp violated: {}", dc.loneliness);
    }

    #[test]
    fn welcome_canned_high_mood_long_absence_misses_user() {
        // High mood + >=1h away → "想你好久啦" (feels the long absence).
        let s = welcome_back_canned(0.8, 7200);
        assert!(s.contains("想你好久"), "long happy absence should miss user, got {}", s);
    }

    #[test]
    fn welcome_canned_low_mood_is_subdued() {
        // Low mood → subdued, quiet greeting (not bouncy).
        let s = welcome_back_canned(0.2, 600);
        assert!(s.contains("……"), "low mood welcome should be subdued, got {}", s);
    }

    #[test]
    fn welcome_canned_neutral_short_is_plain() {
        // Neutral mood, short absence → plain "你回来啦~".
        let s = welcome_back_canned(0.5, 400);
        assert!(s.contains("你回来啦"), "neutral short welcome, got {}", s);
    }

    #[test]
    fn welcome_canned_long_vs_short_differ() {
        // Same mood, different absence → different lines.
        assert_ne!(
            welcome_back_canned(0.5, 400),
            welcome_back_canned(0.5, 7200),
            "long and short absences should pick different lines"
        );
    }
}
