//! Grounded Generation: builds the system prompt with memory constraints
//! and formats retrieved memories with confidence/source annotations.
//! Design doc 5.10: LLM may only reference retrieved memories; must say
//! "not sure" rather than fabricate when relevant memory is absent.

use crate::db::persona as db_persona;
use crate::db::onboarding::UserProfile;
use crate::db::relationship as db_relationship;
use crate::emotion::state::EmotionState;
use crate::mind::retrieval::RetrievalResult;
use crate::mind::planner::Intent;

/// Static personality template loaded at compile time.
const SYSTEM_TEMPLATE: &str = include_str!("../../resources/prompts/system.txt");

/// Builds the full system prompt that constrains the LLM to grounded memory.
///
/// Structure:
///   1. Role / persona description (from persona_traits + relationship)
///   2. Memory constraint instructions (the grounding guardrail)
///   3. Emotion snapshot (how the pet feels right now)
///   4. Intent from the Planner
///   5. Retrieved memories (facts + episodes), each with confidence/source
/// [Current time] section (Phase 6): injected so the LLM always knows the time
/// without a tool round. "几点" is answered directly from the prompt; get_time
/// stays as a runtime smoke test of the agent loop. Uses local time + the
/// perception time-of-day bucket so it matches the rest of the system.
fn current_time_section() -> String {
    use chrono::Datelike;
    use crate::perception::time::{current_time_of_day, TimeOfDay};
    let now = chrono::Local::now();
    let weekday_cn = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [now.weekday().num_days_from_monday() as usize];
    let tod_cn = match current_time_of_day() {
        TimeOfDay::Morning => "上午",
        TimeOfDay::Afternoon => "下午",
        TimeOfDay::Evening => "晚上",
        TimeOfDay::LateNight => "深夜",
        TimeOfDay::DeepNight => "凌晨",
    };
    format!(
        "[Current time]\n现在 {} {} {}\n时段：{}",
        now.format("%H:%M"),
        weekday_cn,
        now.format("%Y-%m-%d"),
        tod_cn,
    )
}

pub fn build_system_prompt(
    retrieval: &RetrievalResult,
    emotion: &EmotionState,
    intent: &Intent,
) -> String {
    let mut sections = vec![SYSTEM_TEMPLATE.to_string(), current_time_section()];

   // 1. Persona + relationship
   sections.push(format_persona(
       &retrieval.persona_traits,
       &retrieval.relationship,
       &retrieval.user_profile,
   ));

    // 2. Grounding constraint
    sections.push(MEMORY_CONSTRAINT.to_string());

    // 3. Emotion
    sections.push(format_emotion(emotion));

    // 4. Intent
    sections.push(format_intent(intent));

    // 5. Retrieved memories
    let memories = format_memories(retrieval);
    if !memories.is_empty() {
        sections.push(memories);
    }

    sections.join("\n\n")
}

/// The direct-answer directive injected for question routes. Written in
/// Chinese because the pet replies in Chinese and the directive must steer
/// the generation language directly (same rationale as the bilingual
/// anti-fabrication rule).
const QA_MODE_PROMPT: &str = "\
[Direct-Answer Mode]
用户这次问的是知识、技术或事实类问题，ta 想要的是答案，不是闲聊。直接、准确、简短地回答，\
像朋友随口解释一样自然，不要上课、不要绕圈子。不要引用记忆，不要追问，不要往自己或宠物相关话题上联想。\
不要假装记得用户说过什么，不要编造用户的过去、偏好或经历——你不知道的事就别说。\
通常一两句话就够，除非问题本身确实需要展开。不确定就老实说不知道。";

/// Builds the system prompt for direct-answer (question) mode.
///
/// Same persona/emotion scaffold as the normal prompt, but deliberately
/// WITHOUT the retrieved memories section and grounding constraint, so the
/// model cannot be steered into hard-associating an unrelated knowledge
/// question with past pet topics (the "你是不是指我的背带" failure mode).
/// Memory focus is stripped from the intent for the same reason.
pub fn build_qa_system_prompt(
    retrieval: &RetrievalResult,
    emotion: &EmotionState,
    intent: &Intent,
) -> String {
    let mut sections = vec![SYSTEM_TEMPLATE.to_string(), current_time_section()];

    sections.push(format_persona(
        &retrieval.persona_traits,
        &retrieval.relationship,
        &retrieval.user_profile,
    ));

    sections.push(format_emotion(emotion));

    let mut qa_intent = intent.clone();
    qa_intent.goal = "converse".to_string();
    qa_intent.memory_anchor.clear();
    sections.push(format_intent(&qa_intent));

    sections.push(QA_MODE_PROMPT.to_string());

    sections.join("\n\n")
}

/// The grounding guardrail text injected into every system prompt.
const MEMORY_CONSTRAINT: &str = "\
[Grounding Constraint]
The following memories are what you actually retrieved. You may respond based on \
these memories about the user. If you have no relevant memory for something, \
say you are not sure rather than fabricating. Each memory below is annotated \
with its confidence level and source date. Do not present information as \
remembered unless it appears in the memories section below.";

/// Formats the persona description from traits + relationship snapshot.
fn format_persona(
    traits: &[db_persona::PersonaTrait],
    relationship: &Option<db_relationship::Relationship>,
    user_profile: &UserProfile,
) -> String {
    let mut lines = vec!["[Persona]".to_string()];

    // Core traits
    let core_traits: Vec<&str> = traits
        .iter()
        .filter(|t| t.trait_type == "core")
        .map(|t| t.trait_key.as_str())
        .collect();
    if !core_traits.is_empty() {
        lines.push(format!("Core personality: {}", core_traits.join(", ")));
    }

    // Adaptive traits
    let adaptive_traits: Vec<&str> = traits
        .iter()
        .filter(|t| t.trait_type == "adaptive")
        .map(|t| t.trait_key.as_str())
        .collect();
    if !adaptive_traits.is_empty() {
        lines.push(format!("Adaptive traits: {}", adaptive_traits.join(", ")));
    }

    // Relationship snapshot
    if let Some(rel) = relationship {
        lines.push(format!(
            "Relationship: closeness {}/100, trust {:.1}/100, known {} days, {} conversations",
            rel.closeness as i32,
            rel.trust,
            rel.days_known,
            rel.total_conversations,
       ));
   }

    // Onboarding profile (user-chosen at first launch): nickname, pet name,
    // personality, relationship style. Primary identity signals for the LLM.
    if let Some(nn) = &user_profile.user_nickname {
        lines.push(format!("用户的称呼: {}", nn));
    }
    if let Some(pn) = &user_profile.pet_name {
        lines.push(format!("你的名字: {}", pn));
    }
    if let Some(ps) = &user_profile.personality_style {
        lines.push(format!("你被期望的性格: {}", ps));
    }
    if let Some(rs) = &user_profile.relationship_style {
        lines.push(format!("与用户的关系设定: {}", rs));
    }

    if lines.len() == 1 {
        // No traits or relationship data yet
        lines.push("A warm, gentle desktop companion who cares about the user.".to_string());
    }

    lines.join("\n")
}

/// Formats the emotion state as a concise snapshot.
fn format_emotion(emotion: &EmotionState) -> String {
    let label = crate::emotion::state::derive_mood_label(emotion);
    format!(
        "[Current Mood] {} (mood {:.1}, energy {:.1}, social {:.1}, stress {:.1})",
        label,
        emotion.mood,
        emotion.physical_energy,
        emotion.social_battery,
        emotion.stress,
    )
}

/// Formats the Planner's intent as a directive.
fn format_intent(intent: &Intent) -> String {
    let mut s = format!(
        "[Intent] goal: {}",
        if intent.goal.is_empty() { "converse naturally".to_string() } else { intent.goal.clone() }
    );
    if !intent.memory_anchor.is_empty() {
        s.push_str(&format!("\nmemory focus: {}", intent.memory_anchor));
    }
    if !intent.tone.is_empty() {
        s.push_str(&format!("\ntone: {}", intent.tone));
    }
    if intent.proactive {
        s.push_str("\n(be proactive: bring up the memory naturally)");
    }
    if intent.goal == "engage" {
        s.push_str("\n(engage: react specifically to what they just shared — prove you listened with something concrete. You may ask ONE genuine follow-up if you're actually curious, but often a single heartfelt line with no question is more natural. Never ask a generic '怎么样'.)");
    } else if intent.goal == "react" {
        s.push_str("\n(react: react warmly and specifically to what they just shared, but do NOT ask any question this turn — just be present and show you listened)");
    }
    s
}

/// Formats all retrieved memories (facts + episodes) with annotations.
///
/// Hermes-inspired split ledger: landmark episodes (关系账——relationship
/// milestones like "user accepted the job offer") get their own [Milestones]
/// section ahead of ordinary memories, so the pet treats them as
/// relationship anchors rather than one more fact. Regular episodes and
/// facts stay under [Memories].
fn format_memories(retrieval: &RetrievalResult) -> String {
    let mut sections = Vec::new();

    // Landmark episodes = relationship ledger (milestones worth anchoring on).
    let milestones: Vec<&crate::db::episodes::Episode> = retrieval
        .episodes
        .iter()
        .map(|se| &se.episode)
        .filter(|e| e.is_landmark)
        .collect();
    if !milestones.is_empty() {
        let mut lines = vec!["[Milestones]".to_string()];
        for ep in milestones {
            let date = ep.time.split('T').next().unwrap_or("?");
            lines.push(format!(
                "- {} (emotion: {}, date: {})",
                ep.summary,
                ep.emotion.as_deref().unwrap_or("neutral"),
                date,
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Latest relationship review — always-on relationship context (the pet's
    // synthesized understanding of where the relationship stands), independent
    // of what the current topic retrieved. Placed before [Memories] so it acts
    // as a relationship anchor rather than one more fact.
    if let Some(review) = &retrieval.relationship_review {
        sections.push(format!("[Relationship]\n{}", review));
    }

    let mut lines = vec!["[Memories]".to_string()];

    // Facts sorted by confidence (already done in retrieval, but ensure here)
    let mut sorted_facts = retrieval.facts.clone();
    sorted_facts.sort_by(|a, b| {
        b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
    });

    for fact in &sorted_facts {
        let date = fact.created_at.split('T').next().unwrap_or("?");
        lines.push(format!(
            "- [Fact] {} / {}: {} (confidence: {}, source: {})",
            fact.category,
            fact.key,
            fact.value,
            confidence_label(fact.confidence),
            date,
        ));
    }

    // Episodes sorted by score (already done in retrieval); landmarks already
    // surfaced above in [Milestones], skip them here to avoid duplication.
    for scored_ep in retrieval
        .episodes
        .iter()
        .filter(|se| !se.episode.is_landmark)
    {
        let ep = &scored_ep.episode;
        let date = ep.time.split('T').next().unwrap_or("?");
        lines.push(format!(
            "- [Episode] {} (importance: {}, emotion: {}, source: {})",
            ep.summary,
            importance_label(ep.importance),
            ep.emotion.as_deref().unwrap_or("neutral"),
            date,
        ));
    }

    if lines.len() == 1 && sections.is_empty() {
        // Explicit empty marker. Without it, no [Memories] section appears in
        // the prompt at all, so the model can't tell memory is empty and
        // fabricates "你上次说…" threads from the personality examples — the
        // measured hallucination root cause. An inline marker is the active
        // signal the model actually reads; a buried rule in the system prompt
        // does not reliably reach it (verified across 4 test rounds).
        return "[Memories]\n（暂无相关记忆——不要提及或编造任何过往，只就当下回应用户。）".to_string();
    }

    if lines.len() > 1 {
        // Inline guard, placed at the point of use (not 1700 tokens away in
        // the system prompt): the model may ONLY cite what is listed above.
        // Targets the G6 failure mode — wrapping a real memory in a fake
        // "你上次说/提过" source, or inventing extra topics — which a distant
        // rule does not stop under thinking-off, but an adjacent note does.
        lines.push(
            "（以上即全部记忆。只可引用已列出的内容；不得添加未列出的项目、人名、事件，"
                .to_string()
                + "也不得编造\"你上次说/提过/念叨\"之类的出处——记着就是记着，没有出处别硬安一个。）",
        );
        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

/// Maps a numeric confidence to a human-readable label.
fn confidence_label(confidence: f64) -> &'static str {
    if confidence >= 0.8 {
        "high"
    } else if confidence >= 0.5 {
        "medium"
    } else {
        "low"
    }
}

/// Maps a numeric importance to a human-readable label.
fn importance_label(importance: f64) -> &'static str {
    if importance >= 0.7 {
        "high"
    } else if importance >= 0.4 {
        "medium"
    } else {
        "low"
    }
}

/// Lightweight check for potential hallucination.
/// Scans the LLM response for assertion patterns about the user and checks
/// whether any provided memory supports them. Returns a list of ungrounded
/// references.
///
/// This is intentionally conservative and simple (no LLM post-processing per
/// architecture principle #8). It flags references that look like specific
/// facts ("you said X", "you like Y") but don't match any provided memory.
pub fn check_groundedness(
    response: &str,
    retrieval: &RetrievalResult,
) -> Vec<String> {
    let mut violations = Vec::new();

    // Gather all values from provided facts for matching.
    let fact_values: Vec<&str> = retrieval
        .facts
        .iter()
        .map(|f| f.value.as_str())
        .collect();

    let ep_summaries: Vec<&str> = retrieval
        .episodes
        .iter()
        .map(|e| e.episode.summary.as_str())
        .collect();

    // Claim patterns: phrases that assert something about the user's past or
    // preferences. If the response makes such a claim but no provided memory
    // overlaps the claim window, flag it. English + Chinese — Liri replies in
    // Chinese, so the EN-only set caught nothing. Patterns are kept
    // high-precision (a generic "你的" would flag every normal sentence).
    let claim_patterns = [
        "you said",
        "you mentioned",
        "you told",
        "you like",
        "you prefer",
        "you have",
        "your ",
        // Chinese: assert a prior statement or a stable preference.
        "你说过",
        "你之前说",
        "你之前提到",
        "你之前提过",
        "你不是说",
        "你告诉过我",
        "你跟我说过",
        "你最喜欢",
        "你最爱的",
        "你一直很喜欢",
    ];
    let lower = response.to_lowercase();

    for pattern in &claim_patterns {
        if let Some(pos) = lower.find(pattern) {
            // Extract a window after the claim pattern. The end offset is in
            // BYTES; +40 into Chinese text can land inside a multi-byte CJK
            // code point, and slicing there would panic — step up to the next
            // char boundary (ceil_char_boundary).
            let target = (pos + pattern.len() + 40).min(response.len());
            let window_end = ceil_char_boundary(response, target);
            let window = &response[pos..window_end];
            let window_lower = window.to_lowercase();

            // Check if any fact value or episode summary overlaps.
            let grounded = fact_values
                .iter()
                .any(|v| window_lower.contains(&v.to_lowercase()))
                || ep_summaries
                    .iter()
                    .any(|s| window_lower.contains(&s.to_lowercase()));

            if !grounded {
                violations.push(format!(
                    "Possible hallucination: '{}' references something not in provided memories",
                    window.trim()
                ));
            }
        }
    }

    if !violations.is_empty() {
        log::warn!(
            "Grounding check found {} potential violations: {:?}",
            violations.len(),
            violations
        );
    }

    violations
}

/// Smallest char-boundary byte offset `>= target`, clamped to the string
/// length. `target` may fall inside a multi-byte (CJK) code point; slicing the
/// string there panics, so advance to the next boundary. Mirrors the
/// nightly `str::ceil_char_boundary` so we stay on stable Rust.
fn ceil_char_boundary(s: &str, target: usize) -> usize {
    if target >= s.len() {
        return s.len();
    }
    let mut i = target;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::episodes::Episode;
    use crate::db::facts::Fact;
    use crate::db::persona::PersonaTrait;
    use crate::db::relationship::Relationship;
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

   fn retrieval_with_data() -> RetrievalResult {
        RetrievalResult {
            episodes: vec![ScoredEpisode {
                episode: Episode {
                    emotion_anchor: None,
                    id: "ep_1".to_string(),
                    time: "2026-07-10T14:00:00+00:00".to_string(),
                    summary: "user ate hotpot with friends".to_string(),
                    emotion: Some("happy".to_string()),
                    importance: 0.8,
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
                score: 0.85,
                score_breakdown:                     ScoreBreakdown {
                    semantic: 0.9,
                    strength: 0.7,
                    novelty: 0.0,
                    recency: 0.95,
                    emotion: 1.0,
                },
            }],
            facts: vec![Fact {
                id: "f_1".to_string(),
                category: "preference".to_string(),
                key: "drink".to_string(),
                value: "milk tea".to_string(),
                confidence: 0.9,
                valid_from: Some("2026-07-14".to_string()),
                valid_to: None,
                source_episode: None,
                mention_count: 3,
                created_at: "2026-07-14T10:00:00+00:00".to_string(),
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }],
            relationship: Some(Relationship {
                closeness: 35.0,
                trust: 60.0,
                days_known: 7,
                total_conversations: 20,
                shared_events: 3,
                last_interaction_at: None,
                last_interaction_type: None,
                closeness_log: None,
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }),
            relationship_review: None,
            persona_traits: vec![PersonaTrait {
                id: "t_1".to_string(),
                trait_type: "core".to_string(),
                trait_key: "gentle".to_string(),
                confidence: 0.95,
               source: "design".to_string(),
               created_at: "2026-07-14T10:00:00+00:00".to_string(),
               updated_at: "2026-07-14T10:00:00+00:00".to_string(),
           }],
           user_profile: UserProfile::default(),
       }
   }

   #[test]
    fn test_relationship_review_injected() {
        // A populated relationship_review must surface as a [Relationship]
        // block in the system prompt (always-on relationship context).
        let mut r = empty_retrieval();
        r.relationship_review = Some("你们最近聊到了实习的事，相处轻松".to_string());
        let prompt = build_system_prompt(&r, &EmotionState::default(), &Intent::default());
        assert!(
            prompt.contains("[Relationship]\n你们最近聊到了实习的事，相处轻松"),
            "relationship review should be injected as a [Relationship] block"
        );
    }

   #[test]
    fn test_system_prompt_contains_constraint() {
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Grounding Constraint]"));
        assert!(prompt.contains("Do not present information as remembered unless"));
    }

    #[test]
    fn test_system_prompt_contains_chinese_grounding_ban() {
        // The anti-fabrication rule must also reach Chinese generation (the pet
        // replies in Chinese), so the ban is bilingual. Guards against the
        // proactive-bubble hallucination regression (e.g. inventing a project).
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("严禁编造"), "Chinese anti-fabrication ban must appear in system prompt");
    }

    #[test]
    fn test_system_prompt_contains_memories() {
        let retrieval = retrieval_with_data();
        let prompt = build_system_prompt(&retrieval, &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("milk tea"));
        assert!(prompt.contains("hotpot"));
        assert!(prompt.contains("gentle"));
    }

    #[test]
    fn test_system_prompt_contains_intent() {
        let intent = Intent {
            goal: "comfort".to_string(),
            memory_anchor: "user has exam tomorrow".to_string(),
            tone: "gentle".to_string(),
            proactive: true,
            action: "normal".to_string(),
            capability: crate::tools::CapabilityMode::None,
        };
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &intent);
        assert!(prompt.contains("goal: comfort"));
        assert!(prompt.contains("exam tomorrow"));
        assert!(prompt.contains("proactive"));
    }

    #[test]
    fn test_system_prompt_injects_current_time() {
        // Phase 6: time is prompt-injected so "几点" is answered without a tool.
        let prompt =
            build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Current time]"));
        assert!(prompt.contains("时段"));
    }

    #[test]
    fn test_qa_prompt_injects_current_time() {
        let prompt =
            build_qa_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Current time]"));
    }

    #[test]
    fn test_confidence_labels() {
        assert_eq!(confidence_label(0.9), "high");
        assert_eq!(confidence_label(0.6), "medium");
        assert_eq!(confidence_label(0.3), "low");
    }

    #[test]
    fn test_groundedness_clean_response() {
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("That sounds fun! Hope you had a great time.", &retrieval);
        assert!(violations.is_empty(), "no claims about user memory: {:?}", violations);
    }

    #[test]
    fn test_groundedness_grounded_claim() {
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("You like milk tea right? Want to get some?", &retrieval);
        assert!(violations.is_empty(), "milk tea is in provided memories: {:?}", violations);
    }

    #[test]
    fn test_groundedness_hallucination() {
        let retrieval = retrieval_with_data();
        let violations = check_groundedness(
            "You said you love hiking mountains every weekend!",
            &retrieval,
        );
        assert!(!violations.is_empty(), "hiking is NOT in provided memories");
    }

    #[test]
    fn test_groundedness_chinese_hallucination() {
        // Liri replies in Chinese — the EN-only patterns caught nothing.
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("你说过你每个周末都去爬山，对吧？", &retrieval);
        assert!(
            !violations.is_empty(),
            "爬山/hiking is NOT in provided memories"
        );
    }

    #[test]
    fn test_groundedness_chinese_grounded() {
        // A Chinese claim that IS backed by a provided memory must pass.
        let mut retrieval = empty_retrieval();
        retrieval.facts.push(Fact {
            id: "f_cn".to_string(),
            category: "preference".to_string(),
            key: "drink".to_string(),
            value: "奶茶".to_string(),
            confidence: 0.9,
            valid_from: None,
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: "2026-08-08T00:00:00+00:00".to_string(),
            updated_at: "2026-08-08T00:00:00+00:00".to_string(),
        });
        let violations = check_groundedness("你最喜欢奶茶对吧，给你带了一杯。", &retrieval);
        assert!(
            violations.is_empty(),
            "奶茶 IS in provided memories: {:?}",
            violations
        );
    }

    #[test]
    fn test_groundedness_cjk_window_does_not_panic() {
        // A long Chinese response where the claim sits deep in multi-byte text:
        // the +40-byte window end must round up to a char boundary, not panic.
        let retrieval = empty_retrieval();
        let padding = "今天天气真不错呀".repeat(20);
        let response = format!("{padding}你说过你从小就住在月球上呢。");
        let violations = check_groundedness(&response, &retrieval);
        assert!(!violations.is_empty(), "月球 is not in memories");
    }

    #[test]
    fn test_milestones_split_ledger() {
        // Hermes-inspired relationship ledger: landmark episodes surface in
        // [Milestones] and are NOT repeated under [Memories].
        let mut retrieval = retrieval_with_data();
        retrieval.episodes[0].episode.is_landmark = true;
        let prompt = build_system_prompt(&retrieval, &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Milestones]"), "landmark must surface in [Milestones]");
        assert!(prompt.contains("hotpot"), "milestone summary present");
        // rfind: system.txt 正文也提到这两个标签，真正的区块在 prompt 末尾。
        let m_start = prompt.rfind("[Milestones]").unwrap();
        let m_end = prompt.rfind("[Memories]").unwrap();
        let after_milestones = &prompt[m_start..m_end];
        assert!(after_milestones.contains("hotpot"), "milestone text must be inside [Milestones] block");
        let after_memories = &prompt[m_end..];
        assert!(!after_memories.contains("hotpot"), "landmark must not repeat under [Memories]");
    }

    #[test]
    fn test_empty_memories_section() {
        let retrieval = empty_retrieval();
        let prompt = build_system_prompt(&retrieval, &EmotionState::default(), &Intent::default());
        assert!(
            !prompt.contains("- [Fact]"),
            "should list no facts when empty"
        );
        // Empty memory now renders an explicit marker (not omitted) so the
        // model can't fabricate "你上次说…" threads — see format_memories.
        assert!(
            prompt.contains("暂无相关记忆"),
            "empty memory must show the explicit no-fabrication marker"
        );
    }

    #[test]
    fn test_emotion_in_prompt() {
        let emotion = EmotionState {
            mood: 0.8,
            ..EmotionState::default()
        };
        let prompt = build_system_prompt(&empty_retrieval(), &emotion, &Intent::default());
        assert!(prompt.contains("[Current Mood]"));
        assert!(prompt.contains("开心"));
    }
}

