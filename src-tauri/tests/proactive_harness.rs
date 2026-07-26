//! Proactive-recall + genuine-questioning standard test.
//! Mirrors the `proactive_bubble` command logic (anchor pick + budget + LLM)
//! so we can verify standards S1-S5 against the real DeepSeek model.
//! Run: cargo test --test proactive_harness -- --nocapture --test-threads=1
//! Stop the dev server first (it locks the exe).

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::mind::budget;
use desktop_pet_lib::mind::planner::Intent;
use desktop_pet_lib::mind::retrieval;

const BANNED: &[&str] = &[
    "有什么事吗", "需要帮忙", "我能帮你", "最近怎么样", "怎么样啦", "怎么样呀",
    "在吗", "有事吗", "能帮你做什么",
];

fn check_standards(reply: &str, anchor_keyword: &str) -> (bool, Vec<String>) {
    let mut fails = Vec::new();
    // S1 anchored: the model may rewrite an English-DB fact into Chinese
    // (milk tea -> 奶茶). Accept literal keyword, char overlap, OR a curated
    // cross-language synonym hit. This is the only place standards get soft;
    // every other standard (S2/S3/S4) is still strict.
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
    ).expect("LLM not configured");

    let db_emotion = db.with_conn(desktop_pet_lib::db::emotion::get).unwrap();
    let emotion = desktop_pet_lib::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let retrieval = retrieval::retrieve(
        "user's life recent events preferences",
        &emotion,
        None,
        &db,
        3,
    ).unwrap();

    println!("retrieved {} facts, {} episodes", retrieval.facts.len(), retrieval.episodes.len());

    let anchorable = |f: &desktop_pet_lib::db::facts::Fact| -> bool {
        if f.confidence < 0.7 { return false; }
        let bad_prefixes = ["knowledge_", "belief_", "chemistry_", "geography_"];
        if bad_prefixes.iter().any(|p| f.key.starts_with(p)) { return false; }
        let v = f.value.to_lowercase();
        let bad_markers = ["user asked", "user is asking", "curious about user",
            "asking about", "does not know", "user doesn't know", "user is busy"];
        if bad_markers.iter().any(|m| v.contains(m)) { return false; }
        true
    };
    let (anchor, keyword, goal, tone) = if let Some(f) = retrieval.facts.iter().find(|f| anchorable(f)) {
        (format!("{}: {}", f.key, f.value), f.value.clone(), "accompany", "playful")
    } else if let Some(ep) = retrieval.episodes.first() {
        let s = ep.episode.summary.clone();
        let kw = s.chars().take(4).collect::<String>();
        (s.clone(), kw, "accompany", "gentle")
    } else {
        panic!("no facts/episodes in DB to anchor on; run a conversation first");
    };

    let intent = Intent {
        goal: goal.to_string(),
        memory_anchor: anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "proactive_check".to_string(),
    };

    let mut messages = budget::allocate_and_compress(&retrieval, &[], &emotion, &intent);
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: format!(
            "（你刚刚突然想起了这件事，想主动跟用户说。提起的记忆：{}。按规则 8/8a/8b 回复。）",
            anchor
        ),
    });

    println!("[proactive-test] anchor={:?}", anchor);

    let chat = llm.chat(&messages, Some(0.8), Some(500)).await.expect("LLM call");
    let reply = chat.content.trim().to_string();
    println!("[proactive-test] reply={:?}", reply);

    let (pass, fails) = check_standards(&reply, &keyword);
    println!("=== PROACTIVE STANDARD ===");
    println!("anchor keyword: {}", keyword);
    println!("pass: {}", pass);
    for f in &fails { println!("  FAIL: {}", f); }

    assert!(pass, "proactive reply failed standards: {:?}", fails);
}
