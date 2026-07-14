use crate::llm::client::ChatMessage;
use std::collections::VecDeque;

/// Maximum messages held in working memory (20 conversation turns = 40 messages).
const MAX_MESSAGES: usize = 40;

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

    /// Appends a message, evicting the oldest if over capacity.
    pub fn push(&mut self, msg: ChatMessage) {
        if self.messages.len() >= MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
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
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        }
    }

    #[test]
    fn test_push_and_recall() {
        let mut wm = WorkingMemory::new();
        wm.push(msg("user", "hello"));
        wm.push(msg("assistant", "hi"));
        assert_eq!(wm.len(), 2);
        assert_eq!(wm.recall_last().unwrap().content, "hi");
    }

    #[test]
    fn test_sliding_window() {
        let mut wm = WorkingMemory::new();
        for i in 0..50 {
            wm.push(msg("user", &format!("msg {}", i)));
        }
        assert_eq!(wm.len(), MAX_MESSAGES);
        // Oldest should be evicted; first message is "msg 10"
        let ctx = wm.get_context();
        assert_eq!(ctx[0].content, "msg 10");
        assert_eq!(ctx[ctx.len() - 1].content, "msg 49");
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
