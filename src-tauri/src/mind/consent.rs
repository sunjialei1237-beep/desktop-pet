//! Conversational filesystem consent (plan 2026-08-17 §3.7): mirrors the
//! cross-turn forget-disambiguation pattern. When an Observe tool is denied
//! for an unauthorized root, Rust arms a pending authorization (Principle #1:
//! the LLM never writes permission state); the user's next short reply
//! resolves it — granted (once / always) or denied (with a 24h re-ask
//! cooldown so a "no" is never nagged).
//!
//! The resolution turn skips ingest ("可以" is not a memory) and the retry is
//! NATURAL CONVERSATION: once granted, the user's next environment request
//! passes the policy — no explicit retry machinery exists.

use chrono::{DateTime, Utc};

/// Cross-turn slot held in AppState (same shape as PendingForget).
#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    /// Canonical root the denied tool call wanted.
    pub root: String,
    pub created_at: DateTime<Utc>,
}

/// Stale-slot window — mirrors the forget disambiguation's 90s.
pub const STALE_AFTER_SECS: i64 = 90;

/// Pure classification of the user's reply to an authorization ask.
/// Deny patterns are checked FIRST ("不要"/"不行" contain allow-ish chars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentReply {
    /// "以后都行 / 一直可以" — persistent grant.
    Always,
    /// "可以 / 就这次 / 行" — single-interaction grant.
    Once,
    /// "不行 / 不要 / 算了" — explicit refusal.
    Deny,
    /// None of the above — treat the ask as abandoned, proceed normally.
    Unrelated,
}

pub fn classify_reply(text: &str) -> ConsentReply {
    let t = text.trim();
    let lower = t.to_lowercase();

    // Deny first: "不要" / "不行" / "先不用" must never fall into allow.
    const DENY: &[&str] = &[
        "不行", "不要", "不用", "不许", "拒绝", "算了", "别看", "不可以", "no", "don't",
    ];
    if DENY.iter().any(|k| lower.contains(k)) {
        return ConsentReply::Deny;
    }

    const ALWAYS: &[&str] = &["以后", "一直", "永久", "总是", "always", "whenever"];
    if ALWAYS.iter().any(|k| lower.contains(k)) {
        return ConsentReply::Always;
    }

    // Allow: a short affirmative reply. Whole-message match for the bare
    // ones ("好", "行", "嗯") so "好累" doesn't count; contains for the
    // unambiguous phrases.
    const BARE_ALLOW: &[&str] = &["好", "行", "嗯", "可以", "允许", "ok", "yes", "好呀", "好嘞", "没问题"];
    if BARE_ALLOW.contains(&t) {
        return ConsentReply::Once;
    }
    const PHRASE_ALLOW: &[&str] = &["就这次", "这次可以", "看吧", "给你看", "你看看吧", "同意"];
    if PHRASE_ALLOW.iter().any(|k| t.contains(k)) {
        return ConsentReply::Once;
    }

    ConsentReply::Unrelated
}

/// Outcome of resolving the pending authorization this turn (mirrors
/// PendingResolution's role in the forget flow).
#[derive(Debug, Clone)]
pub enum ConsentState {
    /// No pending ask, or the ask was abandoned — normal pipeline.
    Proceed,
    /// User granted access; the fs_grants row is already written.
    Granted { root: String, always: bool },
    /// User refused; a deny row (with cooldown) is already written.
    Denied { root: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_yes_is_once() {
        assert_eq!(classify_reply("可以"), ConsentReply::Once);
        assert_eq!(classify_reply("好"), ConsentReply::Once);
        assert_eq!(classify_reply("嗯"), ConsentReply::Once);
        assert_eq!(classify_reply("ok"), ConsentReply::Once);
    }

    #[test]
    fn persistent_yes_is_always() {
        assert_eq!(classify_reply("以后都可以"), ConsentReply::Always);
        assert_eq!(classify_reply("一直允许你"), ConsentReply::Always);
    }

    #[test]
    fn deny_wins_over_allow_chars() {
        // "不要" contains "要"-adjacent chars; "不行" contains "行" — deny
        // must be checked first and win.
        assert_eq!(classify_reply("不要"), ConsentReply::Deny);
        assert_eq!(classify_reply("不行"), ConsentReply::Deny);
        assert_eq!(classify_reply("先不用了"), ConsentReply::Deny);
        assert_eq!(classify_reply("算了"), ConsentReply::Deny);
    }

    #[test]
    fn bare_allow_does_not_match_inside_sentences() {
        // "好累" must NOT count as consent — bare forms match whole message only.
        assert_eq!(classify_reply("我今天好累"), ConsentReply::Unrelated);
        assert_eq!(classify_reply("行吧那就看看"), ConsentReply::Unrelated);
    }

    #[test]
    fn unrelated_proceeds() {
        assert_eq!(classify_reply("今天天气怎么样"), ConsentReply::Unrelated);
        assert_eq!(classify_reply(""), ConsentReply::Unrelated);
    }
}
