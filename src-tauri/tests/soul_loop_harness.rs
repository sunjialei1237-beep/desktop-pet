//! Soul slow-loop closure harness (P13 / P15 verification).
//!
//! Locks down the two new code paths added this round:
//!   1. `maybe_run_if_due` — the slow_tick scheduling function. With no prior
//!      reflection it runs one end-to-end and persists a reflections row.
//!      (The 20h cooldown branch is unit-tested in reflection.rs.)
//!   2. `generate_welcome_back` consumes a surfaced thought — pre-insert a
//!      next_interaction thought ("what last night's reflection left for next
//!      time the user shows up"), call welcome-back, assert the thought is now
//!      marked surfaced AND a reply came back. This is the "隔夜回来她说出
//!      昨晚念头" path (Design 7.1 / P13.2).
//!
//! Uses the REAL LLM (config.toml) but an in-memory DB — repeatable, no
//! pollution of live reflections / traits / thoughts.
//!
//! Run: cargo test --test soul_loop_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::reflections::{self, InternalThought};
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::pending::proactive;
use desktop_pet_lib::soul::reflection;

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

/// Seeds an in-memory DB with a few realistic interactions so reflection and
/// retrieval have something concrete to work with. (Same seed as soul_harness.)
fn seed(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    db.with_conn(|conn| {
        for (id, summary, emotion, importance, strength) in [
            ("ep_seed_1", "用户说最近在找实习，语气有点焦虑", "担心", 0.8f64, 0.8f64),
            ("ep_seed_2", "用户和朋友一起去吃了火锅，很开心", "开心", 0.6, 0.7),
            ("ep_seed_3", "用户提到喜欢喝奶茶", "平静", 0.4, 0.5),
        ] {
            conn.execute(
                "INSERT INTO episodes (id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type, source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at, consolidated, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 'user', NULL, NULL, 'conversation', NULL, NULL,
                    ?6, 0, NULL, 0, ?2)",
                rusqlite::params![id, now, summary, emotion, importance, strength],
            )
            .map_err(|e| format!("seed episode {} failed: {}", id, e))?;
        }
        for (id, cat, key, val, conf) in [
            ("f_seed_1", "preference", "饮料", "奶茶", 0.9f64),
            ("f_seed_2", "goal", "近况", "正在找实习", 0.85),
        ] {
            conn.execute(
                "INSERT INTO facts (id, category, key, value, confidence, valid_from, valid_to,
                    source_episode, mention_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, 1, ?7, ?7)",
                rusqlite::params![id, cat, key, val, conf, today, now],
            )
            .map_err(|e| format!("seed fact {} failed: {}", id, e))?;
        }
        Ok(())
    })
}

fn count_rows(db: &DbState, table: &str) -> i64 {
    // table is a hard-coded literal in this file, never external input.
    let sql = format!("SELECT COUNT(*) FROM {}", table);
    db.with_conn(|conn| {
        conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())
    })
    .unwrap_or(0)
}

/// Inserts a next_interaction thought — what last night's reflection would have
/// left for "next time the user shows up".
fn insert_pending_thought(db: &DbState, id: &str, content: &str) {
    db.with_conn(|conn| {
        reflections::insert_thought(
            conn,
            &InternalThought {
                id: id.to_string(),
                content: content.to_string(),
                emotion: Some("happy".to_string()),
                source_reflection: None,
                surfacing_type: "next_interaction".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                surfaced_at: None,
            },
        )
    })
    .expect("insert pending thought");
}

fn is_unsurfaced(db: &DbState, id: &str) -> bool {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT surfaced_at IS NULL FROM internal_thoughts WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, bool>(0),
        )
        .map_err(|e| e.to_string())
    })
    .unwrap_or(false)
}

/// Gap 1: `maybe_run_if_due` runs an end-to-end reflection when none exists yet.
#[tokio::test]
async fn maybe_run_if_due_runs_when_due() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let llm = make_llm();
    let db = test_db();
    seed(&db).expect("seed interactions");
    let refl_before = count_rows(&db, "reflections");

    let ran = reflection::maybe_run_if_due(&db, &llm)
        .await
        .expect("maybe_run_if_due errored — scheduling path broken");

    // No prior reflection -> should_run=true -> should have run.
    assert!(ran, "expected ran=true (no prior reflection)");

    let refl_after = count_rows(&db, "reflections");
    assert_eq!(
        refl_after,
        refl_before + 1,
        "reflection row was not persisted — maybe_run_if_due didn't actually run reflection"
    );
    println!("[maybe_run_if_due] reflections {} -> {}", refl_before, refl_after);
}

/// Gap 2: `generate_welcome_back` consumes a surfaced thought (the "she says
/// what she thought last night" path) and returns a reply.
#[tokio::test]
async fn welcome_back_consumes_surfaced_thought() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let llm = make_llm();
    let db = test_db();
    seed(&db).expect("seed interactions");

    // The thought last night's reflection left behind.
    insert_pending_thought(&db, "thought_wb", "他今天好像有点累，希望他早点休息");
    assert!(
        is_unsurfaced(&db, "thought_wb"),
        "precondition: thought must start unsurfaced"
    );

    // 330s away ≈ the welcome-back scenario (5.5min absence).
    let outcome = proactive::generate_welcome_back(&db, &llm, None, &[], 330, false)
        .await
        .expect("generate_welcome_back errored");

    // CORE: the thought was consumed by the welcome-back path (surface_thoughts
    // marks it surfaced). This is the wiring that was missing before this round.
    assert!(
        !is_unsurfaced(&db, "thought_wb"),
        "thought was NOT surfaced — generate_welcome_back didn't consume surface_thoughts"
    );

    let o = outcome.expect("expected a reply (LLM configured)");
    assert!(!o.reply.trim().is_empty(), "reply is empty");
    println!("[welcome_back] thought consumed (surfaced_at set) ✓");
    println!("[welcome_back] reply : {:?}", o.reply);
    println!("[welcome_back] anchor: {:?}", o.anchor);
}
