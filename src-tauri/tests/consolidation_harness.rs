//! Consolidation end-to-end harness (B1): proves the compression → fact
//! backfill chain works against the REAL configured LLM, on a clean in-memory DB.
//!
//! Verifies:
//!   1. `consolidate` compresses a batch of low-importance episodes (threshold
//!      met, 100+ unconsolidated low-importance rows) into one new episode.
//!   2. Durable facts from the summary are written BACK to the facts table
//!      (P13 V2 — consolidation updates Facts), each with a valid
//!      source_episode FK pointing at the consolidated episode.
//!   3. The originals are marked consolidated (consolidated=1).
//!
//! Uses the REAL LLM (API key in config.toml) but an in-memory DB — repeatable,
//! does not touch your live memory.
//!
//! Run: cargo test --test consolidation_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::soul::consolidation;

/// Seeds 100 low-importance unconsolidated episodes. Most are trivial daily
/// noise; a few contain explicit durable user facts the consolidation LLM
/// should surface back into the facts table.
/// Seeds `count` low-importance unconsolidated episodes, starting at id index
/// `start`. Most are trivial daily noise; a few (i % 10 == 2/6/9 pattern) contain
/// explicit durable user facts the consolidation LLM should surface back into
/// the facts table. `ts_offset` staggers created_at so later seeds sort after
/// earlier ones (ORDER BY created_at ASC keeps the original pool first).
fn seed(db: &DbState, start: i64, count: i64) -> Result<(), String> {
    let base = chrono::Utc::now();
    let summaries = [
        "用户说今天午饭吃了牛肉面，觉得挺好吃的",
        "用户提到最近在追一部剧，周末会看两集",
        "用户说自己喜欢喝奶茶，几乎每天一杯",
        "用户今天上班有点累，说项目快收尾了",
        "用户说周末想去公园散步",
        "用户说昨晚睡得不太好，有点困",
        "用户提到喜欢猫，以后想养一只",
        "用户说下午要开个会",
        "用户说最近在学做饭，做了番茄炒蛋",
        "用户说天气不错，心情挺好的",
    ];
    db.with_conn(|conn| {
        for k in 0..count {
            let i = start + k;
            let summary = summaries[(i % 10) as usize];
            let ts = (base + chrono::Duration::seconds(i)).to_rfc3339();
            conn.execute(
                "INSERT INTO episodes (id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type, source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at, consolidated, created_at)
                 VALUES (?1, ?2, ?3, NULL, 0.2, 0, 'user', NULL, NULL, 'conversation', NULL, NULL,
                    0.4, 0, NULL, 0, ?2)",
                rusqlite::params![format!("ep_low_{}", i), ts, summary],
            )
            .map_err(|e| format!("seed episode {} failed: {}", i, e))?;
        }
        Ok(())
    })
}

/// Seeds pure-noise episodes (no durable facts) to top the pool back up to the
/// 100-row consolidation threshold mid-run, simulating continued daily chatter.
/// Staggered later than the fact-bearing pool so the original episodes are
/// consumed first.
fn seed_noise(db: &DbState, start: i64, count: i64) -> Result<(), String> {
    let base = chrono::Utc::now() + chrono::Duration::days(1);
    db.with_conn(|conn| {
        for k in 0..count {
            let i = start + k;
            let ts = (base + chrono::Duration::seconds(i)).to_rfc3339();
            conn.execute(
                "INSERT INTO episodes (id, time, summary, emotion, importance, is_landmark,
                    subject, participants, topics, source_type, source_conversation_id, source_turn,
                    memory_strength, recall_count, last_recalled_at, consolidated, created_at)
                 VALUES (?1, ?2, ?3, NULL, 0.15, 0, 'user', NULL, NULL, 'conversation', NULL, NULL,
                    0.3, 0, NULL, 0, ?2)",
                rusqlite::params![format!("ep_noise_{}", i), ts, "用户今天做了点日常小事"],
            )
            .map_err(|e| format!("seed noise {} failed: {}", i, e))?;
        }
        Ok(())
    })
}

fn count_rows(db: &DbState, sql: &str) -> i64 {
    db.with_conn(|conn| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string()))
        .unwrap_or(-1)
}

#[tokio::test]
async fn consolidation_fact_backfill_end_to_end() {
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
    // Seed 100 fact-bearing episodes. The pool drops below the 100-row
    // threshold after each batch (each batch eats 10, adds 1 summary), so
    // noise seeds top it back up mid-run — exactly like continued daily
    // chatter in production (where memory keeps growing).
    seed(&db, 0, 100).expect("seed 100 low-importance episodes");

    let facts_before = count_rows(&db, "SELECT COUNT(*) FROM facts");

    // --- Drain all 100 seeded episodes: consolidate() eats BATCH_SIZE=10 per
    // call while the unconsolidated low-importance pool is >= 100; each batch
    // shrinks the pool by ~9 (10 eaten, 1 summary added), so we top it back up
    // with noise seeds between batches, exactly like continued daily chatter.
    let mut total_consolidated: i64 = 0;
    let mut noise_used: i64 = 0;
    for batch in 1..=30 {
        let low_imp: i64 = count_rows(
            &db,
            "SELECT COUNT(*) FROM episodes WHERE consolidated = 0 AND importance < 0.4",
        );
        if low_imp < 100 {
            let need = 100 - low_imp;
            seed_noise(&db, noise_used, need).expect("top up noise seeds");
            noise_used += need;
        }
        let n = consolidation::consolidate(&db, &llm)
            .await
            .expect("consolidate errored — consolidation chain broken") as i64;
        total_consolidated += n;
        let originals_done: i64 = count_rows(
            &db,
            "SELECT COUNT(*) FROM episodes WHERE consolidated = 1 AND id LIKE 'ep_low_%'",
        );
        println!(
            "batch {}: +{} consolidated (total {}, {} of 100 originals done)",
            batch, n, total_consolidated, originals_done
        );
        if originals_done >= 100 {
            break;
        }
    }
    let originals_done: i64 = count_rows(
        &db,
        "SELECT COUNT(*) FROM episodes WHERE consolidated = 1 AND id LIKE 'ep_low_%'",
    );
    assert_eq!(
        originals_done, 100,
        "all 100 seeded low-importance episodes must be consolidated"
    );
    println!("total consolidated (originals + noise/summaries): {}", total_consolidated);

    // (1) One consolidated summary episode per batch was produced.
    let summary_count = count_rows(
        &db,
        "SELECT COUNT(*) FROM episodes WHERE source_type = 'consolidation'",
    );
    assert!(
        summary_count >= 10,
        "expected >=10 consolidated summary episodes (one per batch), got {}",
        summary_count
    );

    // (2) Facts were written back, each FK-resolving to the consolidated episode.
    let facts_after = count_rows(&db, "SELECT COUNT(*) FROM facts");
    let backfilled = count_rows(
        &db,
        "SELECT COUNT(*) FROM facts WHERE source_episode LIKE 'ep_consolidated%'",
    );
    println!(
        "facts: {} -> {} ({} backfilled from the summary)",
        facts_before, facts_after, backfilled
    );
    let dangling = count_rows(
        &db,
        "SELECT COUNT(*) FROM facts f LEFT JOIN episodes e ON f.source_episode = e.id
         WHERE f.source_episode IS NOT NULL AND e.id IS NULL",
    );
    assert_eq!(dangling, 0, "backfilled facts must satisfy the source_episode FK");

    // (3) The LLM really found durable facts in the summaries.
    assert!(
        backfilled >= 1,
        "expected at least 1 fact written back from the consolidated summary (seed contains explicit facts)"
    );
    // Print what was written for manual inspection (#11).
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT category, key, value, confidence, mention_count, source_episode
                 FROM facts WHERE source_episode LIKE 'ep_consolidated%'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        println!("\n=== CONSOLIDATION E2E RESULT ===");
        for (cat, key, value, conf, mentions, src) in rows {
            println!("  [{}] {}: {} (conf={:.2}, x{}) <- {}", cat, key, value, conf, mentions, src);
        }
        Ok(())
    })
    .expect("print backfilled facts");
}
