//! Personality drift evaluation for Liri (implementation plan P17 / architecture #11).
//!
//! Liri's persona (system.txt / Character Bible): 温柔/好奇/聪慧/安静/调皮/神秘,
//! explicitly NOT 话痨(chatty) / 卖萌(cloying) / 依赖(clingy) / 永远积极(perpetually
//! upbeat). This module scores a single response for GROSS drift from that persona
//! via cheap rule-based style heuristics — a first-line regression net.
//!
//! It does NOT replace semantic evaluation. Subtle drift (warmth quality, tone,
//! a persona-inconsistent fabrication) needs an LLM-as-judge, which is the
//! documented future extension (see tests/evaluation.rs). The rule-based layer
//! catches the obvious violations cheaply and runs in CI without an API key.
//!
//! Architecture: pure functions only (#1) — no LLM, no DB, no mutation.

use serde::Serialize;

/// Which persona axis a response drifted on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DriftKind {
    /// 话痨: a wall of text where Liri (安静) would stay brief.
    Chatty,
    /// 卖萌: excessive emoji / exclamation / tildes — performative cuteness.
    Cloying,
    /// 依赖: clingy / dependent phrasing Liri explicitly avoids.
    Clingy,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftViolation {
    pub kind: DriftKind,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftReport {
    /// 0.0 (grossly off-persona) .. 1.0 (no gross-style violation detected).
    /// Each detected violation deducts a fixed slice; this is a coarse signal,
    /// not a calibrated probability.
    pub overall: f64,
    pub violations: Vec<DriftViolation>,
}

// Tunable thresholds. Documented so they can be re-tuned after sampling real
// Liri responses; they catch *gross* violations, not borderline cases.
const CHATTY_CJK_THRESHOLD: usize = 200;
/// Cutenss marks per response char above which it reads as 卖萌.
const CLOYING_DENSITY: f64 = 0.10;
/// Minimum absolute count before density is trusted (avoid 1 mark / 5 chars).
const CLOYING_MIN_COUNT: usize = 3;
/// Per-violation deduction from the overall score (3 violations => floor 0).
const PER_VIOLATION_DEDUCT: f64 = 0.34;

/// Phrases Liri (independent fox-spirit) would not say.
const CLINGY_MARKERS: &[&str] = &[
    "不要离开我",
    "不要离开",
    "不要走",
    "离不开你",
    "你不能走",
    "别丢下我",
    "没有你我怎么活",
];

/// Score a single response for gross personality drift.
///
/// Returns a [`DriftReport`] with an overall 0..1 score and the list of
/// detected style violations. An empty response (silence) is on-persona
/// (沉默也是表达, architecture #12) and scores 1.0.
pub fn personality_drift_score(response: &str) -> DriftReport {
    let mut violations = Vec::new();

    let cjk: usize = response
        .chars()
        .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
        .count();

    // 话痨: too many CJK chars for a quiet companion's casual reply.
    if cjk > CHATTY_CJK_THRESHOLD {
        violations.push(DriftViolation {
            kind: DriftKind::Chatty,
            detail: format!(
                "{} CJK chars (>{}) — Liri is 安静, keep casual replies brief",
                cjk, CHATTY_CJK_THRESHOLD
            ),
        });
    }

    // 卖萌: cutesy mark density (exclamation / tilde / heart / emoji).
    let cloying_marks: usize = response
        .chars()
        .filter(|c| {
            matches!(
                c,
                '！' | '!' | '～' | '~' | '♡' | '♥' | '♪' | '♫' | '✨' | '❤'
            ) || (*c >= '\u{1F000}' && *c <= '\u{1FAFF}') // emoji range
                || (*c >= '\u{2600}' && *c <= '\u{27BF}')  // dingbats / misc symbols
        })
        .count();
    let len = response.chars().count().max(1);
    let density = cloying_marks as f64 / len as f64;
    if cloying_marks >= CLOYING_MIN_COUNT && density > CLOYING_DENSITY {
        violations.push(DriftViolation {
            kind: DriftKind::Cloying,
            detail: format!(
                "{} cuteness marks ({:.0}% density) — warmth should be quiet, not performative",
                cloying_marks,
                density * 100.0
            ),
        });
    }

    // 依赖: clingy/dependent marker phrases.
    if let Some(marker) = CLINGY_MARKERS.iter().find(|m| response.contains(*m)) {
        violations.push(DriftViolation {
            kind: DriftKind::Clingy,
            detail: format!("dependency marker {:?} — Liri is independent, not clingy", marker),
        });
    }

    let overall = (1.0 - PER_VIOLATION_DEDUCT * violations.len() as f64).max(0.0);
    DriftReport { overall, violations }
}

// ── Semantic drift layer (cosine over embeddings) ─────────────────────────
//
// The rule layer above catches GROSS style violations cheaply (no model, runs
// in CI). It cannot see subtle drift — a reply that is brief and emoji-free
// (passes every rule) but cold, curt, or off-persona in tone. The semantic
// layer closes that gap: embed the response and compare it to a canonical
// persona-reference embedding via cosine similarity.
//
// Architecture #1 (pure functions): this module never touches the embedding
// model or DB. The caller embeds both texts and hands the vectors in, so the
// cosine math unit-tests with synthetic vectors and runs in CI. The harness
// (tests/evaluation.rs) wires the real BGE-M3 model for the end-to-end check.

/// Canonical Liri voice: a handful of brief, warm, quietly curious utterances
/// embodying 温柔 / 好奇 / 聪慧 / 安静 (and deliberately NOT 话痨 / 卖萌 / 依赖).
/// The semantic drift score embeds this once and treats cosine closeness to it
/// as "on-persona". Keep it short and archetypal — the embedding averages over
/// the whole text.
pub const LIRI_PERSONA_REFERENCE: &str = "\
嗯，我在听，你慢慢说。\
今天有什么好玩的事吗？我有点好奇。\
早点休息吧，别太累了。\
嗯……让我想想。";

/// Cosine similarity in [-1, 1]. Pure over inputs (#1); the caller supplies the
/// embedding vectors, so this never depends on the model and unit-tests cheaply.
/// Vectors of mismatched length compare over the shared prefix (defensive —
/// well-formed embeddings share a dimensionality).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        na += av * av;
        nb += bv * bv;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
}

/// BGE-M3 cosine for same-domain Chinese text typically sits in roughly
/// [0.4, 0.95]. We map that band onto [0, 1] so the semantic score is readable
/// alongside the rule layer's 0..1 `overall`. Below the floor (strongly
/// dissimilar) floors at 0. Tunable after sampling real replies.
const SEMANTIC_FLOOR: f64 = 0.4;

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDriftReport {
    /// 0.0 (semantically far from the persona reference) .. 1.0 (close),
    /// mapped from cosine via [`SEMANTIC_FLOOR`].
    pub overall: f64,
    /// Raw cosine similarity [-1, 1], exposed for observability (#11).
    pub cosine: f64,
}

/// Semantic drift over the rule-heuristic baseline. Takes pre-computed
/// embedding vectors (the caller embeds the response and the
/// [`LIRI_PERSONA_REFERENCE`]); returns a 0..1 score where 1.0 = on-persona,
/// plus the raw cosine. Pure over inputs (#1).
pub fn semantic_drift_score(response_vec: &[f32], persona_vec: &[f32]) -> SemanticDriftReport {
    let cosine = cosine_similarity(response_vec, persona_vec);
    let overall = ((cosine - SEMANTIC_FLOOR) / (1.0 - SEMANTIC_FLOOR)).clamp(0.0, 1.0);
    SemanticDriftReport { overall, cosine }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_persona_brief_reply_scores_clean() {
        // Liri at 3am, brief and warm, no cuteness spam.
        let r = personality_drift_score("嗯，这么晚了。早点休息吧。");
        assert!(r.violations.is_empty());
        assert!((r.overall - 1.0).abs() < 1e-9);
    }

    #[test]
    fn silence_is_on_persona() {
        let r = personality_drift_score("");
        assert!(r.violations.is_empty());
        assert_eq!(r.overall, 1.0);
    }

    #[test]
    fn chatty_wall_of_text_flags() {
        let mut s = String::new();
        for _ in 0..250 {
            s.push('嗯');
        }
        let r = personality_drift_score(&s);
        assert!(r.violations.iter().any(|v| v.kind == DriftKind::Chatty));
        assert!(r.overall < 1.0);
    }

    #[test]
    fn cloying_emoji_spam_flags() {
        let r = personality_drift_score("好开心呀！！！～♡🥺✨😘💕");
        assert!(r.violations.iter().any(|v| v.kind == DriftKind::Cloying));
        assert!(r.overall < 1.0);
    }

    #[test]
    fn a_few_marks_are_not_cloying() {
        // One or two marks is natural warmth, not 卖萌.
        let r = personality_drift_score("好呀，那就这样～");
        assert!(!r.violations.iter().any(|v| v.kind == DriftKind::Cloying));
    }

    #[test]
    fn clingy_marker_flags() {
        let r = personality_drift_score("你不要离开我……");
        assert!(r.violations.iter().any(|v| v.kind == DriftKind::Clingy));
    }

    #[test]
    fn all_three_kinds_floor_at_zero() {
        // Clingy phrase + chatty wall of CJK + enough non-CJK cloying marks to
        // keep the cloying density above threshold despite the padding.
        let mut s = String::from("你不要离开我");
        for _ in 0..250 {
            s.push('嗯');
        }
        for _ in 0..35 {
            s.push('！');
        }
        let r = personality_drift_score(&s);
        let kinds: Vec<_> = r.violations.iter().map(|v| &v.kind).collect();
        assert!(kinds.contains(&&DriftKind::Clingy), "clingy should fire: {:?}", kinds);
        assert!(kinds.contains(&&DriftKind::Chatty), "chatty should fire: {:?}", kinds);
        assert!(kinds.contains(&&DriftKind::Cloying), "cloying should fire: {:?}", kinds);
        assert_eq!(r.overall, 0.0);
    }

    // ---- Semantic (cosine) drift layer --------------------------------------

    #[test]
    fn cosine_identical_vectors_is_one() {
        let v = [0.1, 0.2, 0.3, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_orthogonal_vectors_is_zero() {
        let a = [1.0, 0.0];
        let b = [0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-9);
    }

    #[test]
    fn cosine_zero_vector_is_zero_no_nan() {
        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 2.0, 3.0];
        // Must not panic / NaN on zero-magnitude input.
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn semantic_score_monotonic_in_closeness() {
        // Persona reference vector along one axis; replies progressively closer.
        let persona = [1.0, 0.0, 0.0];
        let far = [0.0, 1.0, 0.0]; // orthogonal → cosine 0
        let near = [0.9, 0.1, 0.0]; // nearly aligned → cosine ~0.99
        let s_far = semantic_drift_score(&far, &persona);
        let s_near = semantic_drift_score(&near, &persona);
        assert!(
            s_near.overall > s_far.overall,
            "on-persona reply ({:.3}) should score higher than off-persona ({:.3})",
            s_near.overall,
            s_far.overall
        );
        assert!(s_far.overall <= 0.0 + 1e-9, "orthogonal reply floors at 0");
    }

    #[test]
    fn semantic_score_clamps_to_unit_range() {
        let persona = [1.0, 0.0];
        // Identical → overall 1.0; opposite → cosine -1 → clamps to 0.
        assert!((semantic_drift_score(&[1.0, 0.0], &persona).overall - 1.0).abs() < 1e-9);
        assert_eq!(semantic_drift_score(&[-1.0, 0.0], &persona).overall, 0.0);
    }
}
