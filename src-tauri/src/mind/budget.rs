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
#[cfg(test)]
use crate::db::onboarding::UserProfile;

use crate::mind::planner::Intent;

/// Maximum total input tokens (target budget).
#[allow(dead_code)]
const MAX_TOKENS: usize = 4096;

// --- Soul v2 plan L2a: near-end directive switch --------------------------------
// `[prompt] near_end_directive` (config.toml), set once at startup (lib.rs).
// Default ON. OFF = exact v1 message layout — a runtime rollback path that
// needs no rebuild (Architecture #6).
static NEAR_END_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_near_end_enabled(v: bool) {
    NEAR_END_ENABLED.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_near_end_enabled() -> bool {
    NEAR_END_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Budget allocations per module (in approximate tokens).
mod budget {
    pub const CONVERSATION: usize = 1600;
    pub const PERSONA: usize = 80;
    pub const EMOTION: usize = 25;
    pub const FACTS: usize = 300;
    pub const EPISODES: usize = 1200;
    pub const INTENT: usize = 100;
    /// System.txt v2 (Soul 升级 P2)：14 示例 + 认知透镜把模板撑到 ~2300
    /// 内部估算 token（v1 ~1700）。增幅 ≤600 在方案成本上限内；预算不跟上
    /// 会让 compress_system_prompt 误裁 [Memories]（test_system_prompt_with
    /// _memories 曾因此 fail）。
    pub const SYSTEM_SCAFFOLD: usize = 900;
    /// Latest relationship-review summary slot (always-on [Relationship]).
    pub const RELATIONSHIP: usize = 80;
}

/// The system-prompt token budget (facts + episodes + persona + emotion +
/// intent + scaffold). Exposed so the debug panel can show "system prompt N /
/// budget M" next to the actual (post-compression) size — observability for
/// architecture #8 (cost) and #11 (why is the context that big?).
pub fn system_prompt_budget() -> usize {
    budget::FACTS + budget::EPISODES + budget::PERSONA
        + budget::EMOTION + budget::INTENT + budget::SYSTEM_SCAFFOLD
        + budget::RELATIONSHIP
}

/// The QA (direct-answer) system-prompt token budget. Same as the normal budget
/// MINUS the memory slots (facts + episodes): `build_qa_system_prompt` injects
/// no `[Memories]`, so those slots don't apply. Exposed so the debug panel shows
/// the right ceiling for a QA turn ("system N / budget M") instead of the
/// memory-inclusive 2005 — otherwise a 505-token QA prompt looks "under budget"
/// against the wrong number.
pub fn qa_system_prompt_budget() -> usize {
    budget::PERSONA + budget::EMOTION + budget::INTENT + budget::SYSTEM_SCAFFOLD
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
        .map(|m| estimate_tokens(m.content_str()) + 4) // +4 for role overhead per message
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

    messages.push(ChatMessage::system(system_prompt));

    // 2. Working memory: recent conversation, truncated from the front.
    let conv_messages = compress_conversation(working_memory, budget::CONVERSATION);
    messages.extend(conv_messages);

    // 3. Soul v2 plan L2a: near-end directive (time + mood + intent) as a
    // trailing system message after the history — the recency-weighted
    // steering slot (CCv2 post_history_instructions). All callers of this
    // allocator get it uniformly. Off-switch restores the exact v1 layout.
    if is_near_end_enabled() {
        messages.push(ChatMessage::system(
            crate::mind::grounding::build_near_end_directive(emotion, intent),
        ));
    }

    messages
}

/// Allocates messages for direct-answer (question) mode: persona + emotion +
/// intent + QA directive + working memory, WITHOUT retrieved memories, so a
/// knowledge question cannot be steered into hard-associating unrelated
/// pet topics. Working memory is still kept so multi-turn questions flow.
pub fn allocate_qa(
    retrieval: &RetrievalResult,
    working_memory: &[ChatMessage],
    emotion: &EmotionState,
    intent: &Intent,
) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    let system_prompt =
        crate::mind::grounding::build_qa_system_prompt(retrieval, emotion, intent);

    messages.push(ChatMessage::system(system_prompt));

    let conv_messages = compress_conversation(working_memory, budget::CONVERSATION);
    messages.extend(conv_messages);

    // Soul v2 L2a: QA near-end directive (direct-answer + time + mood).
    if is_near_end_enabled() {
        messages.push(ChatMessage::system(
            crate::mind::grounding::build_qa_near_end(emotion, intent),
        ));
    }

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
        + budget::EMOTION + budget::INTENT + budget::SYSTEM_SCAFFOLD
        + budget::RELATIONSHIP;

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
       relationship_review: retrieval.relationship_review.clone(),
       persona_traits: retrieval.persona_traits.clone(),
       user_profile: retrieval.user_profile.clone(),
       first_met: retrieval.first_met.clone(),
   };

    crate::mind::grounding::build_system_prompt(&truncated_retrieval, emotion, intent)
}

/// Truncates working memory to fit within the conversation token budget.
///
/// Hermes-inspired rule (user messages are never compressed away): user
/// messages are collected verbatim first and are always kept; assistant
/// replies are kept only while budget allows, and older assistant replies
/// are evicted first when a user message pushes over budget. This preserves
/// what the USER actually said at the cost of losing the pet's own earlier
/// replies — the pet can still follow the user's thread, and the user's
/// words are never misremembered through truncation.
fn compress_conversation(messages: &[ChatMessage], budget_tokens: usize) -> Vec<ChatMessage> {
    let total = estimate_messages_tokens(messages);
    if total <= budget_tokens {
        return messages.to_vec();
    }

    // Collect from newest to oldest (reversed). User messages always stay;
    // assistant messages fill remaining budget, oldest assistants evicted first.
    let mut kept: Vec<ChatMessage> = Vec::new();
    let mut used = 0usize;
    for m in messages.iter().rev() {
        let t = estimate_tokens(m.content_str()) + 4;
        if m.role == "user" {
            kept.push(m.clone());
            used += t;
            // Evict oldest kept assistant replies until we fit again.
            while used > budget_tokens {
                match kept.iter().rposition(|k| k.role != "user") {
                    Some(pos) => {
                        used -= estimate_tokens(kept[pos].content_str()) + 4;
                        kept.remove(pos);
                    }
                    None => break, // all user messages; keep newest by dropping from front
                }
            }
        } else if used + t <= budget_tokens {
            kept.push(m.clone());
            used += t;
        }
        // assistant message over budget: skip (it is the pet's own words, droppable)
    }

    // If even all user messages over budget, trim the oldest ones (never
    // possible in practice — user messages are short — but guard anyway).
    let mut kept = kept;
    kept.reverse();
    while used > budget_tokens && !kept.is_empty() {
        used -= estimate_tokens(kept[0].content_str()) + 4;
        kept.remove(0);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// The near-end switch is global state; tests that allocate messages and
    /// the test that flips the flag must not run concurrently.
    fn flag_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
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
           first_met: None,
       }
   }

   fn retrieval_with_episodes(count: usize) -> RetrievalResult {
        let episodes: Vec<ScoredEpisode> = (0..count)
            .map(|i| ScoredEpisode {
                episode: Episode {
                    emotion_anchor: None,
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
                score_breakdown:                 ScoreBreakdown {
                    semantic: 0.8,
                    strength: 0.6,
                    novelty: 0.0,
                    recency: 0.9,
                    emotion: 0.5,
                },
            })
            .collect();

       RetrievalResult {
           episodes,
           facts: vec![],
           relationship: None,
           relationship_review: None,
           persona_traits: vec![],
           user_profile: UserProfile::default(),
           first_met: None,
       }
   }

   fn msg(role: &str, content: &str) -> ChatMessage {
        match role {
            "user" => ChatMessage::user(content),
            "assistant" => ChatMessage::assistant(content),
            _ => ChatMessage::system(content),
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
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
        let retrieval = empty_retrieval();
        let wm = vec![
            msg("user", "hello"),
            msg("assistant", "hi there!"),
        ];
        let messages = allocate_and_compress(&retrieval, &wm, &EmotionState::default(), &Intent::default());

        // Soul v2 L2a: system + 2 conversation messages + trailing near-end
        // directive (time/mood/intent) — 4 messages.
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content_str().contains("[Grounding Constraint]"));
        assert_eq!(messages[3].role, "system", "trailing near-end directive");
        assert!(messages[3].content_str().contains("[Current time]"));
    }

    #[test]
    fn test_total_tokens_within_budget() {
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
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
    fn test_conversation_keeps_all_user_messages() {
        // Hermes rule: user messages are never compressed away. Even when the
        // budget is tiny, every user message must survive; only assistant
        // replies get evicted.
        let mut wm = vec![];
        for i in 0..20 {
            wm.push(msg("user", &format!("user message number {}", i)));
            wm.push(msg("assistant", &format!("a longer assistant reply with more words here {}", i)));
        }

        let compressed = compress_conversation(&wm, 300);
        let user_msgs: Vec<&str> = compressed
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content_str())
            .collect();
        assert_eq!(
            user_msgs.len(),
            20,
            "all user messages must survive compression, got {}",
            user_msgs.len()
        );
        // Order preserved, verbatim.
        assert_eq!(user_msgs[0], "user message number 0");
        assert_eq!(user_msgs[19], "user message number 19");
        // Assistant replies were evicted to make room.
        let assistants = compressed.iter().filter(|m| m.role == "assistant").count();
        assert!(assistants < 20, "assistant replies should be evicted first");
    }

    #[test]
    fn test_conversation_no_truncation_needed() {
        let wm = vec![msg("user", "hi"), msg("assistant", "hello")];
        let compressed = compress_conversation(&wm, 1600);
        assert_eq!(compressed.len(), 2, "should not truncate when within budget");
    }

    #[test]
    fn test_system_prompt_with_memories() {
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
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
                surfaced_count: 0,
                last_surfaced_at: None,
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
            relationship_review: None,
           persona_traits: vec![PersonaTrait {
               id: "t1".to_string(),
               trait_type: "core".to_string(),
               trait_key: "cheerful".to_string(),
               confidence: 0.9,
               source: "design".to_string(),
               created_at: "2026-07-14T10:00:00+00:00".to_string(),
               updated_at: "2026-07-14T10:00:00+00:00".to_string(),
           }],
           user_profile: UserProfile::default(),
           first_met: None,
       };

       let messages = allocate_and_compress(
            &retrieval,
            &[],
            &EmotionState::default(),
            &Intent::default(),
        );

        let system = messages[0].content_str();
        assert!(system.contains("milk tea"));
        assert!(system.contains("cheerful"));
        assert!(system.contains("closeness 20"));
    }

    #[test]
    fn test_near_end_switch_restores_v1_layout() {
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
        // Soul v2 L2a rollback path: switch OFF -> exact v1 layout (single
        // system message, no trailing directive).
        set_near_end_enabled(false);
        let messages = allocate_and_compress(
            &empty_retrieval(),
            &[],
            &EmotionState::default(),
            &Intent::default(),
        );
        assert_eq!(messages.len(), 1, "v1 layout: system only, no trailing directive");
        set_near_end_enabled(true);
        let messages = allocate_and_compress(
            &empty_retrieval(),
            &[],
            &EmotionState::default(),
            &Intent::default(),
        );
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, "system");
    }

    #[test]
    fn test_intent_in_system_prompt() {
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
        let intent = Intent {
            goal: "encourage".to_string(),
            memory_anchor: "user has interview".to_string(),
            tone: "warm".to_string(),
            proactive: true,
            action: "normal".to_string(),
            capability: crate::tools::CapabilityMode::None,
        };
        let messages = allocate_and_compress(
            &empty_retrieval(),
            &[],
            &EmotionState::default(),
            &intent,
        );
        // Soul v2 L2a: intent lives in the trailing near-end directive now.
        let near_end = messages.last().unwrap();
        assert_eq!(near_end.role, "system");
        assert!(near_end.content_str().contains("encourage"));
        assert!(near_end.content_str().contains("interview"));
        assert!(near_end.content_str().contains("proactive"));
        assert!(!messages[0].content_str().contains("encourage"), "static system stays intent-free");
    }

    #[test]
    fn test_qa_system_prompt_budget_excludes_memory_slots() {
        // QA mode injects no [Memories], so its budget omits the facts +
        // episodes slots — strictly smaller than the normal budget (not 2005).
        let qa = qa_system_prompt_budget();
        let normal = system_prompt_budget();
        assert!(qa < normal, "QA budget {} should be < normal {}", qa, normal);
        assert_eq!(qa, 80 + 25 + 100 + 900); // PERSONA + EMOTION + INTENT + SCAFFOLD(v2)
    }

    #[test]
    fn test_qa_keeps_identity() {
        let _flag = flag_lock().lock().unwrap_or_else(|e| e.into_inner());
        // QA mode must KEEP identity (persona + user name) so a direct answer
        // still sounds like 璃 and can address the user by name. Locks the fix
        // where qa_mode loads persona/relationship/user_profile instead of a
        // pure default() that stripped them along with the memories.
        let mut r = empty_retrieval();
        r.persona_traits = vec![PersonaTrait {
            id: "p1".to_string(),
            trait_type: "core".to_string(),
            trait_key: "温柔".to_string(),
            confidence: 0.9,
            source: "seed".to_string(),
            created_at: "2026-08-04T00:00:00Z".to_string(),
            updated_at: "2026-08-04T00:00:00Z".to_string(),
        }];
        r.user_profile = UserProfile {
            user_nickname: Some("小明".to_string()),
            pet_name: None,
            personality_style: None,
            relationship_style: None,
        };
        let intent = Intent {
            goal: "converse".to_string(),
            memory_anchor: String::new(),
            tone: "gentle".to_string(),
            proactive: false,
            action: "normal".to_string(),
            capability: crate::tools::CapabilityMode::None,
        };
        let messages = allocate_qa(&r, &[], &EmotionState::default(), &intent);
        let sys = messages[0].content_str();
        assert!(sys.contains("温柔"), "QA prompt should keep core persona trait");
        assert!(sys.contains("小明"), "QA prompt should keep user nickname");
    }
}
