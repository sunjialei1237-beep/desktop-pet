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

/// Cross-turn slot held in AppState (same shape as PendingForget). One ask
/// may cover several roots denied within the same tool loop; the user's one
/// reply resolves all of them together (§8.5-M6).
#[derive(Debug, Clone)]
pub struct PendingAuthorization {
    /// Canonical roots the denied tool calls wanted (deduplicated).
    pub roots: Vec<String>,
    pub created_at: DateTime<Utc>,
    /// U1 (plan §8.4): the capability the LLM originally wanted and the
    /// top-level request text. Carried through the consent state so "可以"
    /// re-runs the SAME tool round in the SAME turn instead of "先答应，
    /// 下一句再说一遍".
    pub followup_capability: crate::tools::CapabilityMode,
    pub followup_text: String,
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

    // 1) Bare deny keywords — checked first, always win.
    const DENY: &[&str] = &[
        "不行", "不要", "不用", "不许", "拒绝", "算了", "别看", "不可以", "不想",
        "不能", "不准", "不让", "不同意", "别同意", "no", "don't",
    ];
    if DENY.iter().any(|k| lower.contains(k)) {
        return ConsentReply::Deny;
    }

    // 2) Negation glued to an allow anchor ("不想给你看", "以后再也不想看").
    // A negation immediately before the anchor flips the phrase into a refusal
    // even though the allow anchor itself is present.
    const ALLOW_ANCHORS: &[&str] = &["给你看", "看吧", "同意", "允许", "可以", "授权"];
    let negated_anchor = ALLOW_ANCHORS.iter().any(|anchor| {
        lower.match_indices(anchor).any(|(pos, _)| {
            let prefix = &lower[..pos];
            prefix
                .chars()
                .rev()
                .take(2)
                .any(|c| c == '不' || c == '别' || c == '没')
        })
    });
    if negated_anchor {
        return ConsentReply::Deny;
    }

    // 3) Persistent grant: a time word AND an affirmative anchor must coexist.
    // Time word alone never grants — "以后再说" / "一直没空" are unrelated.
    const TIME: &[&str] = &["以后", "一直", "永久", "总是", "每次", "always", "whenever"];
    const AFFIRM: &[&str] = &["都行", "可以", "没问题", "允许", "同意", "授权", "开放", "给你看", "看吧", "能看", "ok", "yes", "allow"];
    if TIME.iter().any(|k| lower.contains(k)) && AFFIRM.iter().any(|k| lower.contains(k)) {
        return ConsentReply::Always;
    }

    // 4) Allow: a short affirmative reply. Whole-message match for the bare
    // ones ("好", "行", "嗯") so "好累" doesn't count; contains for the
    // unambiguous phrases.
    const BARE_ALLOW: &[&str] = &[
        "好", "行", "嗯", "可以", "允许", "ok", "yes", "好呀", "好嘞", "没问题",
    ];
    if BARE_ALLOW.contains(&t) {
        return ConsentReply::Once;
    }
    const PHRASE_ALLOW: &[&str] = &[
        "就这次", "这次可以", "可以了", "可以的", "看吧", "给你看", "你看看吧", "同意",
    ];
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
    /// User granted access; the fs_grants rows are already written for every
    /// root the pending ask covered. `followup` carries the U1 continuation
    /// when there is one.
    Granted {
        roots: Vec<String>,
        always: bool,
        followup: Option<GrantFollowup>,
    },
    /// User refused; deny rows (with cooldown) are already written.
    Denied { roots: Vec<String> },
    /// No pending ask, but the user said a deny-regret phrase: every explicit
    /// deny row was flipped back to Once with a fresh clock (U4).
    UnfrozeDenies { count: usize },
}

/// Continuation payload for U1: after "可以", re-arm the same capability and
/// remind the LLM of the original request so it acts NOW, same turn.
#[derive(Debug, Clone)]
pub struct GrantFollowup {
    pub capability: crate::tools::CapabilityMode,
    pub text: String,
}

/// U4 standalone detection: phrases that mean "open it up again" regardless
/// of whether there is a pending ask this turn. Deliberately purely lexical —
/// same trust surface as classify_reply, no LLM judgment on security state.
pub fn looks_like_deny_revoke(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    const REGRET: &[&str] = &[
        "改主意", "现在开放", "开放吧", "解锁", "解禁", "解除禁止", "取消拒绝",
        "撤销拒绝", "重新开放", "给你看了", "可以给你看",
    ];
    REGRET.iter().any(|k| t.contains(k))
}

/// A create_note proposal waiting for the user's explicit "可以/不行"
/// (plan §3.6 F1 + Principle #11: mutation is never applied in the same
/// tool round that proposed it).
#[derive(Debug, Clone)]
pub struct PendingNote {
    pub filename: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Resolution outcome for the pending note this turn.
#[derive(Debug, Clone)]
pub enum NoteState {
    /// No pending note, or it timed out — normal pipeline.
    Proceed,
    /// User confirmed; the file was already written atomically.
    Saved { filename: String },
    /// User declined; the proposal was dropped, nothing written.
    Declined { filename: String },
    /// User confirmed but the atomic write failed (quota / IO) — nothing was
    /// written; tell the user honestly.
    SaveFailed { filename: String, why: String },
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
        assert_eq!(classify_reply("每次都能看"), ConsentReply::Always);
        assert_eq!(classify_reply("always ok"), ConsentReply::Always);
    }

    #[test]
    fn time_word_without_affirmation_is_unrelated() {
        // "以后再说" means LATER, never a grant; "一直没空" is just a status.
        assert_eq!(classify_reply("以后再说"), ConsentReply::Unrelated);
        assert_eq!(classify_reply("一直没空"), ConsentReply::Unrelated);
        assert_eq!(classify_reply("以后不想提这事"), ConsentReply::Deny);
    }

    #[test]
    fn negated_allow_anchor_is_deny() {
        // The dangerous false-consent class: the allow anchor is present but a
        // negation right before it flips the meaning.
        assert_eq!(classify_reply("我不同意"), ConsentReply::Deny);
        assert_eq!(classify_reply("别同意"), ConsentReply::Deny);
        assert_eq!(classify_reply("我不想给你看"), ConsentReply::Deny);
        assert_eq!(classify_reply("不给你看任何东西"), ConsentReply::Deny);
        assert_eq!(classify_reply("以后再也不想看"), ConsentReply::Deny);
        assert_eq!(classify_reply("每次不能看"), ConsentReply::Deny);
        assert_eq!(classify_reply("一直不准看"), ConsentReply::Deny);
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

    #[test]
    fn deny_regret_phrases_detected() {
        // U4: the user explicitly re-opens a previously denied location.
        assert!(looks_like_deny_revoke("我改主意了，现在开放那个项目"));
        assert!(looks_like_deny_revoke("刚才那个位置解锁吧"));
        assert!(looks_like_deny_revoke("撤销拒绝"));
        // Normal chitchat must NOT trip the unfreeze.
        assert!(!looks_like_deny_revoke("今天的云很开放啊"));
        assert!(!looks_like_deny_revoke(""));
    }
}
