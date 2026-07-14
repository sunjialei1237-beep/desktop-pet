//! Prompt Budget: value-density compression for the LLM context window.
//! Design doc 5.10: no module is deleted, each is compressed to fit ~4K tokens.
//!
//! Fixed-priority budget allocation (design doc GPT recommendation):
//!   Current Conversation:  1600 token  (must keep, working memory)
//!   Persona:                 80 token  (must keep)
//!   Emotion:                 25 token  (must keep)
//!   Facts:                  300 token  (compressible, take top confidence)
//!   Episodes:              1200 token  (compressible, summary only)
//!   Intent (Planner):       100 token
//!   System scaffold:        ~300 token (grounding constraint + formatting)
//!   Reserve:                ~341 token (response headroom)

use crate::emotion::state::EmotionState;
use crate::llm::client::ChatMessage;
use crate::mind::retrieval::RetrievalResult;

use crate::mind::planner::Intent;

/// Maximum total input tokens (target budget).
const MAX_TOKENS: usize = 4096;

/// Budget allocations per module (in approximate tokens).
mod budget {
    pub const CONVERSATION: usize = 1600;
    pub const PERSONA: usize = 80;
    pub const EMOTION: usize = 25;
    pub const FACTS: usize = 300;
    pub const EPISODES: usize = 1200;
    pub const INTENT: usize = 100;
    pub const SYSTEM_SCAFFOLD: usize = 300;
}

/// Rough token estimate for mixed Chinese/English text.
/// Chinese characters are ~1 token each; ASCII is ~0.25 tokens per char.
/// We count CJK chars at 1 token and remaining chars at 1/4 token.
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3400}'..='\u{4DBF}').contains(&ch)
            || ('\u{F900}'..='\u{FAFF}').contains(&ch)
        {
            cjk += 1;
        } else {
 other += 1;
        }
    }
    cjk + (other / 4)
}

/// Estimates total tokens across a slice of messages.
pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|m| estimate_tokens(&m.content) + 4) // +4 for role overhead per message
        .sum()
}

/// Allocates and compresses retrieval results + working memory into a
/// token-budgeted messages array ready for the LLM.
///
/// The system message contains: grounding constraint, persona, emotion,
/// intent, and compressed memories (facts + episodes).
/// Following messages are working memory (recent conversation) compressed
/// to fit the conversation budget.
pub fn allocate_and_compress(
    retrieval: &RetrievalResult,
    working_memory: &[ChatMessage],
    emotion: &EmotionState,
    intent: &Intent,
) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    // 1. System message: grounding constraint + persona + emotion + intent + memories
    let system_prompt = crate::mind::grounding::build_system_prompt(retrieval, emotion, intent);

    // Compress memories if they exceed budget.
    let system_prompt = compress_system_prompt(system_prompt, retrieval, emotion, intent);

    messages.push(ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
    });

    // 2. Working memory: recent conversation, truncated from the front.
    let conv_messages = compress_conversation(working_memory, budget::CONVERSATION);
    messages.extend(conv_messages);

    messages
}

/// Compresses the system prompt by trimming memories if they exceed the
/// combined budget for facts + episodes + persona + emotion + intent + scaffold.
fn compress_system_prompt(
    prompt: String,
    retrieval: &RetrievalResult,
    emotion: &EmotionState,
    intent: &Intent,
) -> String {
    let budget = budget::FACTS + budget::EPISODES + budget::PERSONA
        + budget::EMOTION + budget::INTENT + budget::SYSTEM_SCAFFOLD;

    let tokens = estimate_tokens(&prompt);
    if tokens <= budget {
        return prompt;
    }

    // If over budget, rebuild with fewer episodes/facts.
    // Strategy: drop lowest-score episodes first, then lowest-confidence facts.
    log::info!(
        "System prompt {} tokens exceeds budget {}, compressing",
        tokens,
        budget
    );

    // Estimate how many episodes we can keep.
    let episode_tokens: Vec<usize> = retrieval
        .episodes
        .iter()
        .map(|e| estimate_tokens(&e.episode.summary) + 20)
        .collect();

    let fact_tokens: Vec<usize> = retrieval
        .facts
        .iter()
        .map(|f| estimate_tokens(&f.value) + 30)
        .collect();

    let non_memory_tokens = tokens
        - episode_tokens.iter().sum::<usize>()
        - fact_tokens.iter().sum::<usize>();
    let memory_budget = budget.saturating_sub(non_memory_tokens);

    // Greedily fit episodes (highest score first) then facts (highest confidence first).
    let mut used = 0usize;
    let mut keep_episodes = 0usize;
    for (i, &t) in episode_tokens.iter().enumerate() {
        if used + t <= memory_budget {
            used += t;
            keep_episodes = i + 1;
        } else {
            break;
        }
    }
    let remaining = memory_budget.saturating_sub(used);
    let mut keep_facts = 0usize;
    for (i, &t) in fact_tokens.iter().enumerate() {
        if used + t <= memory_budget {
            used += t;
            keep_facts = i + 1;
        } else {
            break;
        }
    }
    let _ = remaining; // suppress unused warning

    // Rebuild with truncated retrieval.
    let truncated_retrieval = RetrievalResult {
        episodes: retrieval.episodes[..keep_episodes.min(retrieval.episodes.len())].to_vec(),
        facts: retrieval.facts[..keep_facts.min(retrieval.facts.len())].to_vec(),
        relationship: retrieval.relationship.clone(),
        persona_traits: retrieval.persona_traits.clone(),
    };

    crate::mind::grounding::build_system_prompt(&truncated_retrieval, emotion, intent)
}

/// Truncates working memory from the front (oldest messages first)
/// to fit within the conversation token budget.
fn compress_conversation(messages: &[ChatMessage], budget_tokens: usize) -> Vec<ChatMessage> {
    let total = estimate_messages_tokens(messages);
    if total <= budget_tokens {
        return messages.to_vec();
    }

 // Drop from the front until we fit.
    let mut start = 0;
    while start < messages.len() {
        let remaining = &messages[start..];
        if estimate_messages_tokens(remaining) <= budget_tokens {
            break;
        }
        start += 1;
    }

    messages[start..].to_vec()
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

    fn retrieval_with_episodes(count: usize) -> RetrievalResult {
        let episodes: Vec<ScoredEpisode> = (0..count)
            .map(|i| ScoredEpisode {
                episode: Episode {
                    id: format!("ep_{}", i),
                    time: "2026-07-10T14:00:00+00:00".to_string(),
                    summary: format!("user did interesting thing number {}", i),
                    emotion: Some("happy".to_string()),
                    importance: 0.5,
                    is_landmark: false,
                    subject: "user".to_string(),
                    participants: None,
                    topics: None,
                    source_type: "conversation".to_string(),
                    source_conversation_id: None,
                    source_turn: None,
                    memory_strength: 0.6,
                    recall_count: 0,
                    last_recalled_at: None,
                    consolidated: false,
                    created_at: "2026-07-10T14:00:00+00:00".to_string(),
                },
                score: 1.0 - (i as f64 * 0.1),
                score_breakdown: ScoreBreakdown {
                    semantic: 0.8,
                    strength: 0.6,
                    recency: 0.9,
                    emotion: 0.5,
                },
            })
            .collect();

        RetrievalResult {
            episodes,
            facts: vec![],
            relationship: None,
            persona_traits: vec![],
        }
    }

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn test_estimate_tokens_ascii() {
        let tokens = estimate_tokens("hello world this is a test");
        // 26 chars / 4 = 6 tokens
        assert_eq!(tokens, 6);
    }

    #[test]
    fn test_estimate_tokens_cjk() {
        // Each CJK char is 1 token
        let tokens = estimate_tokens("nihao shijie");
        assert!(tokens > 0);
    }

    #[test]
    fn test_allocate_basic() {
        let retrieval = empty_retrieval();
        let wm = vec![
            msg("user", "hello"),
            msg("assistant", "hi there!"),
        ];
        let messages = allocate_and_compress(&retrieval, &wm, &EmotionState::default(), &Intent::default());

        // Should have system + 2 conversation messages
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("[Grounding Constraint]"));
    }

    #[test]
    fn test_total_tokens_within_budget() {
        let retrieval = retrieval_with_episodes(5);
        let mut wm = vec![];
        for i in 0..20 {
            wm.push(msg("user", &format!("message number {} with some extra words here", i)));
            wm.push(msg("assistant", &format!("reply number {} with some more words to pad it out", i)));
        }
        let messages = allocate_and_compress(
            &retrieval,
            &wm,
            &EmotionState::default(),
            &Intent::default(),
        );

        let total = estimate_messages_tokens(&messages);
        // Must fit within 4100 (4096 + small tolerance for per-message overhead)
        assert!(
            total <= 4100,
            "total tokens {} exceeds 4100 budget",
            total
        );
    }

    #[test]
    fn test_conversation_truncation() {
        let mut wm = vec![];
        for i in 0..100 {
            wm.push(msg("user", &format!("long message with lots of words number {}", i)));
        }

        let compressed = compress_conversation(&wm, 200);
        let tokens = estimate_messages_tokens(&compressed);
        assert!(tokens <= 200, "compressed {} tokens > 200 budget", tokens);
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_conversation_no_truncation_needed() {
        let wm = vec![msg("user", "hi"), msg("assistant", "hello")];
        let compressed = compress_conversation(&wm, 1600);
        assert_eq!(compressed.len(), 2, "should not truncate when within budget");
    }

    #[test]
    fn test_system_prompt_with_memories() {
        let retrieval = RetrievalResult {
            episodes: vec![],
            facts: vec![Fact {
                id: "f1".to_string(),
                category: "preference".to_string(),
                key: "drink".to_string(),
                value: "milk tea".to_string(),
                confidence: 0.9,
                valid_from: None,
                valid_to: None,
                source_episode: None,
                mention_count: 1,
                created_at: "2026-07-14T10:00:00+00:00".to_string(),
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }],
            relationship: Some(Relationship {
                closeness: 20.0,
                trust: 50.0,
                days_known: 3,
                total_conversations: 10,
                shared_events: 1,
                last_interaction_at: None,
                last_interaction_type: None,
                closeness_log: None,
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }),
            persona_traits: vec![PersonaTrait {
                id: "t1".to_string(),
                trait_type: "core".to_string(),
                trait_key: "cheerful".to_string(),
                confidence: 0.9,
                source: "design".to_string(),
                created_at: "2026-07-14T10:00:00+00:00".to_string(),
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }],
        };

        let messages = allocate_and_compress(
            &retrieval,
            &[],
            &EmotionState::default(),
            &Intent::default(),
        );

        let system = &messages[0].content;
        assert!(system.contains("milk tea"));
        assert!(system.contains("cheerful"));
        assert!(system.contains("closeness 20"));
    }

    #[test]
    fn test_intent_in_system_prompt() {
        let intent = Intent {
            goal: "encourage".to_string(),
            memory_anchor: "user has interview".to_string(),
            tone: "warm".to_string(),
            proactive: true,
            action: "normal".to_string(),
        };
        let messages = allocate_and_compress(
            &empty_retrieval(),
            &[],
            &EmotionState::default(),
            &intent,
        );
        assert!(messages[0].content.contains("encourage"));
        assert!(messages[0].content.contains("interview"));
        assert!(messages[0].content.contains("proactive"));
    }
}
