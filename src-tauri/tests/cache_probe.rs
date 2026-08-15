//! B14 缓存实测（Soul v2 方案 P4）：验证近端拆分让静态前缀真的命中
//! DeepSeek 前缀缓存。两组：
//! ① v2 布局（近端开）：同一会话连打 3 轮，第 2/3 轮的 prompt_cache_hit
//!    应接近静态 system 前缀大小（~2500 token），证明 [static, history...] 前缀稳定。
//! ② v1 布局模拟（时间塞回 system 第二段）：两次调用间隔 65s 跨分钟边界，
//!    第二次命中应显著小于完整前缀（时间每分钟变化 → 其后全部 miss）。
//! Run: cargo test --test cache_probe -- --ignored --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};

#[tokio::test]
#[ignore]
async fn cache_probe_near_end_vs_inline_time() {
    let cfg = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &cfg.llm.base_url,
        &cfg.llm.api_key,
        &cfg.llm.main_model,
        &cfg.llm.reflection_model,
    )
    .expect("LLM not configured");

    // ---- ① v2 layout: near-end on (production default) --------------------
    desktop_pet_lib::mind::budget::set_near_end_enabled(true);
    let persona = "你是璃，一只住在用户屏幕上的小狐灵桌宠，话不多，像朋友随手发消息。".to_string();
    let mut history: Vec<ChatMessage> = vec![ChatMessage::system(persona.clone())];
    let turns = ["今天好累", "我最近在学摄影", "外面下雨了"];
    let mut last_hit: Option<u32> = None;
    let mut last_miss: Option<u32> = None;
    for (i, t) in turns.iter().enumerate() {
        history.push(ChatMessage::user(t.to_string()));
        let mut messages = history.clone();
        messages.push(ChatMessage::system(format!(
            "[Current time]\n现在 0{}:{} 周六 2026-08-15\n时段：晚上\n\n[Current Mood] 平静\n\n[Intent] goal: converse",
            8 + i, 10 + i
        )));
        let r = llm.chat(&messages, Some(0.8), Some(2048), None).await.expect("chat");
        println!(
            "[v2 turn {}] prompt={} cache_hit={:?} cache_miss={:?}",
            i + 1, r.prompt_tokens, r.prompt_cache_hit_tokens, r.prompt_cache_miss_tokens
        );
        history.push(ChatMessage::assistant(r.content.trim().to_string()));
        last_hit = r.prompt_cache_hit_tokens;
        last_miss = r.prompt_cache_miss_tokens;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    println!(
        "v2 layout final turn: hit={:?} miss={:?} (期望 hit>0 且随轮次增长——静态前缀+早期历史命中)",
        last_hit, last_miss
    );

    // ---- ② v1-style: time inlined into the system slot 2 -------------------
    // Same static-ish prefix but with the CURRENT time string embedded (minute
    // resolution). Two calls 65s apart cross a minute boundary; the second
    // call's prefix diverges at the time section, so hits should collapse to 0
    // (everything after the divergence point is a miss).
    let now2 = || chrono::Local::now().format("%H:%M").to_string();
    let sys = |t: String| {
        ChatMessage::system(format!(
            "{persona}\n\n[Current time]\n现在 {t} 周六 2026-08-15\n时段：晚上\n\n[Current Mood] 平静"
        ))
    };
    let r1 = llm
        .chat(&[sys(now2()), ChatMessage::user("在干嘛".to_string())], Some(0.8), Some(2048), None)
        .await
        .expect("chat1");
    println!(
        "[v1-style call1] hit={:?} miss={:?}",
        r1.prompt_cache_hit_tokens, r1.prompt_cache_miss_tokens
    );
    tokio::time::sleep(std::time::Duration::from_secs(65)).await;
    let r2 = llm
        .chat(&[sys(now2()), ChatMessage::user("在干嘛".to_string())], Some(0.8), Some(2048), None)
        .await
        .expect("chat2");
    println!(
        "[v1-style call2 (跨分钟)] hit={:?} miss={:?} (期望 hit≈0——时间在第二段变分钟后其后全 miss)",
        r2.prompt_cache_hit_tokens, r2.prompt_cache_miss_tokens
    );
}
