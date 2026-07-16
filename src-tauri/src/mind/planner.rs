//! Behavior Planner: reads Brain State and produces an Intent.
//! Design doc 5.5: the planner directs, the LLM acts. The planner never
//! writes dialogue — it writes goal/tone/action/memory_anchor.
//! Rules only (no LLM call) per architecture principle #8.

use crate::db::pending::PendingEvent;
use crate::db::relationship::Relationship;
use crate::emotion::state::EmotionState;
use crate::mind::retrieval::RetrievalResult;

/// The planner's output. Directs how the LLM actor should behave.
#[derive(Debug, Clone)]
pub struct Intent {
    /// Goal: "care" | "listen" | "celebrate" | "accompany" | "converse" | "encourage"
    pub goal: String,
    /// Memory to reference naturally (e.g. "user has exam tomorrow").
    pub memory_anchor: String,
    /// Tone: "gentle" | "quiet" | "excited" | "playful"
    pub tone: String,
    /// Whether the pet proactively initiates (for proactive bubbles).
    pub proactive: bool,
    /// Action: "normal" | "silence" | "proactive_check" | "celebrate"
    pub action: String,
}

impl Default for Intent {
    fn default() -> Self {
        Intent {
            goal: String::new(),
            memory_anchor: String::new(),
            tone: String::new(),
            proactive: false,
            action: "normal".to_string(),
        }
    }
}

/// Anxiety keyword detection (rule-based, no LLM).
const ANXIETY_KEYWORDS: &[&str] = &[
    "worried", "anxious", "nervous", "stressed", "scared", "afraid",
    "panic", "overwhelm", "dan xin", "hai pa", "jing zhang", "ya li",
    "jiao lv", "fan si", "lei", "ku",
];

/// Good news keyword detection.
const GOOD_NEWS_KEYWORDS: &[&str] = &[
    "happy", "great", "amazing", "awesome", "passed", "won",
    "got it", "succeeded", "finished", "done it", "kai xin", "gao xing",
    "tongguo", "cheng gong", "wancheng", "bang", "tai bang",
];

/// Produces an Intent based on the current brain state.
///
/// Rule priority (first match wins):
///   1. Pending event due → proactive_check
///   2. User expressing anxiety + high stress → silence
///   3. User sharing good news + high mood → celebrate
///   4. High loneliness + low closeness → accompany (proactive)
///   5. Default → normal converse
pub fn plan(
    user_text: &str,
    emotion: &EmotionState,
    relationship: Option<&Relationship>,
    pending_due: &[PendingEvent],
    retrieval: &RetrievalResult,
) -> Intent {
    let lower = user_text.to_lowercase();

    // Derive memory anchor from top-scored episode (if score > 0.4).
    let memory_anchor = if !retrieval.episodes.is_empty() {
        let top = &retrieval.episodes[0];
        if top.score > 0.4 {
            top.episode.summary.clone()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Rule 1: Pending event is due → proactive follow-up.
    if let Some(event) = pending_due.first() {
        return Intent {
            goal: "care".to_string(),
            memory_anchor: event.title.clone(),
            tone: "gentle".to_string(),
            proactive: true,
            action: "proactive_check".to_string(),
        };
    }

    // Rule 2: User is anxious and stress is high → listen quietly.
    let user_anxious = ANXIETY_KEYWORDS.iter().any(|kw| lower.contains(kw));
    if user_anxious && emotion.stress > 0.7 {
        return Intent {
            goal: "listen".to_string(),
            memory_anchor,
            tone: "quiet".to_string(),
            proactive: false,
            action: "silence".to_string(),
        };
    }

    // Rule 3: User shares good news and mood is high → celebrate.
    let good_news = GOOD_NEWS_KEYWORDS.iter().any(|kw| lower.contains(kw));
    if good_news && emotion.mood > 0.6 {
        return Intent {
            goal: "celebrate".to_string(),
            memory_anchor,
            tone: "excited".to_string(),
            proactive: false,
            action: "normal".to_string(),
        };
    }

    // Rule 4: High loneliness + relationship allows proactive → accompany.
    if emotion.loneliness > 0.6 {
        if let Some(rel) = relationship {
            if rel.closeness >= 20.0 {
                return Intent {
                    goal: "accompany".to_string(),
                    memory_anchor,
                    tone: "gentle".to_string(),
                    proactive: true,
                    action: "normal".to_string(),
                };
            }
        }
    }

    // Rule 5: Default → normal conversation.
    Intent {
        goal: "converse".to_string(),
        memory_anchor,
        tone: "gentle".to_string(),
        proactive: false,
        action: "normal".to_string(),
    }
}

/// Checks if the user text contains anxiety-related keywords.
pub fn is_anxiety_expression(text: &str) -> bool {
    let lower = text.to_lowercase();
    ANXIETY_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Checks if the user text contains good-news keywords.
pub fn is_good_news(text: &str) -> bool {
    let lower = text.to_lowercase();
    GOOD_NEWS_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::episodes::Episode;
    use crate::mind::retrieval::{RetrievalResult, ScoreBreakdown, ScoredEpisode};

    fn empty_retrieval() -> RetrievalResult {
        RetrievalResult {
            episodes: vec![],
            facts: vec![],
            relationship: None,
            persona_traits: vec![],
        }
    }

    fn retrieval_with_episode(summary: &str, score: f64) -> RetrievalResult {
        RetrievalResult {
            episodes: vec![ScoredEpisode {
                episode: Episode {
                    id: "ep_1".to_string(),
                    time: "2026-07-10T14:00:00+00:00".to_string(),
                    summary: summary.to_string(),
                    emotion: Some("happy".to_string()),
                    importance: 0.7,
                    is_landmark: false,
                    subject: "user".to_string(),
                    participants: None,
                    topics: None,
                    source_type: "conversation".to_string(),
                    source_conversation_id: None,
                    source_turn: None,
                    memory_strength: 0.7,
                    recall_count: 1,
                    last_recalled_at: None,
                    consolidated: false,
                    created_at: "2026-07-10T14:00:00+00:00".to_string(),
                },
                score,
                score_breakdown: ScoreBreakdown {
                    semantic: 0.8,
                    strength: 0.7,
                    recency: 0.9,
                    emotion: 0.5,
                },
            }],
            facts: vec![],
            relationship: None,
            persona_traits: vec![],
        }
    }

    fn calm_emotion() -> EmotionState {
        EmotionState::default()
    }

    fn stressed_emotion() -> EmotionState {
        EmotionState {
            mood: 0.3,
            physical_energy: 0.4,
            social_battery: 0.3,
            stress: 0.8,
            loneliness: 0.0,
            rest_need: 0.5,
        }
    }

    fn happy_emotion() -> EmotionState {
        EmotionState {
            mood: 0.85,
            physical_energy: 0.8,
            social_battery: 0.7,
            stress: 0.1,
            loneliness: 0.0,
            rest_need: 0.0,
        }
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

    fn pending_event(title: &str) -> PendingEvent {
        PendingEvent {
            id: "pe_1".to_string(),
            title: title.to_string(),
            event_date: "2026-07-15".to_string(),
            remind_date: Some("2026-07-15T08:00:00".to_string()),
            source_episode: None,
            status: "pending".to_string(),
            importance: 0.8,
            followup_count: 0,
            created_at: "2026-07-14T10:00:00".to_string(),
            triggered_at: None,
            resolved_at: None,
        }
    }

    #[test]
    fn test_pending_event_proactive_check() {
        let intent = plan(
            "how are you",
            &calm_emotion(),
            None,
            &[pending_event("interview tomorrow")],
            &empty_retrieval(),
        );
        assert_eq!(intent.action, "proactive_check");
        assert!(intent.proactive);
        assert_eq!(intent.goal, "care");
        assert_eq!(intent.memory_anchor, "interview tomorrow");
    }

    #[test]
    fn test_anxiety_silence() {
        let intent = plan(
            "I am so nervous about the exam, I am stressed",
            &stressed_emotion(),
            None,
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.action, "silence");
        assert_eq!(intent.tone, "quiet");
    }

    #[test]
    fn test_anxiety_without_stress_is_normal() {
        // User says anxious things but pet is not stressed → normal response.
        let intent = plan(
            "I am worried about something",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.action, "normal");
    }

    #[test]
    fn test_good_news_celebrate() {
        let intent = plan(
            "I passed the exam! So happy!",
            &happy_emotion(),
            None,
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.goal, "celebrate");
        assert_eq!(intent.tone, "excited");
    }

    #[test]
    fn test_loneliness_accompany() {
        let rel = Relationship {
            closeness: 35.0,
            trust: 50.0,
            days_known: 10,
            total_conversations: 30,
            shared_events: 5,
            last_interaction_at: None,
            last_interaction_type: None,
            closeness_log: None,
            updated_at: "2026-07-14T10:00:00".to_string(),
        };
        let intent = plan(
            "hi",
            &lonely_emotion(),
            Some(&rel),
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.goal, "accompany");
        assert!(intent.proactive);
    }

    #[test]
    fn test_loneliness_low_closeness_no_proactive() {
        // Closeness too low (< 20) → don't proactively reach out.
        let rel = Relationship {
            closeness: 10.0,
            trust: 20.0,
            days_known: 2,
            total_conversations: 3,
            shared_events: 0,
            last_interaction_at: None,
            last_interaction_type: None,
            closeness_log: None,
            updated_at: "2026-07-14T10:00:00".to_string(),
        };
        let intent = plan(
            "hi",
            &lonely_emotion(),
            Some(&rel),
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.goal, "converse");
        assert!(!intent.proactive);
    }

    #[test]
    fn test_default_normal() {
        let intent = plan(
            "I had lunch at the new restaurant",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        );
        assert_eq!(intent.action, "normal");
        assert_eq!(intent.goal, "converse");
        assert_eq!(intent.tone, "gentle");
    }

    #[test]
    fn test_memory_anchor_from_retrieval() {
        let retrieval = retrieval_with_episode("user likes milk tea", 0.7);
        let intent = plan(
            "what should I drink",
            &calm_emotion(),
            None,
            &[],
            &retrieval,
        );
        assert_eq!(intent.memory_anchor, "user likes milk tea");
    }

    #[test]
    fn test_memory_anchor_empty_for_low_score() {
        let retrieval = retrieval_with_episode("user likes milk tea", 0.2);
        let intent = plan(
            "what should I drink",
            &calm_emotion(),
            None,
            &[],
            &retrieval,
        );
        assert!(intent.memory_anchor.is_empty());
    }

    #[test]
    fn test_anxiety_detection() {
        assert!(is_anxiety_expression("I am so worried about this"));
        assert!(is_anxiety_expression("really stressed out"));
        assert!(!is_anxiety_expression("I am happy today"));
    }

    #[test]
    fn test_good_news_detection() {
        assert!(is_good_news("I passed the test!"));
        assert!(is_good_news("We won the game!"));
        assert!(!is_good_news("I failed unfortunately"));
    }
}
