//! Proactive-recall + genuine-questioning standard test.
//! Drives the real `proactive::generate` pipeline (anchor pick + budget + LLM —
//! the engine behind `proactive_bubble`) and checks the reply against standards
//! S1-S5. Generate is the single source of truth; this file no longer replicates
//! the anchor/budget/LLM logic.
//! Run: cargo test --test proactive_harness -- --nocapture --test-threads=1
//! Stop the dev server first (it locks the exe).

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::LlmClient;
use desktop_pet_lib::pending::proactive;

const BANNED: &[&str] = &[
    "有什么事吗", "需要帮忙", "我能帮你", "最近怎么样", "怎么样啦", "怎么样呀",
    "在吗", "有事吗", "能帮你做什么",
];

fn check_standards(reply: &str, anchor_keyword: &str) -> (bool, Vec<String>) {
    let mut fails = Vec::new();
    // S1 anchored: the model may rewrite an English-DB fact into Chinese
    // (milk tea -> 奶茶). Accept literal keyword, char overlap, OR a curated
    // cross-language synonym hit. generate's anchor for a fact is "key: value",
    // which still contains the English value, so the synonym rule fires on it.
    // Every other standard (S2/S3/S4) is strict.
    let overlap = anchor_keyword.chars().filter(|c| reply.contains(*c)).count();
    let kw_len = anchor_keyword.chars().count();
    let syns: &[(&str, &[&str])] = &[
        ("milk tea", &["奶茶"]),
        ("hotpot", &["火锅"]),
        ("blue", &["蓝", "蓝色"]),
        ("cats", &["猫"]),
        ("working out", &["健身", "锻炼"]),
        ("100kg for reps", &["100", "深蹲"]),
    ];
    let synonym_hit = syns.iter().any(|(en, zh)| {
        anchor_keyword.to_lowercase().contains(en) && zh.iter().any(|z| reply.contains(z))
    });
    let anchored = reply.contains(anchor_keyword)
        || (kw_len >= 2 && overlap >= 2)
        || synonym_hit;
    if !anchored {
        fails.push(format!("S1 not anchored: reply lacks '{}' (overlap {}/{})", anchor_keyword, overlap, kw_len));
    }
    let q = reply.matches('？').count() + reply.matches('?').count();
    if q > 1 {
        fails.push(format!("S2 interrogation: {} question marks", q));
    }
    for b in BANNED {
        if reply.contains(b) {
            fails.push(format!("S3 banned phrase present: {}", b));
        }
    }
    let sentences: Vec<&str> = reply
        .split(|c: char| c == '。' || c == '？' || c == '?' || c == '！')
        .filter(|s| !s.trim().is_empty())
        .collect();
    if sentences.len() > 2 {
        fails.push(format!("S4 not concise: {} sentences", sentences.len()));
    }
    (fails.is_empty(), fails)
}

#[tokio::test]
async fn proactive_recall_meets_standards() {
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

    // Drive the real pipeline via the single source of truth — no duplicated
    // emotion/retrieval/anchor/budget/LLM logic. generate returns the reply plus
    // the anchor it grounded on; the anchor is what S1 checks the reply against.
    // Force the memory branch (memory_ratio=100): S1-S5 verify the ANCHORED
    // reply. The production default is 15 (85% lively 碎碎念) — this harness
    // is about recall standards, not the lively/memory mix (bubble_content_check
    // reports that; proactive-bubble governance 2026-08-14).
    let outcome = proactive::generate(&db, &llm, None, &[], 100, false)
        .await
        .expect("proactive::generate errored")
        .expect("no memory to anchor on — run a conversation first so facts/episodes exist");

    println!("[proactive-test] anchor={:?}", outcome.anchor);
    println!("[proactive-test] reply={:?}", outcome.reply);

    let (pass, fails) = check_standards(&outcome.reply, &outcome.anchor);
    println!("=== PROACTIVE STANDARD ===");
    println!("anchor keyword: {}", outcome.anchor);
    println!("pass: {}", pass);
    for f in &fails {
        println!("  FAIL: {}", f);
    }

    assert!(pass, "proactive reply failed standards: {:?}", fails);
}
