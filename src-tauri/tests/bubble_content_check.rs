//! One-off content check (2026-08-09 续⁸). Drives the real `proactive::generate`
//! — the engine behind `proactive_bubble` — N times against the real DB + real
//! LLM, and prints every bubble so a human can eyeball content diversity.
//!
//! User feedback was "冒泡内容全和糯米有关，要像真人突然找你聊天". 续⁸ made
//! generate pick 70% lively (anchorless, moment-driven) / 30% memory-anchored.
//! This check confirms that split holds at runtime and that lively bubbles are
//! actually lively (no fabrication, no canned greeting, no interrogation).
//!
//! Bypasses the 30-min frequency gate entirely — that's a frontend-poll +
//! AppState concern; this is about WHAT she says, not WHEN.
//!
//! Run: cargo test --test bubble_content_check -- --ignored --nocapture --test-threads=1
//! Stop the desktop pet first (it locks the DB).

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::pending::proactive;

const N: usize = 15;

/// Phrases that signal a fabricated claim about the user's past. Lively bubbles
/// must voice HER moment (feelings/surroundings/time), never "你之前说过的X".
const FABRICATION_MARKERS: &[&str] = &[
    "你之前", "你说过", "你提到", "你跟我说", "你上次", "你最喜欢", "你不是说", "你不是要",
    "你不是要找", "记得你", "你不是喜欢",
];

/// Canned greeting phrases that kill the "real person suddenly chatting" feel.
const CANNED: &[&str] = &[
    "有什么事", "需要帮忙", "我能帮你", "最近怎么样", "怎么样啦", "在吗", "有事吗", "在干嘛",
];

#[tokio::test]
#[ignore]
async fn bubble_content_diversity() {
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
    )
    .expect("LLM not configured");

    // Real embedding so the memory-anchored path does true semantic recall (not
    // the None-degraded fallback). Degrades to None if the model isn't loaded.
    let embedding = EmbeddingService::new(std::path::Path::new(&config.embedding.model_dir));
    embedding.load().ok();
    let emb_ref: Option<&EmbeddingService> = if embedding.is_ready() {
        Some(&embedding)
    } else {
        None
    };

    println!(
        "\n=== BUBBLE CONTENT CHECK ({} calls) embedding={} ===",
        N,
        emb_ref.is_some()
    );

    let mut lively = 0usize;
    let mut memory = 0usize;
    let mut memory_nuomi = 0usize; // memory bubbles anchored on 糯米/猫/狗
    let mut fabrication_hits = 0usize;
    let mut canned_hits = 0usize;
    let mut multi_question = 0usize;

    for i in 1..=N {
        match proactive::generate(&db, &llm, emb_ref, &[], proactive::DEFAULT_MEMORY_RATIO).await {
            Ok(Some(o)) => {
                let is_lively = o.anchor.is_empty();
                if is_lively {
                    lively += 1;
                } else {
                    memory += 1;
                }
                if !is_lively
                    && (o.anchor.contains("糯米")
                        || o.anchor.contains("猫")
                        || o.anchor.contains("狗")
                        || o.anchor.to_lowercase().contains("pet"))
                {
                    memory_nuomi += 1;
                }
                let fab: Vec<&str> = FABRICATION_MARKERS
                    .iter()
                    .copied()
                    .filter(|m| o.reply.contains(m))
                    .collect();
                if !fab.is_empty() {
                    fabrication_hits += 1;
                }
                let canned: Vec<&str> = CANNED
                    .iter()
                    .copied()
                    .filter(|m| o.reply.contains(m))
                    .collect();
                if !canned.is_empty() {
                    canned_hits += 1;
                }
                let q = o.reply.matches('？').count() + o.reply.matches('?').count();
                if q > 1 {
                    multi_question += 1;
                }

                let kind = if is_lively { "LIVELY " } else { "MEMORY " };
                println!(
                    "\n[{:>2}/{}] {} anchor={:?} q={} fab={:?} canned={:?}",
                    i, N, kind, o.anchor, q, fab, canned
                );
                println!("      {}", o.reply);
            }
            Ok(None) => println!(
                "\n[{:>2}/{}] (suppressed — grounding_guard blocked a fabrication, or lively guard rejected)",
                i, N
            ),
            Err(e) => println!("\n[{:>2}/{}] ERROR: {}", i, N, e),
        }
    }

    println!("\n=== SUMMARY ===");
    println!(
        "lively(target ~85%): {}   memory(target ~15%): {}",
        lively, memory
    );
    println!(
        "memory anchored on 糯米/猫/狗: {}/{} ({:.0}%)",
        memory_nuomi,
        memory,
        if memory > 0 {
            100.0 * memory_nuomi as f64 / memory as f64
        } else {
            0.0
        }
    );
    println!("fabrication markers hit: {} (target 0)", fabrication_hits);
    println!("canned phrases hit: {} (target 0)", canned_hits);
    println!("bubbles with >1 question mark: {} (target 0)", multi_question);
}
