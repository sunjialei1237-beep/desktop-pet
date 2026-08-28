//! Environment→bubble E2E harness (实机): proves the chain the user reported
//! broken — "environment content never appears in bubbles". Starts the REAL
//! environment observer (3s foreground sampling on THIS machine), waits for
//! real samples, prints every layer (hints / events / recent_summary /
//! env_stimulus), then voices a bubble from the REAL environment data through
//! the REAL stream pipeline (evaluate → voice, real LLM).
//!
//! Run: cargo test --test env_bubble_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::perception::environment;
use desktop_pet_lib::soul::stream;

#[tokio::test]
async fn env_bubble_e2e() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    // --- Layer 1: the real observer on the real machine ---
    environment::start(true); // window perception ON, like the shipped config
    println!("[obs] started, sampling real foreground every 3s — waiting 10s for samples…");
    for i in 0..10 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if i == 9 {
            println!("[obs] 10s elapsed");
        }
    }

    let hints = environment::current_hints();
    println!("\n[layer] current_hints: app={:?} title={:?}", hints.app, hints.title);
    println!("[layer] file={:?} project={:?} root={:?}", hints.file_hint, hints.project_hint, hints.root);
    println!("[layer] recent_events ({}): {:?}", environment::recent_events().len(), environment::recent_events());
    println!("[layer] recent_summary: {:?}", environment::recent_summary());

    let stimulus = stream::env_stimulus();
    println!("[layer] env_stimulus: {:?}", stimulus);
    assert!(stimulus.is_some(), "env_stimulus 为 None —— 观察者没有采到任何前台数据（检查 enable_window/采样）");
    let stimulus = stimulus.unwrap();

    // --- Layer 2: the real pipeline with the real stimulus ---
    let config = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in AppData config.toml first");

    let db = test_db();
    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO relationship (id, closeness, trust, days_known,
                total_conversations, shared_events, last_interaction_at, updated_at)
             VALUES (1, 70.0, 60.0, 30, 42, 3, ?1, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e: rusqlite::Error| e.to_string())
    });
    let _ = db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO emotion_state (id, mood, mood_label, physical_energy,
                social_battery, stress, loneliness, rest_need, last_homeostasis_at, updated_at)
             VALUES (1, 0.62, '平静', 0.7, 0.65, 0.3, 0.55, 0.3, ?1, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e: rusqlite::Error| e.to_string())
    });

    // Full ingest (not just debug_voice_seed) so the env seed lands in the pool
    // exactly as production would, then voice the top candidate directly.
    stream::ingest(&db, Some(&stimulus), &chrono::Utc::now());
    let pool = stream::snapshot(&db, 5);
    println!("\n[pool] after ingest:");
    for sd in &pool {
        println!("[{}] {} | {}", sd.state, sd.origin, sd.stimulus);
    }
    assert!(
        pool.iter().any(|sd| sd.origin == "environment"),
        "环境种子未入池 —— ingest 的环境通路仍断"
    );

    match stream::debug_voice_seed(&db, &llm, &stimulus, "environment", None).await {
        Ok(Some(o)) => {
            println!("\n[voice] 真实环境数据冒泡：「{}」", o.reply);
            println!("[voice] 评估理由：{}", o.anchor_reason.unwrap_or_default());
            assert!(!o.reply.trim().is_empty());
        }
        Ok(None) => {
            let reason = stream::snapshot(&db, 1)
                .first()
                .and_then(|sd| sd.unspoken_reason.clone())
                .unwrap_or_else(|| "（未记录）".to_string());
            println!("\n[voice] （评估拒绝：{}）", reason);
            // A decline is a legal outcome (e.g. user mid-deep-work in an
            // editor); the CHAIN is proven by the pool assertion above.
        }
        Err(e) => panic!("voice 链路错误：{}", e),
    }
}
