use crate::llm::client::ChatMessage;
use std::collections::VecDeque;

/// Soft ceiling for messages held in working memory (40 messages ≈ 20 turns).
/// Allowed to grow past it up to the next push; then ONE batch trim to
/// `TRIM_TO` instead of popping one message per push.
///
/// Why batch: DeepSeek prefix caching only hits when a request fully matches a
/// persisted prefix unit — popping the oldest message EVERY push meant every
/// turn re-sent a brand-new prefix (cache reset every turn once the window was
/// full). Batch trimming resets the cache only once per ~8 turns (24→41), so
/// the other turns keep a byte-identical history prefix and keep hitting.
const MAX_MESSAGES: usize = 40;
/// After a batch trim, the window holds this many messages (24 = 12 turns).
const TRIM_TO: usize = 24;

/// Short-term conversation buffer. Pure in-memory, never persisted to DB.
/// Slides forward as new messages arrive, keeping only the most recent context.
pub struct WorkingMemory {
    messages: VecDeque<ChatMessage>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        WorkingMemory {
            messages: VecDeque::with_capacity(MAX_MESSAGES),
        }
    }

    /// Appends a message. Over the soft ceiling, trims the window ONCE in a
    /// batch (oldest dropped down to `TRIM_TO`) instead of popping per push —
    /// between trims the message sequence grows strictly by appending, which
    /// is exactly what the DeepSeek prefix cache needs to hit.
    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push_back(msg);
        if self.messages.len() > MAX_MESSAGES {
            while self.messages.len() > TRIM_TO {
                self.messages.pop_front();
            }
        }
    }

    /// Returns all messages as a Vec for LLM context injection.
    pub fn get_context(&self) -> Vec<ChatMessage> {
        self.messages.iter().cloned().collect()
    }

    /// Returns the most recent message, if any.
    pub fn recall_last(&self) -> Option<&ChatMessage> {
        self.messages.back()
    }

    /// Returns the number of messages currently held.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns true if no messages are held.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clears all messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        match role {
            "user" => ChatMessage::user(content),
            "assistant" => ChatMessage::assistant(content),
            _ => ChatMessage::system(content),
        }
    }

    #[test]
    fn test_push_and_recall() {
        let mut wm = WorkingMemory::new();
        wm.push(msg("user", "hello"));
        wm.push(msg("assistant", "hi"));
        assert_eq!(wm.len(), 2);
        assert_eq!(wm.recall_last().unwrap().content_str(), "hi");
    }

    #[test]
    fn test_sliding_window() {
        let mut wm = WorkingMemory::new();
        for i in 0..50 {
            wm.push(msg("user", &format!("msg {}", i)));
        }
        // Batch trim: grows freely to 41, then one shot down to TRIM_TO (24),
        // then +9 appends → 33. (Per-push eviction was replaced by batch
        // trims for prefix-cache stability.)
        assert_eq!(wm.len(), 33);
        assert!(wm.len() <= MAX_MESSAGES + 1);
        let ctx = wm.get_context();
        assert_eq!(ctx[0].content_str(), "msg 17");
        assert_eq!(ctx[ctx.len() - 1].content_str(), "msg 49");
    }

    #[test]
    fn test_batch_trim_prefers_append_then_trim() {
        // Between trims the sequence must grow purely by appending (cache
        // prefix stability); eviction happens in one batch only.
        let mut wm = WorkingMemory::new();
        for i in 0..41 {
            wm.push(msg("user", &format!("msg {}", i)));
        }
        // 41st push crossed the soft ceiling → batch trim to TRIM_TO (24).
        assert_eq!(wm.len(), TRIM_TO);
        assert_eq!(wm.get_context()[0].content_str(), "msg 17");
        assert_eq!(wm.get_context()[wm.len() - 1].content_str(), "msg 40");
        // Appending after the trim must NOT drop anything until the ceiling
        // is crossed again (prefix keeps growing).
        wm.push(msg("user", "msg 41"));
        assert_eq!(wm.len(), TRIM_TO + 1);
        assert_eq!(wm.get_context()[0].content_str(), "msg 17");
    }

    #[test]
    fn test_empty_recall() {
        let wm = WorkingMemory::new();
        assert!(wm.recall_last().is_none());
    }

    #[test]
    fn test_clear() {
        let mut wm = WorkingMemory::new();
        wm.push(msg("user", "test"));
        wm.clear();
        assert_eq!(wm.len(), 0);
    }
}
