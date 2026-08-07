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
}
