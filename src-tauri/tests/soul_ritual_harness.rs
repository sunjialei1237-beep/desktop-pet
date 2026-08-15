//! Ritual/landmark closure harness (第一梯队三连 verification).
//!
//! Locks down the three new Soul paths end-to-end with the REAL LLM and an
//! in-memory DB (repeatable, no pollution of live app_config):
//!   1. 晚安 — generate_goodnight produces a caring bedtime reply, and the
//!      goodnight mark makes the gate silent for the rest of the day.
//!   2. 周日总结 — generate_weekly_summary recaps a seeded week (mentions the
//!      week's real content), and week_has_content stays false on an empty DB.
//!   3. 里程碑 — a seeded first_met 31 days ago produces a "认识满 30 天"
//!      celebration, the milestone is recorded, and due_landmark goes None
//!      (fires exactly once).
//!
//! Run: cargo test --test soul_ritual_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::soul::{landmark, ritual, weekly};

fn make_llm() -> LlmClient {
    let config = config::load_config().unwrap_or_default();
    LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in config.toml first")
}

/// Seeds a small week of memories (episodes + facts, all within 7 days).
fn seed_week(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        for (id, summary) in [
            ("ep_w1", "和糯米去猫咖撸猫"),
            ("ep_w2", "面试了一家公司"),
            ("ep_w3", "深夜赶完了一个大作业"),
        ] {
            conn.execute(
                "INSERT INTO episodes (id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type, source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at, consolidated, created_at)
                 VALUES (?1, ?2, ?3, '开心', 0.6, 0, 'user', NULL, NULL, 'conversation', NULL, NULL,
                    0.6, 0, NULL, 0, ?2)",
                rusqlite::params![id, now, summary],
            )
            .map_err(|e| format!("seed episode {} failed: {}", id, e))?;
        }
        conn.execute(
            "INSERT INTO facts (id, category, key, value, confidence, valid_from, valid_to,
                source_episode, mention_count, created_at, updated_at, surfaced_count, last_surfaced_at)
             VALUES ('f_w1', 'preference', '宠物', '最爱糯米', 0.9, ?1, NULL, NULL, 2, ?1, ?1, 0, NULL)",
            rusqlite::params![now],
        )
        .map_err(|e| format!("seed fact failed: {}", e))?;
        Ok(())
    })
}

#[tokio::test]
async fn goodnight_produces_caring_reply_and_marks_silent() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let db = test_db();
    let llm = make_llm();

    // Never fired => due; generate, then the gate must be silent for today.
    assert!(
        db.with_conn(|conn| Ok(ritual::should_run_goodnight(conn))).unwrap(),
        "goodnight is due on a clean DB"
    );
    let outcome = ritual::generate_goodnight(&db, &llm, None, &[])
        .await
        .expect("generate_goodnight errored");
    let o = outcome.expect("goodnight returned None — grounding_guard suppressed everything");
    println!("goodnight reply: {:?}", o.reply);

    // Soft voice check: a bedtime line should feel like one (LLM may rephrase).
    let has_bedtime_feel = ["睡", "晚安", "熬夜", "休息", "身体"]
        .iter()
        .any(|k| o.reply.contains(k));
    println!("bedtime-voice soft check: {}", has_bedtime_feel);

    db.with_conn(|conn| {
        ritual::mark_goodnight_done(conn)?;
        assert!(!ritual::should_run_goodnight(conn), "marked => silent today");
        assert!(ritual::goodnight_done_today(conn));
        Ok::<_, String>(())
    })
    .unwrap();
}

#[tokio::test]
async fn weekly_summary_recaps_the_week_and_empty_week_stays_silent() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let db = test_db();

    // Empty DB: nothing to recap (the loop would mark done and stay quiet).
    assert!(!weekly::week_has_content(&db), "empty week is silent");

    seed_week(&db).expect("seed week failed");
    assert!(weekly::week_has_content(&db), "seeded week has content");

    let llm = make_llm();
    let o = weekly::generate_weekly_summary(&db, &llm, &[])
        .await
        .expect("generate_weekly_summary errored")
        .expect("weekly summary returned None for a content-filled week");
    println!("weekly summary reply: {:?}", o.reply);

    // Soft check: the recap should reference the week's real content (the LLM
    // picks what to mention; any one of the seeded topics counts).
    let mentions_week = ["糯米", "猫", "面试", "作业", "实习"]
        .iter()
        .any(|k| o.reply.contains(k));
    println!("mentions-week soft check: {}", mentions_week);

    // Hard check: NO fabrication of unseeded specifics (grounding_guard's job
    // — a made-up person/topic in a weekly recap is the worst failure mode).
    for k in ["篮球", "星际穿越", "火锅"] {
        assert!(
            !o.reply.contains(k),
            "weekly recap fabricated unseeded content '{}': {}",
            k,
            o.reply
        );
    }
}

#[tokio::test]
async fn landmark_celebrates_once_then_goes_silent() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let db = test_db();

    // Seed a first meeting 31 days ago via the change_log backfill source.
    let early = (chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339();
    db.with_conn(|conn| {
        desktop_pet_lib::db::changelog::append(
            conn, "facts", "insert", None, None, None, None, None,
        )?;
        conn.execute(
            "UPDATE change_log SET timestamp = ?1 WHERE id = (SELECT MIN(id) FROM change_log)",
            rusqlite::params![early],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .unwrap();

    let due = db
        .with_conn(|conn| Ok(landmark::due_landmark(conn)))
        .unwrap()
        .expect("31 days => a landmark is due");
    assert_eq!(due.id, "days:30", "largest covered milestone is 30");
    println!("landmark due: {}", due.description);

    seed_week(&db).expect("seed week failed");
    let llm = make_llm();
    let o = landmark::generate_landmark(&db, &llm, None, &[], &due)
        .await
        .expect("generate_landmark errored")
        .expect("landmark returned None");
    println!("landmark reply: {:?}", o.reply);

    // Soft voice check: celebration mentions the milestone number.
    let mentions_milestone = o.reply.contains("30") || o.reply.contains("认识");
    println!("milestone-voice soft check: {}", mentions_milestone);

    // Hard checks: exactly once, and the smaller milestone never resurfaces.
    db.with_conn(|conn| {
        landmark::mark_landmark_celebrated(conn, &due.id)?;
        assert_eq!(landmark::due_landmark(conn), None, "celebrated once => silent");
        Ok::<_, String>(())
    })
    .unwrap();
}
