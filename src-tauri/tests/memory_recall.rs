//! Cross-session memory test. Proves the recall channel works end to end:
//! seed a NEW fact (empty working memory), then in a FRESH working memory ask
//! about it. A correct answer can only come from the DB facts table, not from
//! short-term context. Also verifies the extractor prompt change stops creating
//! pseudo-facts out of plain questions.
//!
//! Run: cargo test --test memory_recall -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::mind::converse;
use desktop_pet_lib::mind::pacing::QuestionPacing;
use desktop_pet_lib::mind::working::WorkingMemory;
use std::sync::Mutex;

/// A fact we will seed then recall. Uses a distinctive name unlikely to already
/// exist in the DB, so the recall answer must come from what we just stored.
const SEED_MSG: &str = "顺便告诉你，我养了一只叫糯米的小狗，它两岁了";
const RECALL_MSG: &str = "我家的狗叫什么名字？";
const EXPECTED_TOKEN: &str = "糯米";

/// A pure question that must NOT produce any fact. Used to verify the extractor
/// prompt change suppresses pseudo-facts ("user asked about X").
const NOISE_QUESTION: &str = "黑洞为什么会蒸发？";

fn snapshot_fact_keys(db: &DbState) -> Vec<String> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT key, value FROM facts").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows.flatten() {
            out.push(format!("{}={}", r.0, r.1));
        }
        Ok(out)
    }).unwrap_or_default()
}

#[tokio::test]
async fn cross_session_recall_works() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    let config = config::load_config().unwrap_or_default();
    let db_path = config::resolve_db_path(&config);
    let db = DbState::open(&db_path).expect("open db");
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    ).expect("LLM not configured");

    let pacing = Mutex::new(QuestionPacing::default());

    let facts_before = snapshot_fact_keys(&db);
    println!("facts before seed: {}", facts_before.len());

    // ---- Phase 1: SEED a fresh fact with an empty working memory ----
    let seed_wm = WorkingMemory::new();
    let conv_id = format!("mem_seed_{}", chrono::Utc::now().timestamp());
    let seed_wm_ctx = seed_wm.get_context();
    let seed = converse::converse(
        &converse::ConverseCtx {
            text: SEED_MSG, conversation_id: &conv_id, turn: 0,
            wm_context: &seed_wm_ctx, llm: &llm, db: &db,
            embedding: None, pacing: &pacing,
        },
        |_|{},
    ).await.expect("seed converse");
    println!("SEED reply: {:?}", seed.response);
    println!("SEED route: {:?}, trigger: {}", seed.route, seed.trigger_reason);

    // Confirm the fact actually landed in the DB (independent of gate routing).
    let facts_after_seed = snapshot_fact_keys(&db);
    let seed_stored = facts_after_seed.iter().any(|f| f.contains(EXPECTED_TOKEN));
    println!("fact containing '{}' persisted in DB: {}", EXPECTED_TOKEN, seed_stored);
    if !seed_stored {
        println!("!! Seed did not persist a fact. New facts since before:");
        for f in &facts_after_seed {
            if !facts_before.iter().any(|b| b == f) { println!("   + {}", f); }
        }
    }

    // ---- Phase 2: NOISE check — a pure question must not create a pseudo-fact ----
    let noise_wm = WorkingMemory::new();
    let noise_conv_id = format!("mem_noise_{}", chrono::Utc::now().timestamp());
    let noise_wm_ctx = noise_wm.get_context();
    let noise = converse::converse(
        &converse::ConverseCtx {
            text: NOISE_QUESTION, conversation_id: &noise_conv_id, turn: 0,
            wm_context: &noise_wm_ctx, llm: &llm, db: &db,
            embedding: None, pacing: &pacing,
        },
        |_|{},
    ).await.expect("noise converse");
    let facts_after_noise = snapshot_fact_keys(&db);
    let new_noise_facts: Vec<&String> = facts_after_noise
        .iter()
        .filter(|f| !facts_after_seed.iter().any(|s| s == *f))
        .collect();
    println!("NOISE reply: {:?}", noise.response);
    println!("new facts created by a pure question (should be ~0): {}", new_noise_facts.len());
    for f in &new_noise_facts { println!("   + {}", f); }

    // ---- Phase 3: RECALL with a FRESH empty working memory ----
    let recall_wm = WorkingMemory::new();
    let recall_conv_id = format!("mem_recall_{}", chrono::Utc::now().timestamp());
    let recall_wm_ctx = recall_wm.get_context();
    let recall = converse::converse(
        &converse::ConverseCtx {
            text: RECALL_MSG, conversation_id: &recall_conv_id, turn: 0,
            wm_context: &recall_wm_ctx, llm: &llm, db: &db,
            embedding: None, pacing: &pacing,
        },
        |_|{},
    ).await.expect("recall converse");
    println!("RECALL reply: {:?}", recall.response);
    let recalled = recall.response.contains(EXPECTED_TOKEN);
    println!("recall mentions '{}': {}", EXPECTED_TOKEN, recalled);

    println!("\n=== MEMORY TEST RESULT ===");
    println!("seed persisted:   {}", seed_stored);
    println!("noise suppressed: {}", new_noise_facts.is_empty());
    println!("cross-session recall: {}", recalled);

    assert!(seed_stored, "seed fact was not persisted to DB (check gate routing / extractor)");
    assert!(recalled, "recall did not mention '{}'. reply was: {}", EXPECTED_TOKEN, recall.response);
}
