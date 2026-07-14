//! Proactive behavior: decides when the pet should initiate a conversation.
//! Design doc 9.2: bubbles at most every 30 minutes; silent during deep focus.

use crate::db::pending::PendingEvent;
use crate::emotion::state::EmotionState;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// Minimum interval between proactive bubbles (30 minutes).
const MIN_BUBBLE_INTERVAL_SECS: i64 = 30 * 60;

/// A proactive action the pet wants to take.
#[derive(Debug, Clone, Serialize)]
pub struct ProactiveAction {
    pub event_id: Option<String>,
    pub action_type: String, // "followup" | "random_chat" | "encourage"
    pub message_hint: String,
}

/// Simplified perception state for proactive decisions.
#[derive(Debug, Clone, Default)]
pub struct PerceptionState {
    pub is_deep_focus: bool,
    pub closeness: f64,
}

/// Decides whether the pet should proactively bubble up.
///
/// Rules (priority order):
///   1. Deep focus → None (don't disturb)
///   2. Too soon after last bubble → None (frequency control)
///   3. Closeness < 20 → None (too early in relationship)
///   4. Due event → followup
///   5. High loneliness → random_chat
pub fn trigger_proactive(
    events: &[PendingEvent],
    emotion: &EmotionState,
    perception: &PerceptionState,
    last_bubble_time: &DateTime<Utc>,
) -> Option<ProactiveAction> {
    // Rule 1: Don't disturb during deep focus.
    if perception.is_deep_focus {
        return None;
    }

    // Rule 2: Frequency control — at least 30 minutes since last bubble.
    let now = Utc::now();
    let elapsed = (now - *last_bubble_time).num_seconds();
    if elapsed < MIN_BUBBLE_INTERVAL_SECS {
        return None;
    }

    // Rule 3: Closeness gate — don't proactively bubble to strangers.
    if perception.closeness < 20.0 {
        return None;
    }

    // Rule 4: Due pending event → follow up.
    if let Some(event) = events.first() {
        return Some(ProactiveAction {
            event_id: Some(event.id.clone()),
            action_type: "followup".to_string(),
            message_hint: event.title.clone(),
        });
    }

    // Rule 5: High loneliness → random chat.
    if emotion.loneliness > 0.7 {
        return Some(ProactiveAction {
            event_id: None,
            action_type: "random_chat".to_string(),
            message_hint: String::new(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pending::PendingEvent;

    fn pending_event(id: &str, title: &str) -> PendingEvent {
        PendingEvent {
            id: id.to_string(),
            title: title.to_string(),
            event_date: "2026-07-15".to_string(),
            remind_date: Some("2026-07-14T08:00:00".to_string()),
            source_episode: None,
            status: "pending".to_string(),
            importance: 0.8,
            followup_count: 0,
            created_at: "2026-07-14T10:00:00".to_string(),
            triggered_at: None,
            resolved_at: None,
        }
    }

    fn calm_emotion() -> EmotionState {
        EmotionState::default()
    }

    fn lonely_emotion() -> EmotionState {
        EmotionState {
            mood: 0.4,
            physical_energy: 0.5,
            social_battery: 0.4,
            stress: 0.3,
            loneliness: 0.75,
            rest_need: 0.2,
        }
    }

    fn close_perception() -> PerceptionState {
        PerceptionState {
            is_deep_focus: false,
            closeness: 35.0,
        }
    }

    #[test]
    fn test_deep_focus_no_bubble() {
        let perception = PerceptionState {
            is_deep_focus: true,
            closeness: 50.0,
        };
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &perception, &last);
        assert!(result.is_none());
    }

    #[test]
    fn test_too_soon_no_bubble() {
        let last = Utc::now() - chrono::Duration::minutes(10);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &close_perception(), &last);
        assert!(result.is_none());
    }

    #[test]
    fn test_low_closeness_no_bubble() {
        let perception = PerceptionState {
            is_deep_focus: false,
            closeness: 10.0,
        };
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &perception, &last);
        assert!(result.is_none());
    }

    #[test]
    fn test_due_event_followup() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview tomorrow")], &calm_emotion(), &close_perception(), &last);
        assert!(result.is_some());
        let action = result.unwrap();
        assert_eq!(action.action_type, "followup");
        assert_eq!(action.message_hint, "interview tomorrow");
    }

    #[test]
    fn test_loneliness_random_chat() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[], &lonely_emotion(), &close_perception(), &last);
        assert!(result.is_some());
        let action = result.unwrap();
        assert_eq!(action.action_type, "random_chat");
    }

    #[test]
    fn test_no_event_no_loneliness_none() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[], &calm_emotion(), &close_perception(), &last);
        assert!(result.is_none());
    }
}
