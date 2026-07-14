use crate::emotion::state::EmotionState;
use crate::llm::client::ChatMessage;

/// Whether retrieval should be attempted for this message.
#[derive(Debug)]
pub struct TriggerDecision {
    pub should_retrieve: bool,
    pub reason: String,
}

/// Memory trigger: decides whether it's worth searching memories for this input.
/// Uses fast rules only (no LLM call) per architecture principle #8 (cost is design).
pub fn should_retrieve(
    text: &str,
    _emotion: &EmotionState,
    working_memory: &[ChatMessage],
) -> TriggerDecision {
    let lower = text.to_lowercase();

    // Rule 1: User asks about memory ("do you remember...", "you know...")
    let memory_triggers = [
        "remember", "recall", "last time", "you know", "i said", "i told you",
        "ji de", "zhidao", "shang ci", "shuo guo",
    ];
    for trigger in memory_triggers.iter() {
        if lower.contains(trigger) {
            return TriggerDecision {
                should_retrieve: true,
                reason: format!("memory reference detected: '{}'", trigger),
            };
        }
    }

    // Rule 2: Substantial content (longer than 5 chars, not pure greeting)
    let greetings = ["hi", "hello", "ok", "haha", "lol", "hmm", "bye", "goodnight", "zai jian", "ni hao"];
    let trimmed = text.trim();
    if trimmed.chars().count() <= 3 || greetings.iter().any(|g| lower == *g || lower.starts_with(g)) {
        return TriggerDecision {
            should_retrieve: false,
            reason: "short greeting or noise".to_string(),
        };
    }

    // Rule 3: Check if recent working memory already has similar context
    // (avoid redundant retrieval of the same memories)
    let recent_user_msgs: Vec<&ChatMessage> = working_memory
        .iter()
        .filter(|m| m.role == "user")
        .rev()
        .take(5)
        .collect();

    // If the last 5 user messages were all very short (chitchat), don't retrieve
    if recent_user_msgs.len() >= 5
        && recent_user_msgs
            .iter()
            .all(|m| m.content.trim().chars().count() <= 10)
    {
        return TriggerDecision {
            should_retrieve: false,
            reason: "5+ consecutive short messages".to_string(),
        };
    }

    // Default: retrieve for substantive messages
    TriggerDecision {
        should_retrieve: true,
        reason: "substantive message".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotion::state::EmotionState;
    use crate::mind::working::WorkingMemory;

    fn emotion() -> EmotionState {
        EmotionState::default()
    }

    #[test]
    fn test_explicit_memory_reference() {
        let wm = WorkingMemory::new();
        let decision = should_retrieve("do you remember what I said last time?", &emotion(), &wm.get_context());
        assert!(decision.should_retrieve);
    }

    #[test]
    fn test_short_greeting() {
        let wm = WorkingMemory::new();
        let decision = should_retrieve("haha", &emotion(), &wm.get_context());
        assert!(!decision.should_retrieve);
    }

    #[test]
    fn test_substantive_message() {
        let wm = WorkingMemory::new();
        let decision = should_retrieve("I went to eat hotpot with my friends today", &emotion(), &wm.get_context());
        assert!(decision.should_retrieve);
    }

    #[test]
    fn test_consecutive_short_messages() {
        let mut wm = WorkingMemory::new();
        for _ in 0..6 {
            wm.push(ChatMessage { role: "user".to_string(), content: "ok".to_string() });
            wm.push(ChatMessage { role: "assistant".to_string(), content: "hmm".to_string() });
        }
        let decision = should_retrieve("what about tomorrow", &emotion(), &wm.get_context());
        assert!(!decision.should_retrieve);
        assert!(decision.reason.contains("consecutive"));
    }

    #[test]
    fn test_very_short() {
        let wm = WorkingMemory::new();
        let decision = should_retrieve("...", &emotion(), &wm.get_context());
        assert!(!decision.should_retrieve);
    }
}
