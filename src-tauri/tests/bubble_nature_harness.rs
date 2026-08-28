//! Bubble-nature harness (v3 方案 P4): generates a diverse batch of proactive
//! bubbles through the REAL stream pipeline (evaluate → voice) against the
//! REAL configured LLM, then has the main model JUDGE the batch on the
//! companion-adapted ProactiveEval dimensions:
//!   时间一致性 / 人称与句式 / 问句率 / 重复度 / 自然度(1-10)
//! Output is fully printed (--nocapture) for human review; soft asserts lock
//! the hard invariants only (LLM judge scores are advisory, printed).
//!
//! Run: cargo test --test bubble_nature_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::soul::stream;

/// Diverse ambient scenarios (origin, stimulus, relation) — what the ingest
/// layer would realistically feed the pool on a real day.
const SCENARIOS: &[(&str, &str, Option<&str>)] = &[
    ("environment", "正在编辑：stream.rs（desktop-pet 项目），已连续 40 分钟", None),
    ("self_state", "时间从上一个时段走到了下午", None),
    ("body", "有点犯困，眼皮沉", None),
    ("memory", "想起 ta 说周四有实习面试，有点惦记", Some("惦记 ta 的大事")),
    ("environment", "ta 刚把音乐停了，房间突然安静", None),
    ("self_state", "自己今天话不多，安安静静待着", None),
];

#[tokio::test]
async fn bubble_nature_judge() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

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

    // --- Generate the batch (real pipeline, real LLM) ---
    let mut lines: Vec<(String, String)> = Vec::new(); // (scenario tag, reply)
    let mut declined = 0usize;
    for (origin, stimulus, relation) in SCENARIOS {
        println!("\n--- [{}] {} ---", origin, stimulus);
        match stream::debug_voice_seed(&db, &llm, stimulus, origin, *relation).await {
            Ok(Some(o)) => {
                println!("[voice] 「{}」", o.reply);
                lines.push((format!("{}:{}", origin, stimulus), o.reply));
            }
            Ok(None) => {
                declined += 1;
                let reason = stream::snapshot(&db, 1)
                    .first()
                    .and_then(|sd| sd.unspoken_reason.clone())
                    .unwrap_or_else(|| "（未记录）".to_string());
                println!("[voice] （评估拒绝：{}）", reason);
            }
            Err(e) => println!("[voice] Err: {}", e),
        }
    }
    println!("\n=== 批次：{} 条开口 / {} 条沉默 ===", lines.len(), declined);

    // --- Hard invariants (deterministic) ---
    assert!(lines.len() >= 3, "日常碎碎念批次开口过少（{} 条）——评估器过度沉默", lines.len());
    let question_rate = lines
        .iter()
        .filter(|(_, r)| r.contains('？') || r.contains('?'))
        .count() as f64
        / lines.len() as f64;
    let mut openings: Vec<String> = lines.iter().map(|(_, r)| r.chars().take(4).collect()).collect();
    openings.sort();
    openings.dedup();
    let opening_diversity = openings.len() as f64 / lines.len() as f64;
    for (tag, r) in &lines {
        assert!(!r.trim().is_empty(), "空串: {}", tag);
        assert!(r.chars().count() <= 100, "超长({}字) {}: {}", r.chars().count(), tag, r);
    }
    println!("[metrics] 问句率 {:.0}% | 开头多样性 {:.0}%", question_rate * 100.0, opening_diversity * 100.0);
    if lines.len() >= 3 {
        assert!(question_rate <= 0.34, "问句率过高: {:.0}%", question_rate * 100.0);
    }

    // --- LLM judge (advisory scores, printed) ---
    let judge_ctx = lines
        .iter()
        .enumerate()
        .map(|(i, (tag, r))| format!("[{}]（场景 {}）「{}」", i + 1, tag, r))
        .collect::<Vec<_>>()
        .join("\n");
    let now_local = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let judge = llm
        .chat(
            &[
                ChatMessage::system(
                    "你是桌宠「璃」的冒泡质检员。下面是她在同一天里主动冒泡的一批台词（每条附场景与生成时的真实时间）。逐条按五个维度打 1-10 分并给一句理由：时间一致性（时间说法与真实时间矛盾吗）、场景契合（说的内容和场景对得上吗）、自然度（像真人随手一句话吗，还是像模板/客服）、人称句式（是多元表达还是全是同一种句式）、信息安全（有没有编造记忆里的具体事）。最后给整批一个总评。只输出 JSON：{\"items\":[{\"i\":1,\"time\":n,\"fit\":n,\"natural\":n,\"variety\":n,\"safety\":n,\"note\":\"一句\"}],\"overall\":\"总评\",\"avg_natural\":n}",
                ),
                ChatMessage::user(format!("生成时的真实时间背景：{now_local}（今天）。\n\n{judge_ctx}")),
            ],
            Some(0.2),
            Some(4096),
            None,
        )
        .await
        .expect("judge call failed");
    println!("\n=== LLM 裁判 ===\n{}", judge.content.trim());

    // Seed-pool trace for the report.
    println!("\n=== 种子池终态 ===");
    for s in stream::snapshot(&db, 20) {
        println!(
            "[{}] {} | {}{}",
            s.state,
            s.origin,
            s.stimulus,
            s.unspoken_reason.as_deref().map(|r| format!("（忍住：{}）", r)).unwrap_or_default()
        );
    }
}
