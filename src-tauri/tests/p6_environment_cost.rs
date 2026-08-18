//! P6 A/B/C 成本基线（plan 2026-08-17 §6，真实 LLM）。
//!
//! A = 无环境注入；B = 每轮都注入完整 [Environment] section；
//! C = relevance gate 命中才注入（生产路径）。
//!
//! 测法：同一 query 的三组消息用同一个 reflection 模型各打一次，比较
//! provider 返回的 prompt_tokens / prompt_cache_hit_tokens。用量是确定性
//! 字段，因此 C 的结构性成本承诺可以直接硬断言：
//!   - 闲聊轮：C 的 prompt 成本 == A（gate 拦住了环境注入）。
//!   - 环境相关轮：C 的 prompt 成本 == B（该注入时和 B 一样完整）。
//! 从而整体成本介于 A 与 B 之间，且只在真正需要时支付。
//!
//! 运行：cargo test --test p6_environment_cost -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::llm::client::{ChatMessage, ChatResult, LlmClient};
use desktop_pet_lib::mind::grounding;
use desktop_pet_lib::mind::planner;
use desktop_pet_lib::mind::planner::Intent;
use desktop_pet_lib::mind::retrieval::RetrievalResult;
use desktop_pet_lib::perception::environment::{render_environment_section, EnvHints};

/// Deterministic, realistic mid-session environment snapshot for the harness.
fn sample_section() -> String {
    render_environment_section(
        &EnvHints {
            app: Some("code.exe".into()),
            title: Some("agent.rs — desktop-pet".into()),
            file_hint: Some("agent.rs".into()),
            project_hint: Some("Liri".into()),
            root: None,
        },
        false,
        Some("agent.rs → planner.rs"),
    )
}

fn base_messages() -> Vec<ChatMessage> {
    let empty = RetrievalResult::default();
    vec![ChatMessage::system(grounding::build_system_prompt(
        &empty,
        &EmotionState::default(),
        &Intent::default(),
    ))]
}

fn with_user(mut base: Vec<ChatMessage>, text: &str) -> Vec<ChatMessage> {
    base.push(ChatMessage::user(text));
    base
}

async fn call(llm: &LlmClient, messages: Vec<ChatMessage>) -> ChatResult {
    llm.chat_reflection(&messages, Some(0.7), Some(64))
        .await
        .expect("P6 cost call failed")
}

fn usage_line(tag: &str, r: &ChatResult) -> String {
    format!(
        "{tag}: prompt={} completion={} cache_hit={:?} cache_miss={:?}",
        r.prompt_tokens, r.completion_tokens, r.prompt_cache_hit_tokens, r.prompt_cache_miss_tokens
    )
}

#[tokio::test]
async fn p6_cost_abc_baseline() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let cfg = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &cfg.llm.base_url,
        &cfg.llm.api_key,
        &cfg.llm.main_model,
        &cfg.llm.reflection_model,
    )
    .expect("LLM not configured — set api_key in %APPDATA%/DesktopPet/config.toml");

    let env = sample_section();
    let relevant_text = "我在写什么？帮我看看现在这个项目怎么样了";
    let chitchat_text = "哈哈哈哈";

    // Same pure gate the production converse() uses (plan §2.3 layer 2).
    assert!(
        planner::environment_relevant(relevant_text, &Intent::default()),
        "fixture must hit the relevance gate"
    );
    assert!(
        !planner::environment_relevant(chitchat_text, &Intent::default()),
        "fixture chitchat must NOT hit the gate"
    );

    let base = base_messages();
    println!("\n=== P6 A/B/C cost baseline ===");
    println!("env section: {} chars", env.chars().count());

    // Relevant query: A（无 env）vs B（必带 env）vs C（gate 命中 → 带 env）。
    let a_rel = call(&llm, with_user(base.clone(), relevant_text)).await;
    let mut b_rel = base.clone();
    b_rel.push(ChatMessage::system(env.clone()));
    let b_rel = call(&llm, with_user(b_rel, relevant_text)).await;
    let mut c_rel = base.clone();
    c_rel.push(ChatMessage::system(env.clone()));
    let c_rel = call(&llm, with_user(c_rel, relevant_text)).await;

    println!("[relevant ] {}", usage_line("A", &a_rel));
    println!("[relevant ] {}", usage_line("B", &b_rel));
    println!("[relevant ] {}", usage_line("C", &c_rel));

    // Chitchat: C 必须和 A 完全同价（gate 拦住），B 是多花的那个。
    let a_chat = call(&llm, with_user(base.clone(), chitchat_text)).await;
    let mut b_chat = base.clone();
    b_chat.push(ChatMessage::system(env.clone()));
    let b_chat = call(&llm, with_user(b_chat, chitchat_text)).await;
    let c_chat = call(&llm, with_user(base.clone(), chitchat_text)).await;

    println!("[chitchat ] {}", usage_line("A", &a_chat));
    println!("[chitchat ] {}", usage_line("B", &b_chat));
    println!("[chitchat ] {}", usage_line("C", &c_chat));

    // Hard assertions: the gate makes C pay A's price on chitchat and B's
    // price exactly when the environment matters.
    assert_eq!(
        c_chat.prompt_tokens, a_chat.prompt_tokens,
        "chitchat: gate must keep C at A's prompt cost"
    );
    assert!(b_chat.prompt_tokens > a_chat.prompt_tokens, "B must cost more than A");
    assert_eq!(
        c_rel.prompt_tokens, b_rel.prompt_tokens,
        "relevant query: C must pay the same env cost as B"
    );
    assert!(b_rel.prompt_tokens > a_rel.prompt_tokens, "env section must have positive prompt cost");

    let chat_saved = b_chat.prompt_tokens.saturating_sub(a_chat.prompt_tokens) as u64;
    let rel_cost = b_rel.prompt_tokens.saturating_sub(a_rel.prompt_tokens) as u64;
    println!(
        "\n[P6] env section prompt cost ≈ {rel_cost} tokens (rel) / {chat_saved} (chat); gate saves ~{chat_saved} tokens per non-environment turn"
    );
}