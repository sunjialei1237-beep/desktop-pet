//! Memory Hygiene Gate — deterministic, LLM-free filter applied to extractor
//! output BEFORE facts are written (Architecture #1: Rust validates, the LLM
//! only proposes).
//!
//! Catches the recurring extractor miss-extraction failure modes seen in real
//! data: knowledge/trivia questions stored as user facts, the model's own
//! conversational context mistaken for a user attribute, and invented
//! out-of-whitelist categories. See ADR `docs/decisions/2026-08-09-memory-hygiene-layer.md`.
//!
//! This is a conservative DENY-list: it drops only facts whose category, key, or
//! value matches a KNOWN noise shape; anything ambiguous is admitted (the
//! extractor's own confidence + `dedup_insert` still apply). It has no config
//! kill-switch by design — it is a zero-cost deterministic step, exactly like
//! the existing `dedup_insert` / `expire_old` ingest filters (which also have no
//! toggle). Principle #6's kill-switch pattern is reserved for capabilities with
//! real running cost (LLM calls, scheduled work); this gate has neither.

use crate::mind::extractor::FactInput;

/// The only categories that describe a durable user attribute. Anything else
/// (e.g. `pet_dog`, `current_reading`, `geography`) is an invented category and
/// is rejected at the gate. Mirrors the list in `resources/prompts/extractor.txt`.
const VALID_CATEGORIES: &[&str] = &[
    "preference",
    "relationship",
    "goal",
    "profile",
    "school",
    "work",
    "health",
];

/// English value substrings that signal the model recorded a conversational act
/// or general knowledge rather than a user attribute. Chinese trivia (e.g.
/// "太阳东升西落") is caught instead by its noise KEY (`knowledge_question`),
/// because the extractor consistently mis-labels trivia that way; a pure-Chinese
/// trivia value under a clean key is an acknowledged residual gap (addressable
/// later with an optional LLM-judge flag if it appears — ADR "不做").
const NOISE_VALUE_PATTERNS: &[&str] = &[
    "asked about",
    "asking about",
    "user asked",
    "user is asking",
    "is asking about my",
    "does not know",
    "doesn't know",
    "curious about",
    // Pseudo-facts already filtered downstream by proactive's is_anchorable_fact;
    // kept consistent here so they never enter known_facts either.
    "user is busy",
    "busy with work",
];

/// Returns `true` if the fact should be STORED, `false` if it is extractor noise.
///
/// Three independent deny checks — any match rejects:
/// 1. category outside the whitelist (invented category),
/// 2. key names a non-fact (a question / knowledge gap / belief / trivia label),
/// 3. value reads as "the user asked / doesn't know / is busy" — a
///    conversational act or general knowledge, not a user attribute.
pub fn admits(f: &FactInput) -> bool {
    is_valid_category(&f.category) && !is_noise_key(&f.key) && !is_noise_value(&f.value)
}

/// Convenience: filter a slice of facts down to the admissible ones (preserving
/// order). Used by the ingest write path.
pub fn filter_facts(facts: &[FactInput]) -> Vec<FactInput> {
    facts.iter().filter(|f| admits(f)).cloned().collect()
}

fn is_valid_category(category: &str) -> bool {
    VALID_CATEGORIES.iter().any(|c| *c == category)
}

/// Key shapes that signal a non-fact. The extractor mis-labels trivia and
/// conversational questions with these suffixes/prefixes.
fn is_noise_key(key: &str) -> bool {
    let k = key.to_lowercase();
    k.ends_with("_question")
        || k.ends_with("_gap")
        || k.ends_with("_knowledge")
        || k.starts_with("belief_in_")
}

fn is_noise_value(value: &str) -> bool {
    let v = value.to_lowercase();
    NOISE_VALUE_PATTERNS.iter().any(|p| v.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(category: &str, key: &str, value: &str) -> FactInput {
        FactInput {
            category: category.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            confidence: 0.9,
        }
    }

    #[test]
    fn admits_a_clean_user_fact() {
        assert!(admits(&fact("preference", "drink", "likes milk tea")));
        assert!(admits(&fact("relationship", "pet_name", "糯米")));
        assert!(admits(&fact("goal", "job_search", "preparing for internship")));
    }

    #[test]
    fn rejects_invented_category() {
        // Real observed noise: the extractor invented `pet_dog` / `current_reading`
        // / `geography` categories. Info survives in whitelisted categories.
        assert!(!admits(&fact("pet_dog", "name", "糯米")));
        assert!(!admits(&fact("geography", "capital", "Beijing")));
        assert!(!admits(&fact("current_reading", "book", "a new short story")));
    }

    #[test]
    fn rejects_noise_key_shapes() {
        // The Chinese trivia "太阳东升西落" was stored as key `knowledge_question`
        // under a valid category — caught here by the key, not the (English) value.
        assert!(!admits(&fact("profile", "knowledge_question", "太阳从东方升起")));
        assert!(!admits(&fact("profile", "ocean_knowledge", "Pacific")));
        assert!(!admits(&fact("profile", "moon_gap", "why it shines")));
        assert!(!admits(&fact("profile", "belief_in_aliens", "maybe")));
    }

    #[test]
    fn rejects_conversational_act_values() {
        assert!(!admits(&fact("profile", "ocean", "asked about the largest ocean")));
        assert!(!admits(&fact("profile", "moon", "does not know why the moon shines")));
        assert!(!admits(&fact("profile", "dreams", "user is asking about my dreams")));
        assert!(!admits(&fact("work", "status", "user is busy")));
        assert!(!admits(&fact("work", "load", "busy with work")));
        // "curious about X" = the user asked, not a fact about them.
        assert!(!admits(&fact("profile", "interest", "curious about astronomy")));
    }

    #[test]
    fn filter_facts_preserves_admitted_order() {
        let facts = vec![
            fact("preference", "drink", "coffee"),
            fact("profile", "knowledge_question", "trivia"),
            fact("relationship", "pet_name", "糯米"),
            fact("geography", "cap", "Tokyo"),
        ];
        let kept = filter_facts(&facts);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].value, "coffee");
        assert_eq!(kept[1].value, "糯米");
    }

    #[test]
    fn case_insensitive_on_key_and_value() {
        assert!(!admits(&fact("profile", "OCEAN_KNOWLEDGE", "pacific")));
        assert!(!admits(&fact("profile", "x", "Asked About the ocean")));
    }
}
