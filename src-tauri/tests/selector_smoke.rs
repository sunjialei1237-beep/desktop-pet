//! One-off selector smoke (2026-08-16 续⁴¹·2). Answers the user's question:
//! "短时间内桌宠会浮现哪些内容？带记忆/不带记忆的比例？选择器真的会
//! 拒绝不值得浮现的记忆吗？"
//!
//! Two phases against a VACUUM INTO snapshot of the REAL memory DB (never the
//! live file — the test's surfaced_count bumps and bubble_log rows stay on the
//! throwaway copy):
//!   Phase A (12 windows): production config (memory_bubble_ratio=15,
//!     selector ON) — the realized mix a user would actually perceive.
//!   Phase B (8 windows): ratio=100 — every window enters the memory branch,
//!     so the LLM selector's worthiness judgment is exercised head-on: picks
//!     with reasons vs declines (silence) vs pool drain.
//!
//! Due pendings are resolved on the COPY up front so the due path (which
//! bypasses the selector by design) doesn't dominate the measurement.
//!
//! Run: cargo test --test selector_smoke -- --ignored --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::pending::proactive;

const PHASE_A: usize = 12;
const PHASE_B: usize = 8;

#[tokio::test]
#[ignore]
async fn selector_smoke_ratio_and_worthiness() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    let config = config::load_config().unwrap_or_default();
    let real_path = config::resolve_db_path(&config);

    // Consistent snapshot of the real DB (works even against a live WAL).
    let snap = std::env::temp_dir().join("desktop_pet_selector_smoke.db");
    let _ = std::fs::remove_file(&snap);
    {
        let src = rusqlite::Connection::open_with_flags(
            &real_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open real db read-only");
        src.pragma_update(None, "journal_mode", "wal").ok();
        src.execute_batch(&format!(
            "VACUUM INTO '{}'",
            snap.to_string_lossy().replace('\'', "''")
        ))
        .expect("vacuum into snapshot");
    }
    let db = DbState::open(&snap).expect("open snapshot db");

    // Resolve due pendings on the copy so the memory branch (and its selector)
    // is what we measure, not the due-forced reminder path.
    let due_resolved = db
        .with_conn(|conn| {
            Ok(conn
                .execute("UPDATE pending_events SET status='resolved' WHERE status='pending'", [])
                .unwrap_or(0))
        })
        .unwrap_or(0);

    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured");

    let embedding = EmbeddingService::new(std::path::Path::new(&config.embedding.model_dir));
    embedding.load().ok();
    let emb_ref: Option<&EmbeddingService> = if embedding.is_ready() {
        Some(&embedding)
    } else {
        None
    };

    let facts: usize = db
        .with_conn(|conn| crate_count(conn, "facts"))
        .unwrap_or(0);
    let episodes: usize = db
        .with_conn(|conn| crate_count(conn, "episodes"))
        .unwrap_or(0);

    println!(
        "\n=== SELECTOR SMOKE: snapshot={} facts={} episodes={} due_resolved={} embedding={} ===",
        snap.display(),
        facts,
        episodes,
        due_resolved,
        emb_ref.is_some()
    );

    let mut a_lively = 0usize;
    let mut a_memory = 0usize;
    let mut a_silent = 0usize;
    let mut b_memory = 0usize;
    let mut b_lively_fallback = 0usize; // pool drained → lively fallback
    let mut b_silent = 0usize;

    println!("\n--- PHASE A: {} windows, production mix (ratio={}, selector ON) ---", PHASE_A, proactive::DEFAULT_MEMORY_RATIO);
    for i in 1..=PHASE_A {
        match proactive::generate(&db, &llm, emb_ref, &[], proactive::DEFAULT_MEMORY_RATIO, true).await {
            Ok(Some(o)) => {
                if o.anchor.is_empty() {
                    a_lively += 1;
                    println!("\n[A {:>2}/{}] LIVELY", i, PHASE_A);
                } else {
                    a_memory += 1;
                    println!(
                        "\n[A {:>2}/{}] MEMORY anchor={:?} reason={:?}",
                        i, PHASE_A, o.anchor, o.anchor_reason.unwrap_or_default()
                    );
                }
                println!("      {}", o.reply);
            }
            Ok(None) => {
                a_silent += 1;
                println!("\n[A {:>2}/{}] SILENT (selector declined / grounding suppressed — see logs above)", i, PHASE_A);
            }
            Err(e) => println!("\n[A {:>2}/{}] ERROR: {}", i, PHASE_A, e),
        }
    }

    println!("\n--- PHASE B: {} windows, ratio=100 (selector worthiness head-on) ---", PHASE_B);
    for i in 1..=PHASE_B {
        match proactive::generate(&db, &llm, emb_ref, &[], 100, true).await {
            Ok(Some(o)) => {
                if o.anchor.is_empty() {
                    b_lively_fallback += 1;
                    println!("\n[B {:>2}/{}] LIVELY-FALLBACK (pool drained — nothing left worth surfacing)", i, PHASE_B);
                } else {
                    b_memory += 1;
                    println!(
                        "\n[B {:>2}/{}] MEMORY anchor={:?} reason={:?}",
                        i, PHASE_B, o.anchor, o.anchor_reason.unwrap_or_default()
                    );
                }
                println!("      {}", o.reply);
            }
            Ok(None) => {
                b_silent += 1;
                println!("\n[B {:>2}/{}] SILENT (selector declined — nothing worth surfacing)", i, PHASE_B);
            }
            Err(e) => println!("\n[B {:>2}/{}] ERROR: {}", i, PHASE_B, e),
        }
    }

    println!("\n=== SUMMARY ===");
    println!(
        "Phase A (体感比例): lively={} memory={} silent={} → 记忆占比 of spoken: {}/{}",
        a_lively,
        a_memory,
        a_silent,
        a_memory,
        a_lively + a_memory
    );
    println!(
        "Phase B (值不值得): picked={} declined/drained-to-lively={} silent={}",
        b_memory, b_lively_fallback, b_silent
    );
    // What the NEXT window's selector will see (cross-bubble continuity).
    if let Ok(recent) = db.with_conn(|conn| desktop_pet_lib::db::bubble_log::get_recent(conn, 3)) {
        println!("bubble_log tail (what the next decision sees):");
        for e in recent {
            println!("  [{}] {} anchor={}", e.kind, e.text, e.anchor);
        }
    }
}

fn crate_count(conn: &rusqlite::Connection, table: &str) -> Result<usize, String> {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
        .map(|n| n as usize)
        .map_err(|e| e.to_string())
}
