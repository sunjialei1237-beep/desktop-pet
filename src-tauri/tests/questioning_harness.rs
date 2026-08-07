//! Questioning-behavior test for the NORMAL conversation path. Reproduces the
//! user-reported bug: "我在练卧推" and "今天好热" produced flat statements with
//! no follow-up question. Verifies the planner's new "engage" branch +
//! system.txt rules 2-5 produce a genuine follow-up question on shared
//! statements. Run: cargo test --test questioning_harness -- --nocapture
//! (stop dev server first).

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::mind::budget;
use desktop_pet_lib::mind::planner;
use desktop_pet_lib::mind::pacing::{throttle, QuestionPacing, ASK_THRESHOLD};
use desktop_pet_lib::mind::retrieval;

const BANNED: &[&str] = &[
    "有什么事吗", "需要帮忙", "我能帮你", "最近怎么样", "怎么样啦",
    "需要我做什么", "是吗", "哦", "嗯嗯",
];
fn empty_retrieval() -> retrieval::RetrievalResult {
    retrieval::RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None,
        persona_traits: vec![], user_profile: desktop_pet_lib::db::onboarding::UserProfile::default(),
    }
}

// (input, must be classified as a question by the planner).
const SHARE_INPUTS: &[&str] = &["我最近在练卧推", "今天也好热啊", "我开始学吉他了", "我有点累"];

#[tokio::test]
async fn shared_statements_get_engaged_questions() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    // Part A: planner classifies shared statements as "engage".
    let calm = desktop_pet_lib::emotion::state::EmotionState::default();
    for input in SHARE_INPUTS {
        let intent = planner::plan(input, &calm, None, &[], &empty_retrieval());
        assert_eq!(
            intent.goal, "engage",
            "planner should engage on shared statement {:?}, got goal={}", input, intent.goal
        );
    }
    println!("[planner] all {} shared inputs -> engage", SHARE_INPUTS.len());

    // Part B: planner does NOT engage on a direct question (must answer instead).
    let q_intent = planner::plan("你是谁？", &calm, None, &[], &empty_retrieval());
    assert_ne!(q_intent.goal, "engage", "planner must not engage on a question");
    println!("[planner] question input -> {} (not engage)", q_intent.goal);

    // Part C: live LLM — for the two reported inputs, the reply must contain a
    // genuine follow-up question (a question mark) and no banned phrase.
    let config = config::load_config().unwrap_or_default();
    let db_path = config::resolve_db_path(&config);
    let db = DbState::open(&db_path).expect("open db");
    let llm = LlmClient::new(
        &config.llm.base_url, &config.llm.api_key,
        &config.llm.main_model, &config.llm.reflection_model,
    ).expect("LLM configured");

    let db_emotion = db.with_conn(desktop_pet_lib::db::emotion::get).unwrap();
    let emotion = desktop_pet_lib::emotion::state::EmotionState {
        mood: db_emotion.mood, physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery, stress: db_emotion.stress,
        loneliness: db_emotion.loneliness, rest_need: db_emotion.rest_need,
    };

    for input in ["我最近在练卧推", "今天也好热啊"] {
        let retrieval = retrieval::retrieve(input, &emotion, None, &db, 3).unwrap();
        let intent = planner::plan(input, &emotion, None, &[], &retrieval);
        assert_eq!(intent.goal, "engage");
        let mut messages = budget::allocate_and_compress(&retrieval, &[], &emotion, &intent);
        messages.push(ChatMessage { role: "user".to_string(), content: input.to_string() });

        let chat = llm.chat(&messages, Some(0.8), Some(500)).await.expect("LLM call");
        let reply = chat.content.trim().to_string();
        println!("[engage] {:?} -> {:?}", input, reply);

        let has_q = reply.contains('？') || reply.contains('?');
        let banned_hit = BANNED.iter().find(|b| reply.contains(**b));
        assert!(has_q, "no follow-up question in reply to {:?}: {:?}", input, reply);
        assert!(banned_hit.is_none(), "banned phrase {:?} in reply to {:?}: {:?}", banned_hit, input, reply);
    }
    println!("[engage] both reported inputs got a genuine follow-up question");
}

/// Follow-up frequency control: when pacing ALLOWS (credit enough + not last
/// turn + winning roll), the planner's "engage" must survive; when pacing
/// DISALLOWS (credit low / back-to-back / losing roll), the converse layer
/// downgrades "engage" to "react". The planner itself is untouched (pure).
#[test]
fn pacing_throttle_suppresses_or_allows_engage() {
    // 1) ALLOW window: credit >= threshold, last turn not a question, winning roll.
    let charged = QuestionPacing { credit: 3, last_turn_was_question: false };
    assert!(charged.credit >= ASK_THRESHOLD);
    let (goal, next) = throttle("engage", &charged, 0.1);
    assert_eq!(goal, "engage", "allowed engage must stay engage");
    assert!(next.last_turn_was_question, "allowed turn marks last=true");

    // 2) SUPPRESS by low credit: even a winning roll downgrades to react.
    let low = QuestionPacing { credit: 1, last_turn_was_question: false };
    let (goal, _next) = throttle("engage", &low, 0.0);
    assert_eq!(goal, "react", "low credit must downgrade engage to react");

    // 3) SUPPRESS by back-to-back: previous turn asked -> forced react.
    let back_to_back = QuestionPacing { credit: 3, last_turn_was_question: true };
    let (goal, next) = throttle("engage", &back_to_back, 0.0);
    assert_eq!(goal, "react", "back-to-back engage must downgrade to react");
    assert!(!next.last_turn_was_question, "react clears the last flag");

    // 4) SUPPRESS by losing roll: enough credit, not last, but roll >= prob.
    let (goal, _next) = throttle("engage", &charged, 0.9);
    assert_eq!(goal, "react", "losing roll must downgrade engage to react");

    // 5) Non-engage goals are never throttled (planner's own goal preserved).
    let (goal, _next) = throttle("converse", &charged, 0.99);
    assert_eq!(goal, "converse", "non-engage goal passes through untouched");
    println!("[pacing] throttle allows/suppresses engage as designed");
}

