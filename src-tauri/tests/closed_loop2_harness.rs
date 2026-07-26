//! Closed-loop-2 harness: the pet proactively brings up a past plan when it
//! comes due. Proves `proactive::generate` (the engine behind the
//! `proactive_bubble` command) picks a due pending event as the memory anchor
//! and voices it via the LLM — the MVP "she remembers me" criterion.
//!
//! Seed a due pending event in a clean in-memory DB, call `proactive::generate`
//! with the real LLM, and verify: (1) a bubble is produced, (2) it is not
//! assistant-speak, (3) the pending event was marked triggered afterwards —
//! which ONLY happens in the due-pending branch, proving the anchor was the
//! user's past plan, not a generic greeting or fact/episode fallback.
//!
//! Complements proactive_harness (which checks the S1-S5 quality standards on
//! fact/episode anchors against the live DB). This one isolates the pending-due
//! mechanism on a seeded in-memory DB so it is repeatable and self-contained.
//!
//! Run: cargo test --test closed_loop2_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::pending::proactive;

const PENDING_TITLE: &str = "明天有个大公司的实习面试";

fn seed(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    // remind_date one day in the past → the event is due now (get_due checks
    // status='pending' AND remind_date <= now).
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO pending_events (id, title, event_date, remind_date, source_episode,
                status, importance, followup_count, created_at, triggered_at, resolved_at)
             VALUES (?1, ?2, ?3, ?4, NULL, 'pending', 0.8, 0, ?5, NULL, NULL)",
            rusqlite::params!["pe_seed_1", PENDING_TITLE, now, yesterday, now],
        )
        .map_err(|e| format!("seed pending failed: {}", e))?;
        Ok(())
    })
}

#[tokio::test]
async fn proactive_bubble_brings_up_due_pending() {
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
    .expect("LLM not configured — set API key in config.toml first");

    let db = test_db();
    seed(&db).expect("seed due pending event");

    let bubble = proactive::generate(&db, &llm, None, &[])
        .await
        .expect("proactive::generate errored — closed-loop-2 chain broken");

    // (1) A bubble was produced (not silent).
    let outcome = bubble.expect("proactive::generate returned None — no bubble for a due pending");
    let reply = outcome.reply;
    println!("proactive bubble: {:?}", reply);
    println!("anchored on: {:?}", outcome.anchor);

    // (2) Not assistant-speak (proactive-recall-standard S3).
    let assistant_speak = ["有什么事吗", "需要帮忙", "我能帮你", "有什么可以帮", "我能做些什么"];
    let bad = assistant_speak.iter().find(|s| reply.contains(*s));
    assert!(
        bad.is_none(),
        "reply is assistant-speak ('{}'): {}",
        bad.unwrap_or(&""),
        reply
    );

    // (3) The pending event was marked triggered — this ONLY happens in the
    //     due-pending branch of generate, proving the anchor was the user's
    //     past plan (not a fact/episode fallback).
    let still_pending: i64 = db
        .with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM pending_events WHERE id = 'pe_seed_1' AND status = 'pending'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
        })
        .unwrap_or(1);
    assert_eq!(
        still_pending, 0,
        "pending event was not marked triggered — generate did not take the due-pending branch"
    );

    // Soft check: does the reply literally reference the plan? LLM may rephrase
    // ("准备得怎么样啦"), so this is informational, not asserted.
    let references_plan = reply.contains("面试") || reply.contains("实习");

    println!("\n=== CLOSED-LOOP-2 RESULT ===");
    println!("bubble produced      : yes");
    println!("no assistant-speak   : {}", bad.is_none());
    println!("pending anchored     : {} (mark_triggered fired)", still_pending == 0);
    println!("reply mentions plan  : {} (soft — LLM may rephrase)", references_plan);
    println!("reply                : {:?}", reply);
}
