//! Soul layer end-to-end harness. Proves the reflection → monologue-surfacing
//! chain works against the REAL configured LLM, on a clean in-memory DB seeded
//! with a few interactions.
//!
//! This is the verification the previous handoff left half-done (the dev window
//! hit a systemError mid-run). It locks down five things, in order of how likely
//! they are to break:
//!
//!   1. `run_reflection` returns Ok — the chain compiles and runs end to end
//!      (the bare minimum; this is where the systemError interrupted things).
//!   2. A reflection row is actually persisted to the DB.
//!   3. The LLM produced a non-empty summary — it really reflected on something.
//!   4. FK integrity: every internal_thought.source_reflection resolves to a
//!      real reflections row. Regression guard for the insert-order bug fixed
//!      this round (thoughts were written before their parent reflection, so the
//!      whole transaction rolled back under PRAGMA foreign_keys = ON).
//!   5. The surfacing path the frontend relies on: `get_unsurfaced` returns
//!      exactly what was just written, and `mark_surfaced` retires a thought
//!      (one-shot bubble mechanism).
//!
//! Uses the REAL LLM (set API key in config.toml) but an in-memory DB, so it is
//! repeatable and does NOT pollute your live reflections / traits / thoughts.
//!
//! Run: cargo test --test soul_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::{reflections, DbState};
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::soul::reflection::{self, ReflectionTrigger};

/// Seeds an in-memory DB with a few realistic interactions so the LLM has
/// something concrete to reflect on (and is likely to emit a thought). Uses raw
/// SQL so the harness does not depend on db-layer insert helpers being `pub`.
fn seed(db: &DbState) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    db.with_conn(|conn| {
        // Three episodes within the 24h window get_recent scans.
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
        // Two active facts (valid_to NULL) so they show up in get_all_active.
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

/// Counts thoughts whose source_reflection does NOT resolve to a reflections
/// row. Must always be 0; >0 means the insert-order FK bug regressed.
fn dangling_thought_count(db: &DbState) -> i64 {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM internal_thoughts it
             LEFT JOIN reflections r ON it.source_reflection = r.id
             WHERE it.source_reflection IS NOT NULL AND r.id IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())
    })
    .unwrap_or(0)
}

#[tokio::test]
async fn soul_reflection_end_to_end() {
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
    seed(&db).expect("seed interactions");

    let refl_before = count_rows(&db, "reflections");
    let thought_before = count_rows(&db, "internal_thoughts");
    let trait_before = count_rows(&db, "persona_traits");
    println!(
        "before: reflections={}, thoughts={}, traits={}",
        refl_before, thought_before, trait_before
    );

    // --- Run reflection against the real LLM ---
    let result = reflection::run_reflection(ReflectionTrigger::Daily, &db, &llm)
        .await
        .expect("run_reflection errored — Soul chain broken");
    println!("reflection summary   : {:?}", result.summary);
    println!(
        "produced             : {} new trait(s), {} new thought(s)",
        result.new_trait_count, result.new_thought_count
    );

    // (1) Reflection row persisted.
    let refl_after = count_rows(&db, "reflections");
    assert_eq!(
        refl_after,
        refl_before + 1,
        "reflection row was not persisted to DB"
    );

    // (2) LLM actually reflected — non-empty summary.
    assert!(
        !result.summary.trim().is_empty(),
        "reflection summary is empty — LLM produced nothing (check reflection.txt prompt loaded)"
    );

    // (3) FK integrity — the insert-order bug guard. Must stay 0.
    let dangling = dangling_thought_count(&db);
    assert_eq!(
        dangling, 0,
        "{} thought(s) reference a missing reflection row — FK insert-order bug regressed",
        dangling
    );

    // (4) Surfacing path: get_unsurfaced sees exactly the new thoughts.
    let thought_after = count_rows(&db, "internal_thoughts");
    let unsurfaced = db
        .with_conn(|conn| reflections::get_unsurfaced(conn))
        .expect("get_unsurfaced");
    assert_eq!(
        unsurfaced.len() as i64,
        thought_after - thought_before,
        "get_unsurfaced did not return all newly written thoughts — bubble path broken"
    );
    println!("unsurfaced thoughts ready to surface as bubbles: {}", unsurfaced.len());
    for t in &unsurfaced {
        println!("  - {:?} (emotion={:?})", t.content, t.emotion);
    }

    // (5) mark_surfaced retires one thought (one-shot surfacing mechanism).
    if let Some(first) = unsurfaced.first() {
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| reflections::mark_surfaced(conn, &first.id, &now))
            .expect("mark_surfaced");
        let remaining = db
            .with_conn(|conn| reflections::get_unsurfaced(conn))
            .expect("get_unsurfaced recheck");
        assert_eq!(
            remaining.len() as i64,
            (thought_after - thought_before) - 1,
            "mark_surfaced did not retire the thought"
        );
    } else if result.new_thought_count == 0 {
        println!("[warn] LLM produced 0 thoughts this run — surfacing path not substantively exercised");
    }

    let trait_after = count_rows(&db, "persona_traits");
    println!("\n=== SOUL E2E RESULT ===");
    println!("reflection persisted : {} -> {}", refl_before, refl_after);
    println!("thoughts persisted   : {} -> {} (surfacing path OK)", thought_before, thought_after);
    println!("traits persisted     : {} -> {}", trait_before, trait_after);
    println!("FK integrity         : {} dangling (order bug guarded)", dangling);
    println!("summary              : {:?}", result.summary);
}
