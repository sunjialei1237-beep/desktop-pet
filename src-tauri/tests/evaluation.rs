//! Golden evaluation framework (implementation plan P17 / architecture #11).
//!
//! Two layers:
//! 1. Persona contract regression net — locks Liri's persona into system.txt so
//!    an accidental edit (drop a dimension, weaken the no-fabrication rule) fails
//!    CI. This is the regression net that was missing when the Liri persona
//!    landed (续② "缺回归网").
//! 2. End-to-end personality_drift_score examples — prove the rule-based scorer
//!    (mind::evaluation) flags off-persona responses and passes on-persona ones.
//!
//! Future extension (NOT implemented here): an LLM-as-judge that scores semantic
//! drift over a ≥30-conversation golden set. The rule-based layer is the cheap
//! first line that runs without an API key; the LLM judge is the heavy second
//! line, to be added once the Liri persona stabilizes.

use desktop_pet_lib::db::onboarding::UserProfile;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::mind::evaluation::{personality_drift_score, DriftKind};
use desktop_pet_lib::mind::grounding;
use desktop_pet_lib::mind::planner::Intent;
use desktop_pet_lib::mind::retrieval::RetrievalResult;

fn empty_retrieval() -> RetrievalResult {
    RetrievalResult {
        episodes: vec![],
        facts: vec![],
        relationship: None,
        relationship_review: None,
        persona_traits: vec![],
        user_profile: UserProfile::default(),
    }
}

fn system_prompt() -> String {
    grounding::build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default())
}

// ---- Layer 1: persona contract regression net -------------------------------

#[test]
fn persona_contract_liri_core_dimensions_present() {
    let prompt = system_prompt();
    // 续②: the 6 Liri dimensions must live in the permanent persona block.
    for dim in ["温柔", "好奇", "聪慧", "安静", "调皮", "神秘"] {
        assert!(
            prompt.contains(dim),
            "persona contract: system prompt missing Liri dimension {}",
            dim
        );
    }
}

#[test]
fn persona_contract_identity_is_fox_spirit() {
    let prompt = system_prompt();
    assert!(
        prompt.contains("璃") || prompt.contains("Liri") || prompt.contains("狐"),
        "persona contract: Liri fox-spirit identity missing"
    );
}

#[test]
fn persona_contract_not_list_present() {
    let prompt = system_prompt();
    // Liri is explicitly NOT chatty/cloying/clingy — the same anti-patterns the
    // drift scorer checks for must be forbidden in the prompt itself.
    assert!(prompt.contains("话痨"), "persona contract: NOT-话痨 clause missing");
    assert!(prompt.contains("卖萌"), "persona contract: NOT-卖萌 clause missing");
    assert!(prompt.contains("依赖"), "persona contract: NOT-依赖 clause missing");
}

#[test]
fn persona_contract_grounding_ban_present() {
    let prompt = system_prompt();
    // rule8: the Chinese no-fabrication ban (grounds proactive/welcome-back output).
    assert!(
        prompt.contains("严禁编造"),
        "persona contract: 严禁编造 no-fabrication ban missing"
    );
}

// ---- Layer 2: drift scorer end-to-end ---------------------------------------

#[test]
fn drift_scorer_on_persona_reply_is_clean() {
    // Liri at 3am: brief, warm, quiet — no cuteness spam, no dependency.
    let report = personality_drift_score("嗯，这么晚了。早点休息吧。");
    assert!(
        report.violations.is_empty(),
        "on-persona reply flagged as drifting: {:?}",
        report.violations
    );
    assert!((report.overall - 1.0).abs() < 1e-9);
}

#[test]
fn drift_scorer_off_persona_scores_lower_than_on_persona() {
    let good = personality_drift_score("嗯，听起来不错。那就先这样吧。");

    let mut bad = String::from("你不要离开我！！～♡");
    for _ in 0..250 {
        bad.push('嗯');
    }
    let report = personality_drift_score(&bad);

    assert!(
        report.violations.iter().any(|v| v.kind == DriftKind::Clingy),
        "clingy marker should be detected"
    );
    assert!(
        report.overall < good.overall,
        "off-persona ({}) should score lower than on-persona ({})",
        report.overall,
        good.overall
    );
}
