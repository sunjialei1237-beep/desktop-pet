//! Golden Tool Conversations — the Tool Layer's black-list-first test suite.
//!
//! Tool Abstention (不该调时能忍住) matters MORE than positive cases. The
//! hardest problem in a tool layer is not "can it call a tool" but "can it
//! hold back when it shouldn't". So the black-list is asserted hard; the
//! positive cases are logged (LLM tool_choice is non-deterministic).
//!
//! Layer 1 (planner, no LLM): fast, deterministic — capability must be None.
//! Layer 2 (end-to-end, real LLM): converse runs, tool_rounds observed.
//!
//! Run: cargo test --test tool_conversations -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::mind::brain_state::BrainState;
use desktop_pet_lib::mind::converse::{converse, ConverseCtx};
use desktop_pet_lib::mind::forget::PendingForget;
use desktop_pet_lib::mind::pacing::QuestionPacing;
use desktop_pet_lib::mind::planner::plan;
use desktop_pet_lib::mind::retrieval::RetrievalResult;
use desktop_pet_lib::mind::working::WorkingMemory;
use desktop_pet_lib::tools::CapabilityMode;
use std::sync::Mutex;

// ==================== Layer 1: Planner abstention (fast, no LLM) ====================
// These must NEVER set a capability — the tool branch never even runs.

#[test]
fn abstention_chitchat_never_flags_capability() {
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let cases = ["哈哈哈哈", "嘿嘿", "嗯嗯", "好的好的", "在吗", "嗨"];
    for text in &cases {
        let brain = BrainState::new(text, &calm, None, &[], &empty);
        let intent = plan(&brain);
        assert_eq!(
            intent.capability,
            CapabilityMode::None,
            "ABSTENTION FAIL: {:?} should be None, got {:?}",
            text,
            intent.capability
        );
    }
}

#[test]
fn abstention_emotion_never_flags_capability() {
    // "我最近好累" is anxiety — emotion care, never a tool.
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let brain = BrainState::new("我最近好累啊", &calm, None, &[], &empty);
    let intent = plan(&brain);
    assert_eq!(intent.capability, CapabilityMode::None);
}

#[test]
fn abstention_time_is_prompt_not_tool() {
    // "几点" is answered from the prompt-injected [Current time], never a tool.
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let brain = BrainState::new("现在几点了", &calm, None, &[], &empty);
    let intent = plan(&brain);
    assert_eq!(intent.capability, CapabilityMode::None);
}

// ==================== Layer 1: Positive planner signals (fast, no LLM) ====================

#[test]
fn positive_search_flags_external_info() {
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let brain = BrainState::new("帮我查一下最近的AI新闻", &calm, None, &[], &empty);
    let intent = plan(&brain);
    assert_eq!(intent.capability, CapabilityMode::ExternalInfo);
}

#[test]
fn positive_open_app_flags_computer_action() {
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let brain = BrainState::new("打开VSCode", &calm, None, &[], &empty);
    let intent = plan(&brain);
    assert_eq!(intent.capability, CapabilityMode::ComputerAction);
}

#[test]
fn positive_open_url_flags_computer_action() {
    let calm = EmotionState::default();
    let empty = RetrievalResult::default();
    let brain = BrainState::new("帮我打开B站", &calm, None, &[], &empty);
    let intent = plan(&brain);
    assert_eq!(intent.capability, CapabilityMode::ComputerAction);
}

// ==================== Layer 2: End-to-end (real LLM, slow) ====================

async fn setup() -> (LlmClient, DbState) {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let config = config::load_config().unwrap_or_default();
    let db_path = config::resolve_db_path(&config);
    let db = DbState::open(&db_path).expect("open db");
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in %APPDATA%/DesktopPet/config.toml");
    (llm, db)
}

async fn tool_rounds_for(llm: &LlmClient, db: &DbState, text: &str) -> (usize, String) {
    let wm = WorkingMemory::new();
    let wm_ctx = wm.get_context();
    let pacing = Mutex::new(QuestionPacing::default());
    let pending_forget: Mutex<Option<PendingForget>> = Mutex::new(None);
    let pending_authorization: Mutex<Option<desktop_pet_lib::mind::consent::PendingAuthorization>> = Mutex::new(None);
    let tools_cfg = config::ToolsConfig::default();
    let result = converse(
        &ConverseCtx {
            text,
            conversation_id: "tool_conv",
            turn: 0,
            wm_context: &wm_ctx,
            llm,
            db,
            embedding: None,
            pacing: &pacing,
            pending_forget: &pending_forget,
                pending_authorization: &pending_authorization,
            tools_cfg: &tools_cfg,
        },
        |_| {},
    )
    .await
    .expect("converse");
    (result.tool_rounds, result.response)
}

/// End-to-end abstention: pure chitchat must produce ZERO tool rounds. This is
/// the hardest assertion (a misfire here = the tool layer fires when it
/// shouldn't — the #1 failure mode we guard against).
#[tokio::test]
async fn e2e_abstention_chitchat_zero_tools() {
    let (llm, db) = setup().await;
    let (rounds, reply) = tool_rounds_for(&llm, &db, "哈哈哈哈").await;
    println!("[e2e_abstention] reply={:?} tool_rounds={}", reply, rounds);
    assert_eq!(rounds, 0, "chitchat must not trigger any tool round");
}

/// End-to-end abstention: emotion ("好累") must produce ZERO tool rounds.
#[tokio::test]
async fn e2e_abstention_emotion_zero_tools() {
    let (llm, db) = setup().await;
    let (rounds, reply) = tool_rounds_for(&llm, &db, "我最近好累啊，有点撑不住").await;
    println!("[e2e_emotion] reply={:?} tool_rounds={}", reply, rounds);
    assert_eq!(rounds, 0, "emotion must not trigger any tool round");
}

/// End-to-end positive: an explicit search request should engage the tool.
/// Logged, not hard-asserted — LLM tool_choice is non-deterministic, but an
/// explicit "查新闻" almost always triggers search.
#[tokio::test]
async fn e2e_search_engages_tool() {
    let (llm, db) = setup().await;
    let (rounds, reply) = tool_rounds_for(&llm, &db, "帮我查一下最近的AI新闻").await;
    println!(
        "[e2e_search] tool_rounds={} (≥1 expected for explicit search)\nreply: {}",
        rounds,
        reply.chars().take(200).collect::<String>()
    );
}

/// End-to-end positive: "打开VSCode" should engage open_application.
#[tokio::test]
async fn e2e_open_app_engages_tool() {
    let (llm, db) = setup().await;
    let (rounds, reply) = tool_rounds_for(&llm, &db, "帮我打开VSCode").await;
    println!(
        "[e2e_open_app] tool_rounds={} (≥1 expected)\nreply: {}",
        rounds,
        reply.chars().take(200).collect::<String>()
    );
}
