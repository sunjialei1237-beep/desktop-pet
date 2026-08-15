//! Golden evaluation framework (implementation plan P17 / architecture #11).
//!
//! Three layers:
//! 1. Persona contract regression net — locks Liri's persona into system.txt so
//!    an accidental edit (drop a dimension, weaken the no-fabrication rule) fails
//!    CI. This is the regression net that was missing when the Liri persona
//!    landed (续② "缺回归网").
//! 2. End-to-end personality_drift_score examples — prove the rule-based scorer
//!    (mind::evaluation) flags off-persona responses and passes on-persona ones.
//! 3. Semantic drift over embeddings — closes the gap the rule layer CANNOT see:
//!    a reply that is brief and emoji-free (passes every rule) but cold, curt, or
//!    off-persona in tone. Embeds the response + a canonical persona reference
//!    with the REAL BGE-M3 model and asserts the on-persona cosine beats the
//!    off-persona cosine. Pure cosine math is unit-tested in mind::evaluation; this
//!    harness wires the model end to end.
//!
//! Future extension (NOT implemented here): an LLM-as-judge that scores semantic
//! drift over a ≥30-conversation golden set. The rule-based layer is the cheap
//! first line that runs without an API key; the LLM judge is the heavy final
//! line, to be added once the Liri persona stabilizes.

use desktop_pet_lib::config;
use desktop_pet_lib::db::onboarding::UserProfile;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::mind::evaluation::{
    cosine_similarity, personality_drift_score, semantic_drift_score, DriftKind,
    LIRI_PERSONA_REFERENCE,
};
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
        first_met: None,
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

// ---- Layer 3: semantic drift over embeddings (real BGE-M3) ------------------
//
// This is the gap the rule layer cannot see. Both replies below PASS every rule
// (neither is chatty / cloying / clingy), so the rule scorer gives them both
// overall = 1.0. But one is warm and on-persona, the other is cold and curt —
// only the embedding sees the difference. Run:
//   cargo test --test evaluation semantic_drift_end_to_end -- --nocapture

#[test]
fn semantic_drift_end_to_end() {
    let config = config::load_config().unwrap_or_default();
    let model_dir = config::resolve_model_dir(&config);
    println!("[setup] model_dir = {}", model_dir.display());

    let svc = EmbeddingService::new(&model_dir);
    svc.load().expect(
        "embedding model failed to load — check model_dir points at a complete \
         BGE-M3 ONNX export (model.onnx + model.onnx_data + tokenizer.json)",
    );
    assert!(svc.is_ready(), "model reported not ready after load");

    // Embed the canonical persona reference once.
    let persona_vec = svc.embed(LIRI_PERSONA_REFERENCE).expect("embed persona reference");

    // On-persona: brief, warm, quietly caring — Liri's voice at 3am.
    let on_persona = "嗯，这么晚了。早点休息吧。";
    // Off-persona: cold / dismissive / curt — passes every rule (not chatty, not
    // cloying, not clingy) but is nothing like Liri's warmth. The rule layer is
    // blind to this; only the embedding distinguishes the two.
    let off_persona = "行吧，随便你，我无所谓。";

    // Sanity: the rule layer genuinely cannot tell these apart (both clean).
    let rule_on = personality_drift_score(on_persona);
    let rule_off = personality_drift_score(off_persona);
    assert!(
        rule_on.violations.is_empty() && rule_off.violations.is_empty(),
        "both replies must pass the rule layer (else this tests the rules, not semantics): on={:?} off={:?}",
        rule_on.violations, rule_off.violations
    );

    let on_vec = svc.embed(on_persona).expect("embed on-persona reply");
    let off_vec = svc.embed(off_persona).expect("embed off-persona reply");

    let cos_on = cosine_similarity(&on_vec, &persona_vec);
    let cos_off = cosine_similarity(&off_vec, &persona_vec);
    let score_on = semantic_drift_score(&on_vec, &persona_vec);
    let score_off = semantic_drift_score(&off_vec, &persona_vec);

    println!("[semantic] on-persona  cosine={:.4} overall={:.3}", cos_on, score_on.overall);
    println!("[semantic] off-persona cosine={:.4} overall={:.3}", cos_off, score_off.overall);
    println!(
        "[semantic] rule layer gave both overall=1.0 (blind to tone) — semantic layer sees the gap"
    );

    // Headline: the warm reply is closer to the persona than the cold one.
    assert!(
        cos_on > cos_off,
        "on-persona reply cosine ({:.4}) must exceed off-persona ({:.4}) — semantic drift failed to rank tone",
        cos_on, cos_off
    );
    assert!(
        score_on.overall > score_off.overall,
        "on-persona overall ({:.3}) must exceed off-persona ({:.3})",
        score_on.overall, score_off.overall
    );
}
