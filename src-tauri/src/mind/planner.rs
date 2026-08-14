//! Behavior Planner: reads Brain State and produces an Intent.
//! Design doc 5.5: the planner directs, the LLM acts. The planner never
//! writes dialogue — it writes goal/tone/action/memory_anchor.
//! Rules only (no LLM call) per architecture principle #8.

use crate::mind::brain_state::BrainState;
use crate::tools::CapabilityMode;
#[cfg(test)]
use crate::db::onboarding::UserProfile;
#[cfg(test)]
use crate::db::pending::PendingEvent;
#[cfg(test)]
use crate::db::relationship::Relationship;
#[cfg(test)]
use crate::emotion::state::EmotionState;

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
    /// Tool capability hint (Phase 4): None = no tools advertised;
    /// ExternalInfo / ComputerAction = a *candidate* — the LLM still decides
    /// whether to call with tool_choice="auto". Pure recall/emotion/chat → None.
    pub capability: CapabilityMode,
}

impl Default for Intent {
    fn default() -> Self {
        Intent {
            goal: String::new(),
            memory_anchor: String::new(),
            tone: String::new(),
            proactive: false,
            action: "normal".to_string(),
            capability: CapabilityMode::None,
        }
    }
}

/// Anxiety keyword detection (rule-based, no LLM).
const ANXIETY_KEYWORDS: &[&str] = &[
    "worried", "anxious", "nervous", "stressed", "scared", "afraid",
    "panic", "overwhelm", "dan xin", "hai pa", "jing zhang", "ya li",
    "jiao lv", "fan si", "lei", "ku",
    "焦虑", "担心", "害怕", "紧张", "压力", "好累", "累了", "哭",
    "烦", "崩溃", "难受", "受不了", "撑不住", "慌", "怕", "心烦",
    "低落", "失落", "沮丧", "emo", "难过", "失眠", "头疼", "丧",
];

/// Good news keyword detection.
const GOOD_NEWS_KEYWORDS: &[&str] = &[
    "happy", "great", "amazing", "awesome", "passed", "won",
    "got it", "succeeded", "finished", "done it", "kai xin", "gao xing",
    "tongguo", "cheng gong", "wancheng", "bang", "tai bang",
    "开心", "高兴", "通过", "成功", "完成", "搞定", "棒", "太棒",
    "厉害", "考过", "终于", "爽", "好消息", "升职", "加薪",
    "第一名", "赢了", "满分", "进步",
];
/// External-information intent keywords (Phase 4 capability prefilter). A hit
/// sets `CapabilityMode::ExternalInfo` as a *candidate* — the LLM still decides
/// whether to actually call search (tool_choice="auto"). Recall contexts like
/// "你还记得那件新闻吗" match "新闻" and set the candidate; the LLM then chooses
/// NOT to search because it is a memory question (黑名单优先).
const EXTERNAL_INFO_KEYWORDS: &[&str] = &[
    "查一下", "查查", "搜一下", "搜索", "搜搜", "帮我查", "查查看",
    "新闻", "天气", "最近有什么", "最新",
    "search", "look up", "news", "weather", "latest",
];

/// Computer-action intent keywords (open app / open url).
const COMPUTER_ACTION_KEYWORDS: &[&str] = &[
    "打开", "启动", "运行", "开一下",
    "open ", "launch", "run ",
];

/// Shared-statement markers: the user is telling us something about
/// themselves, their day, or their preferences. These warrant a genuine
/// follow-up question (the "engage" goal), not a flat acknowledgment.
/// Deliberately EXCLUDES question words so we don't engage-ask when the user
/// actually asked us something.
const SHARE_MARKERS: &[&str] = &[
    "我在", "我最近", "我今天", "我昨天", "我开始", "我打算", "我想",
    "我喜欢", "我爱", "我讨厌", "我有点", "好热", "好冷", "好累",
    "练", "在做", "在看", "在玩", "在忙", "吃了", "喝了",
    "i am", "i'm", "i like", "i love", "i hate", "i started", "i tried",
    "i feel", "i went", "i had", "today i", "lately i",
];

/// Produces an Intent based on the current brain state.
///
/// Rule priority (first match wins):
///   1. Pending event due → proactive_check
///   2. User expressing anxiety → care (comfort, not silence)
///   3. User sharing good news + high mood → celebrate
///   4. High loneliness + low closeness → accompany (proactive)
///   5. Default → normal converse
/// Produces an Intent from the per-turn BrainState snapshot (Architecture #2:
/// one unified handle instead of five loose references). Pure rules, no LLM
/// (Principle #8).
pub fn plan(brain: &BrainState) -> Intent {
    // Bridge the snapshot to local names so the rule body reads unchanged.
    let user_text = brain.text;
    let emotion = brain.emotion;
    let relationship = brain.relationship;
    let pending_due = brain.pending_due;
    let retrieval = brain.retrieval;
    let lower = user_text.to_lowercase();

    // Capability prefilter (Phase 4): keyword hint for whether this turn might
    // benefit from a tool. Computer-action beats external-info ("打开浏览器搜"
    // is an action). This is a CANDIDATE, not a call decision — the LLM decides
    // with tool_choice="auto". Config gating (search_web off → no tools) happens
    // downstream in capability_to_tools, not here.
    let capability = if COMPUTER_ACTION_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        CapabilityMode::ComputerAction
    } else if EXTERNAL_INFO_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        CapabilityMode::ExternalInfo
    } else {
        CapabilityMode::None
    };

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
            capability,
        };
    }

    // Rule 2: User is anxious and stress is high → listen quietly.
    // Rule 2: User is anxious → comfort them (not silence).
    // Silence was decoupled from empathy stress because it created a feedback
    // loop: user anxious → pet absorbs stress → pet hits stress threshold →
    // silence → silence adds more stress. Now anxiety always routes to care so
    // she responds when the user needs her.
    let user_anxious = ANXIETY_KEYWORDS.iter().any(|kw| lower.contains(kw));
    if user_anxious {
        return Intent {
            goal: "care".to_string(),
            memory_anchor,
            tone: "gentle".to_string(),
            proactive: false,
            action: "normal".to_string(),
            capability,
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
            capability,
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
                    capability,
                };
            }
        }
    }

    // Rule 4b: User shared a statement (not a question) → engage with a
    // genuine follow-up question. Detected via SHARE_MARKERS and the absence
    // of a question mark in the user's text.
    let is_question = user_text.contains('？') || user_text.contains('?')
        || user_text.contains("吗") || user_text.contains("呢");
    let shared = SHARE_MARKERS.iter().any(|m| lower.contains(m));
    if shared && !is_question {
        return Intent {
            goal: "engage".to_string(),
            memory_anchor,
            tone: "curious".to_string(),
            proactive: false,
            action: "normal".to_string(),
            capability,
        };
    }

    // Rule 5: Default → normal conversation.
    Intent {
        goal: "converse".to_string(),
        memory_anchor,
        tone: "gentle".to_string(),
        proactive: false,
        action: "normal".to_string(),
        capability,
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
           relationship_review: None,
           persona_traits: vec![],
           user_profile: UserProfile::default(),
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
                score_breakdown:                     ScoreBreakdown {
                    semantic: 0.8,
                    strength: 0.7,
                    novelty: 0.0,
                    recency: 0.9,
                    emotion: 0.5,
                },
           }],
           facts: vec![],
           relationship: None,
           relationship_review: None,
           persona_traits: vec![],
           user_profile: UserProfile::default(),
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

    /// Build a per-turn BrainState snapshot for `plan` (Architecture #2).
    fn brain<'a>(
        text: &'a str,
        emotion: &'a EmotionState,
        relationship: Option<&'a Relationship>,
        pending_due: &'a [PendingEvent],
        retrieval: &'a RetrievalResult,
    ) -> super::BrainState<'a> {
        super::BrainState::new(text, emotion, relationship, pending_due, retrieval)
    }

    #[test]
    fn test_pending_event_proactive_check() {
        let intent = plan(&brain(
            "how are you",
            &calm_emotion(),
            None,
            &[pending_event("interview tomorrow")],
            &empty_retrieval(),
        ));
        assert_eq!(intent.action, "proactive_check");
        assert!(intent.proactive);
        assert_eq!(intent.goal, "care");
        assert_eq!(intent.memory_anchor, "interview tomorrow");
    }

    #[test]
    fn test_anxiety_routes_to_care() {
        // Anxiety now routes to care (comfort) regardless of pet stress.
        // Silence was removed to break the anxiety → stress → silence loop.
        let intent = plan(&brain(
            "I am so nervous about the exam, I am stressed",
            &stressed_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.action, "normal");
        assert_eq!(intent.goal, "care");
        assert_eq!(intent.tone, "gentle");
    }

    #[test]
    fn test_anxiety_without_stress_is_normal() {
        // User says anxious things but pet is not stressed → normal response.
        let intent = plan(&brain(
            "I am worried about something",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.action, "normal");
    }

    #[test]
    fn test_good_news_celebrate() {
        let intent = plan(&brain(
            "I passed the exam! So happy!",
            &happy_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
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
        let intent = plan(&brain(
            "hi",
            &lonely_emotion(),
            Some(&rel),
            &[],
            &empty_retrieval(),
        ));
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
        let intent = plan(&brain(
            "hi",
            &lonely_emotion(),
            Some(&rel),
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.goal, "converse");
        assert!(!intent.proactive);
    }

    #[test]
    fn test_default_normal() {
        let intent = plan(&brain(
            // NOTE: must NOT match any SHARE_MARKER (e.g. "i had") or the
            // planner correctly classifies it as "engage", not "converse".
            // This test verifies the DEFAULT fall-through branch only.
            "The weather looks calm",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.action, "normal");
        assert_eq!(intent.goal, "converse");
        assert_eq!(intent.tone, "gentle");
    }

    #[test]
    fn test_memory_anchor_from_retrieval() {
        let retrieval = retrieval_with_episode("user likes milk tea", 0.7);
        let intent = plan(&brain(
            "what should I drink",
            &calm_emotion(),
            None,
            &[],
            &retrieval,
        ));
        assert_eq!(intent.memory_anchor, "user likes milk tea");
    }

    #[test]
    fn test_memory_anchor_empty_for_low_score() {
        let retrieval = retrieval_with_episode("user likes milk tea", 0.2);
        let intent = plan(&brain(
            "what should I drink",
            &calm_emotion(),
            None,
            &[],
            &retrieval,
        ));
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

    // --- Phase 4: capability prefilter ---

    #[test]
    fn test_capability_external_info_search() {
        let intent = plan(&brain(
            "帮我查一下最近的AI新闻",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::ExternalInfo);
    }

    #[test]
    fn test_capability_computer_action_open() {
        let intent = plan(&brain(
            "打开VSCode",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::ComputerAction);
    }

    #[test]
    fn test_capability_none_for_chitchat() {
        // 哈哈哈哈 — pure chitchat, no tool needed (abstention).
        let intent = plan(&brain(
            "哈哈哈哈",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::None);
    }

    #[test]
    fn test_capability_none_for_anxiety() {
        // Emotion, not a tool need.
        let intent = plan(&brain(
            "我最近好累",
            &stressed_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::None);
    }

    #[test]
    fn test_capability_candidate_for_recall_context() {
        // "你还记得那件新闻吗" matches "新闻" → ExternalInfo CANDIDATE. The
        // planner sets the candidate; the LLM later chooses NOT to search
        // (it's a memory question) — abstention is decided downstream, not here.
        let intent = plan(&brain(
            "你还记得那件新闻吗",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::ExternalInfo);
    }

    #[test]
    fn test_capability_none_for_time_question() {
        // "几点" matches no external-info keyword — time is prompt-injected
        // (Phase 6), never a tool round.
        let intent = plan(&brain(
            "现在几点了",
            &calm_emotion(),
            None,
            &[],
            &empty_retrieval(),
        ));
        assert_eq!(intent.capability, CapabilityMode::None);
    }
}
