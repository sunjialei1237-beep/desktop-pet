//! Thought-stream end-to-end harness (v3 方案 P4-lite 实机验证).
//!
//! Runs the REAL pipeline — ingest / gate / motivation / flash evaluation /
//! unified renderer / occasion folding / thought re-voicing — against the
//! REAL configured LLM (set API key in AppData config.toml), on a clean
//! in-memory DB seeded with a small relationship. Gates are RESET between
//! scenarios to accelerate surfacing (user-approved: 可以加速浮现的时间,
//! 必须实机), but every scenario prints the gate verdict first so the real
//! machine's signal (quiet hours / presence / focus) is still observable.
//!
//! Output (with --nocapture): per-scenario decision chain + every voiced
//! line + the full seed pool + bubble_log — the raw material for the
//! naturalness/logic review.
//!
//! Run: cargo test --test thought_stream_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::soul::stream;

fn seed(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let three_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
    db.with_conn(|conn| {
        for (id, summary, emotion, importance) in [
            ("ep_s1", "用户说最近在找实习，约了周四面试", "期待", 0.8f64),
            ("ep_s2", "用户和朋友去吃了火锅，很开心", "开心", 0.5),
            ("ep_s3", "用户提到喜欢喝奶茶，三分糖", "平静", 0.4),
        ] {
            conn.execute(
                "INSERT INTO episodes (id, time, summary, emotion, importance, is_landmark, subject,
                    source_type, memory_strength, recall_count, consolidated, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 'user', 'conversation', 0.7, 0, 0, ?2)",
                rusqlite::params![id, now, summary, emotion, importance],
            )
            .map_err(|e| format!("seed episode: {}", e))?;
        }
        conn.execute(
            "INSERT INTO facts (id, category, key, value, confidence, mention_count, surfaced_count,
                created_at, updated_at)
             VALUES ('f_s1', 'goal', '近况', '正在找实习，周四面试', 0.9, 2, 0, ?1, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e| format!("seed fact: {}", e))?;
        // A DUE pending event ("明天有面试" — due now) for the time-trigger scenario.
        conn.execute(
            "INSERT INTO pending_events (id, title, event_date, remind_date, status, importance,
                followup_count, created_at)
             VALUES ('pd_s1', '实习面试', ?1, ?1, 'pending', 0.8, 0, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e| format!("seed pending: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO relationship (id, closeness, trust, days_known,
                total_conversations, shared_events, last_interaction_at, updated_at)
             VALUES (1, 70.0, 60.0, 30, 42, 3, ?1, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e| format!("seed relationship: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO emotion_state (id, mood, mood_label, physical_energy,
                social_battery, stress, loneliness, rest_need, last_homeostasis_at, updated_at)
             VALUES (1, 0.62, '平静', 0.7, 0.65, 0.3, 0.55, 0.3, ?1, ?1)",
            rusqlite::params![now],
        )
        .map_err(|e| format!("seed emotion: {}", e))?;
        // Last interaction 3h ago so skip_recent stays open at scenario start.
        conn.execute(
            "UPDATE relationship SET last_interaction_at = ?1",
            rusqlite::params![three_hours_ago],
        )
        .map_err(|e| format!("seed last_interaction: {}", e))?;
        Ok::<_, String>(())
    })
}

/// Acceleration: reset the budget + interaction clock between scenarios.
fn reset_gates(db: &DbState) {
    let past = (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
    let _ = db.with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO app_config (key, value) VALUES ('last_proactive_bubble_at', ?1)",
            rusqlite::params![past],
        )
        .map_err(|e| e.to_string())
    });
    let _ = db.with_conn(|conn| {
        conn.execute(
            "UPDATE relationship SET last_interaction_at = ?1",
            rusqlite::params![past],
        )
        .map_err(|e| e.to_string())
    });
}

fn print_gate(db: &DbState) {
    let cfg = config::ProactiveConfig::default();
    let verdict = stream::gate(db, &cfg, &chrono::Utc::now());
    match verdict {
        stream::GateVerdict::Proceed => println!("[gate] Proceed"),
        stream::GateVerdict::Silent(r) => println!("[gate] Silent ({}) — 本场景 tick 将静默", r),
    }
}

#[tokio::test]
async fn thought_stream_scenarios() {
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
    seed(&db).expect("seed");

    let mut voiced: Vec<String> = Vec::new();

    // --- S1 环境碎碎念（poll 路径）---
    println!("\n=== S1 环境碎碎念（tick 全链路）===");
    stream::ingest(
        &db,
        Some("正在编辑：stream.rs（desktop-pet 项目），已连续 40 分钟"),
        &chrono::Utc::now(),
    );
    reset_gates(&db);
    print_gate(&db);
    match stream::tick(&db, &llm, None, &config::ProactiveConfig::default()).await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」(锚: {})", o.reply, o.anchor);
            voiced.push(format!("S1 环境碎碎念: {}", o.reply));
        }
        Ok(None) => println!("[voice] （本轮沉默——评估拒绝或分数不足）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S2 欢迎回来 ---
    println!("\n=== S2 欢迎回来（occasion folding）===");
    match stream::occasion_bubble(&db, &llm, "welcome", "ta 离开了 120 分钟，刚刚回来").await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」", o.reply);
            voiced.push(format!("S2 欢迎回来: {}", o.reply));
        }
        Ok(None) => println!("[voice] （评估拒绝——将回退罐头句）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S3 孤独戳一下 ---
    println!("\n=== S3 孤独戳一下（occasion folding）===");
    match stream::occasion_bubble(&db, &llm, "lonely", "一个人待了一会儿，有点想 ta；ta 就在旁边但没说话").await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」", o.reply);
            voiced.push(format!("S3 孤独戳一下: {}", o.reply));
        }
        Ok(None) => println!("[voice] （评估拒绝——将回退罐头句）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S4 早安 ---
    println!("\n=== S4 早安（occasion folding）===");
    match stream::occasion_bubble(&db, &llm, "goodmorning", "今天第一次见到 ta，新的一天开始了").await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」", o.reply);
            voiced.push(format!("S4 早安: {}", o.reply));
        }
        Ok(None) => println!("[voice] （评估拒绝——将回退罐头句）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S5 晚安 ---
    println!("\n=== S5 晚安（occasion folding）===");
    match stream::occasion_bubble(&db, &llm, "goodnight", "这一天要结束了，夜里该休息了").await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」", o.reply);
            voiced.push(format!("S5 晚安: {}", o.reply));
        }
        Ok(None) => println!("[voice] （评估拒绝——将回退罐头句）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S6 到期提醒（时间触发：due pending → seed → tick）---
    println!("\n=== S6 到期提醒（pending 时间触发 → tick）===");
    stream::ingest(&db, None, &chrono::Utc::now());
    reset_gates(&db);
    print_gate(&db);
    match stream::tick(&db, &llm, None, &config::ProactiveConfig::default()).await {
        Ok(Some(o)) => {
            println!("[voice] 「{}」(锚: {})", o.reply, o.anchor);
            voiced.push(format!("S6 到期提醒: {}", o.reply));
        }
        Ok(None) => println!("[voice] （本轮沉默）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- S7 反思念头再表达 ---
    println!("\n=== S7 反思念头再表达（voice_thought）===");
    match desktop_pet_lib::soul::monologue::voice_thought(
        &db,
        &llm,
        "自己话比平时少，安安静静待着也挺好",
    )
    .await
    {
        Ok(Some(r)) => {
            println!("[voice] 「{}」", r);
            voiced.push(format!("S7 念头再表达: {}", r));
        }
        Ok(None) => println!("[voice] （再表达被抑制）"),
        Err(e) => println!("[voice] Err: {}", e),
    }

    // --- 汇总：种子池 + 气泡日志 ---
    println!("\n=== 种子池（thought_stream snapshot）===");
    for s in stream::snapshot(&db, 30) {
        println!(
            "[{}] origin={} salience={:.2} | {}{}",
            s.state,
            s.origin,
            s.salience,
            s.stimulus,
            s.unspoken_reason
                .as_deref()
                .map(|r| format!("（忍住：{}）", r))
                .unwrap_or_default()
        );
    }
    println!("\n=== bubble_log ===");
    let logs = db
        .with_conn(|conn| desktop_pet_lib::db::bubble_log::get_recent(conn, 20))
        .unwrap_or_default();
    for l in logs.iter().rev() {
        println!("[{}] {}", l.kind, l.text);
    }
    println!("\n=== 实机冒泡共 {} 条 ===", voiced.len());
    for v in &voiced {
        println!("· {}", v);
    }

    // --- 轻量断言（LLM 随机性下只锁硬性不变式）---
    assert!(!voiced.is_empty(), "至少应有场景开口（welcome/goodmorning 几乎必说）");
    for v in &voiced {
        let line = v.split(": ").nth(1).unwrap_or(v);
        assert!(!line.trim().is_empty(), "开口不能为空串: {:?}", v);
        assert!(line.chars().count() <= 100, "单气泡应保持一句话（≤100字）: {:?}", line);
    }
}
