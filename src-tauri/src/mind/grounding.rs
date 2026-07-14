//! Grounded Generation: builds the system prompt with memory constraints
//! and formats retrieved memories with confidence/source annotations.
//! Design doc 5.10: LLM may only reference retrieved memories; must say
//! "not sure" rather than fabricate when relevant memory is absent.

use crate::db::persona as db_persona;
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
pub fn build_system_prompt(
    retrieval: &RetrievalResult,
    emotion: &EmotionState,
    intent: &Intent,
) -> String {
    let mut sections = Vec::new();

    // 0. Base personality template (static rules).
    sections.push(SYSTEM_TEMPLATE.to_string());

    // 1. Persona + relationship
    sections.push(format_persona(&retrieval.persona_traits, &retrieval.relationship));

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
    s
}

/// Formats all retrieved memories (facts + episodes) with annotations.
fn format_memories(retrieval: &RetrievalResult) -> String {
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

    // Episodes sorted by score (already done in retrieval)
    for scored_ep in &retrieval.episodes {
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

    if lines.len() == 1 {
        return String::new(); // No memories
    }

    lines.join("\n")
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

    // Check for claim patterns: "you said...", "you like...", "your..."
    // If response contains assertion about user but no matching memory, flag it.
    let claim_patterns = [
        "you said", "you mentioned", "you told", "you like",
        "you prefer", "you have", "your ",
    ];
    let lower = response.to_lowercase();

    for pattern in &claim_patterns {
        if let Some(pos) = lower.find(pattern) {
            // Extract a window after the claim pattern.
            let window_end = (pos + pattern.len() + 40).min(response.len());
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
            persona_traits: vec![],
        }
    }

    fn retrieval_with_data() -> RetrievalResult {
        RetrievalResult {
            episodes: vec![ScoredEpisode {
                episode: Episode {
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
                score_breakdown: ScoreBreakdown {
                    semantic: 0.9,
                    strength: 0.7,
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
            persona_traits: vec![PersonaTrait {
                id: "t_1".to_string(),
                trait_type: "core".to_string(),
                trait_key: "gentle".to_string(),
                confidence: 0.95,
                source: "design".to_string(),
                created_at: "2026-07-14T10:00:00+00:00".to_string(),
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }],
        }
    }

    #[test]
    fn test_system_prompt_contains_constraint() {
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Grounding Constraint]"));
        assert!(prompt.contains("Do not present information as remembered unless"));
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
        };
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &intent);
        assert!(prompt.contains("goal: comfort"));
        assert!(prompt.contains("exam tomorrow"));
        assert!(prompt.contains("proactive"));
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
    fn test_empty_memories_section() {
        let retrieval = empty_retrieval();
        let prompt = build_system_prompt(&retrieval, &EmotionState::default(), &Intent::default());
        assert!(
            !prompt.contains("[Memories]"),
            "should omit memories section when empty"
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
        assert!(prompt.contains("kai xin"));
    }
}
