//! Proactive behavior: decides when the pet should initiate a conversation.
//! Design doc 9.2: bubbles at most every 30 minutes; silent during deep focus.

use crate::db::facts::Fact;
use crate::db::pending::PendingEvent;
use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::emotion::state::EmotionState;
use crate::llm::client::{ChatMessage, LlmClient};
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

/// Generates a proactive bubble by picking a memory anchor — a due pending
/// event first, then an anchorable fact, then a recent episode — and running it
/// through the same retrieval + budget + LLM pipeline as a normal turn, with
/// `proactive = true`. Returns `None` when nothing is worth surfacing (the pet
/// stays silent).
///
/// Backend of the `proactive_bubble` command; extracted so the closed-loop-2
/// path ("she brings up your past plan the next day") is testable without
/// constructing AppState / Tauri State.
///
/// Principle 1 (LLM expresses, Rust maintains state): Rust picks the anchor and
/// assembles the prompt; the LLM only voices it.
/// Principle 8 (Cost): at most one LLM call per invocation.
pub async fn generate(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    wm_context: &[ChatMessage],
) -> Result<Option<String>, String> {
    let now = chrono::Utc::now().to_rfc3339();

    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let pending_due: Vec<PendingEvent> =
        db.with_conn(|conn| crate::db::pending::get_due(conn, &now))?;

    let retrieval = crate::mind::retrieval::retrieve(
        "user's life recent events preferences",
        &emotion,
        embedding,
        db,
        3,
    )?;

    let (memory_anchor, goal, tone): (String, &'static str, &'static str) =
        if let Some(ev) = pending_due.first() {
            (ev.title.clone(), "care", "gentle")
        // Only durable, anchorable facts make good proactive-bubble material;
        // pseudo-facts (questions phrased as facts) are excluded.
        } else if let Some(f) = retrieval.facts.iter().find(|f| is_anchorable_fact(f)) {
            (format!("{}: {}", f.key, f.value), "accompany", "playful")
        } else if let Some(ep) = retrieval.episodes.first() {
            (ep.episode.summary.clone(), "accompany", "gentle")
        } else {
            log::info!("proactive_bubble: no usable memory, staying silent");
            return Ok(None);
        };

    let intent = crate::mind::planner::Intent {
        goal: goal.to_string(),
        memory_anchor: memory_anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "proactive_check".to_string(),
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: format!(
            "（你刚刚突然想起了这件事，想主动跟用户说。提起的记忆：{}。按规则 8/8a/8b 回复。）",
            memory_anchor
        ),
    });

    log::info!(
        "[proactive] anchor={:?} goal={} facts={} episodes={} msgs={}",
        memory_anchor.chars().take(30).collect::<String>(),
        goal,
        retrieval.facts.len(),
        retrieval.episodes.len(),
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096))
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    if let Some(ev) = pending_due.first() {
        let _ = crate::pending::mark_triggered(db, &ev.id);
        let _ = crate::pending::increment_followup(db, &ev.id);
    }
    let _ =
        db.with_conn(|conn| crate::db::relationship::record_interaction(conn, "proactive", &now));

    let reply = chat_result.content.trim().to_string();
    if reply.is_empty() {
        Ok(None)
    } else {
        Ok(Some(reply))
    }
}

/// Whether a fact is worth proactively bringing up. Excludes pseudo-facts
/// (questions the user asked, phrased as facts by an over-eager extractor) and
/// requires reasonable confidence. Durable preferences/relationships/goals pass.
fn is_anchorable_fact(f: &Fact) -> bool {
    if f.confidence < 0.7 {
        return false;
    }
    let bad_key_prefixes = ["knowledge_", "belief_", "chemistry_", "geography_"];
    if bad_key_prefixes.iter().any(|p| f.key.starts_with(p)) {
        return false;
    }
    let v = f.value.to_lowercase();
    let bad_value_markers = [
        "user asked",
        "user is asking",
        "curious about user",
        "asking about",
        "does not know",
        "user doesn't know",
        "user is busy",
    ];
    if bad_value_markers.iter().any(|m| v.contains(m)) {
        return false;
    }
    true
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
