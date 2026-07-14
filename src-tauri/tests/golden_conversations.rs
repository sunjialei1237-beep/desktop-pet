//! Golden Conversation evaluation tests.
//! These verify the structural correctness of the MVP memory loops
//! without requiring a live LLM connection.
//!
//! Loop 1: "User states a fact -> pet stores it -> retrieval finds it"
//! Loop 2: "User mentions future plan -> pending event created -> it's due"
//! Loop 3: "Grounding: stored facts are formatted with annotations"

use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::episodes as db_episodes;
use desktop_pet_lib::db::facts as db_facts;
use desktop_pet_lib::db::pending as db_pending;
use desktop_pet_lib::db::emotion as db_emotion;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::mind::retrieval;
use desktop_pet_lib::mind::grounding;
use desktop_pet_lib::mind::planner::{self, Intent};
use desktop_pet_lib::mind::store;
use desktop_pet_lib::mind::extractor::{ExtractionResult, EpisodeInput, FactInput};
use rusqlite::Connection;

fn store_fact(conn: &Connection, category: &str, key: &str, value: &str, confidence: f64) {
    let now = chrono::Utc::now().to_rfc3339();
    db_facts::dedup_insert(conn, &db_facts::Fact {
        id: format!("fact_{}", uuid::Uuid::new_v4().simple()),
        category: category.to_string(),
        key: key.to_string(),
        value: value.to_string(),
        confidence,
        valid_from: Some(now.clone()),
        valid_to: None,
        source_episode: None,
        mention_count: 1,
        created_at: now.clone(),
        updated_at: now,
    }).unwrap();
}

fn store_episode(conn: &Connection, summary: &str, strength: f64) {
    let now = chrono::Utc::now().to_rfc3339();
    db_episodes::insert(conn, &db_episodes::Episode {
        id: format!("ep_{}", uuid::Uuid::new_v4().simple()),
        time: now.clone(),
        summary: summary.to_string(),
        emotion: Some("happy".to_string()),
        importance: 0.7,
        is_landmark: false,
        subject: "user".to_string(),
        participants: None,
        topics: None,
        source_type: "conversation".to_string(),
        source_conversation_id: None,
        source_turn: None,
        memory_strength: strength,
        recall_count: 0,
        last_recalled_at: None,
        consolidated: false,
        created_at: now,
    }).unwrap();
}

// ==========================================
// GC_001: Remember fact and reference it
// ==========================================
#[test]
fn gc_001_remember_fact_and_retrieve() {
    let db = test_db();

    // Turn 1: User says "I like milk tea"
    db.with_conn(|conn| {
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        store_episode(conn, "user said they like milk tea", 0.8);
        Ok(())
    }).unwrap();

    // Turn 2: User asks "do you remember what I like?"
    let emotion = EmotionState::default();
    let result = retrieval::retrieve("what do I like to drink", &emotion, None, &db, 5).unwrap();

    // Verify: milk tea fact should be retrieved
    assert!(result.facts.iter().any(|f| f.value == "milk tea"),
        "GC_001 FAIL: milk tea fact should be retrievable");

    // Verify: relevant episode should be retrieved
    assert!(result.episodes.iter().any(|e| e.episode.summary.contains("milk tea")),
        "GC_001 FAIL: milk tea episode should be retrievable");

    println!("GC_001 PASS: fact remembered and retrievable");
}

// ==========================================
// GC_002: Pending event created and becomes due
// ==========================================
#[test]
fn gc_002_pending_event_tracking() {
    let db = test_db();

    // User says "I have an interview tomorrow"
    db.with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        db_pending::insert(conn, &db_pending::PendingEvent {
            id: "pe_test_1".to_string(),
            title: "job interview".to_string(),
            event_date: "2026-07-15".to_string(),
            remind_date: Some("2026-07-15T08:00:00+00:00".to_string()),
            source_episode: None,
            status: "pending".to_string(),
            importance: 0.8,
            followup_count: 0,
            created_at: now,
            triggered_at: None,
            resolved_at: None,
        })?;
        Ok(())
    }).unwrap();

    // Simulate next day: check due events
    let due = db.with_conn(|conn| {
        db_pending::get_due(conn, "2099-01-01T00:00:00+00:00")
    }).unwrap();

    assert_eq!(due.len(), 1, "GC_002 FAIL: pending event should be due");
    assert_eq!(due[0].title, "job interview");

    // Verify planner generates proactive_check for due events
    let emotion = EmotionState::default();
    let rel = desktop_pet_lib::db::relationship::Relationship {
        closeness: 35.0, trust: 50.0, days_known: 7,
        total_conversations: 20, shared_events: 3,
        last_interaction_at: None, last_interaction_type: None,
        closeness_log: None, updated_at: "2026-07-14".to_string(),
    };
    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, persona_traits: vec![],
    };
    let intent = planner::plan("hi", &emotion, Some(&rel), &due, &retrieval_result);
    assert_eq!(intent.action, "proactive_check", "GC_002 FAIL: planner should choose proactive_check");
    assert!(intent.proactive);

    println!("GC_002 PASS: pending event tracked and triggers proactive");
}

// ==========================================
// GC_003: Emotion consistency — stress leads to gentle tone
// ==========================================
#[test]
fn gc_003_emotion_consistency() {
    let stressed = EmotionState {
        mood: 0.3,
        physical_energy: 0.4,
        social_battery: 0.3,
        stress: 0.8,
        loneliness: 0.0,
        rest_need: 0.5,
    };

    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, persona_traits: vec![],
    };

    // User is anxious + pet is stressed → silence
    let intent = planner::plan("I am so stressed about everything", &stressed, None, &[], &retrieval_result);
    assert_eq!(intent.action, "silence", "GC_003 FAIL: high stress + anxiety should produce silence");
    assert_eq!(intent.tone, "quiet", "GC_003 FAIL: tone should be quiet");

    println!("GC_003 PASS: stress produces quiet tone");
}

// ==========================================
// GC_004: Correction — old fact expires, new fact replaces
// ==========================================
#[test]
fn gc_004_fact_correction() {
    let db = test_db();
    let now = chrono::Utc::now().to_rfc3339();

 db.with_conn(|conn| {
        // Original: "I like coffee"
        store_fact(conn, "preference", "drink", "coffee", 0.8);

        // Correction: expire old, insert new
        db_facts::expire_old(conn, "preference", "drink", &now)?;
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        Ok(())
    }).unwrap();

    // Only "milk tea" should be active
    let active = db.with_conn(|conn| {
        db_facts::get_active(conn, "preference", "drink")
    }).unwrap();

    assert_eq!(active.len(), 1, "GC_004 FAIL: should have exactly 1 active fact");
    assert_eq!(active[0].value, "milk tea", "GC_004 FAIL: corrected fact should be milk tea");

    println!("GC_004 PASS: fact correction replaces old value");
}

// ==========================================
// GC_005: Temporal validity — changing preferences
// ==========================================
#[test]
fn gc_005_temporal_validity() {
    let db = test_db();
    let now = chrono::Utc::now().to_rfc3339();

    db.with_conn(|conn| {
        // Old fact: "likes coffee" (expired)
        store_fact(conn, "preference", "drink", "coffee", 0.7);
        db_facts::expire_old(conn, "preference", "drink", &now)?;

        // New fact: "likes milk tea" (active)
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        Ok(())
    }).unwrap();

    let active = db.with_conn(|conn| {
        db_facts::get_active(conn, "preference", "drink")
    }).unwrap();

    assert_eq!(active.len(), 1);
    assert_eq!(active[0].value, "milk tea");

    // Retrieval should only return active (non-expired) facts
    let emotion = EmotionState::default();
    let result = retrieval::retrieve("drink", &emotion, None, &db, 5).unwrap();
    assert!(result.facts.iter().all(|f| f.value == "milk tea"),
        "GC_005 FAIL: expired fact should not be retrieved");

    println!("GC_005 PASS: temporal validity enforced");
}

// ==========================================
// GC_006: Grounding — no hallucination
// ==========================================
#[test]
fn gc_006_groundedness_check() {
    let db = test_db();

    // Only store: likes milk tea
    db.with_conn(|conn| {
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        Ok(())
    }).unwrap();

    let emotion = EmotionState::default();
    let result = retrieval::retrieve("what do I like", &emotion, None, &db, 5).unwrap();

    // Check: response mentioning hiking should be flagged
    let violations = grounding::check_groundedness(
        "You said you love hiking mountains every weekend!",
        &result,
    );
    assert!(!violations.is_empty(),
        "GC_006 FAIL: response about hiking (not in memories) should be flagged");

    // Check: response mentioning milk tea should NOT be flagged
    let violations = grounding::check_groundedness(
        "You like milk tea right? Want to get some?",
        &result,
    );
    assert!(violations.is_empty(),
        "GC_006 FAIL: response about milk tea (in memories) should not be flagged");

    println!("GC_006 PASS: grounding detects hallucinations");
}

// ==========================================
// GC_007: System prompt contains grounding constraint
// ==========================================
#[test]
fn gc_007_system_prompt_grounded() {
    let db = test_db();
    db.with_conn(|conn| {
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        Ok(())
    }).unwrap();

    let emotion = EmotionState::default();
    let result = retrieval::retrieve("what do I like", &emotion, None, &db, 5).unwrap();

    let prompt = grounding::build_system_prompt(&result, &emotion, &Intent::default());

    // Must contain the grounding constraint
    assert!(prompt.contains("Grounding Constraint"),
        "GC_007 FAIL: system prompt must contain grounding constraint");

    // Must contain the stored fact
    assert!(prompt.contains("milk tea"),
        "GC_007 FAIL: system prompt must contain retrieved memories");

    // Must contain persona (seeded or default)
    assert!(prompt.contains("Persona"),
        "GC_007 FAIL: system prompt must contain persona section");

    // Must contain emotion
    assert!(prompt.contains("Current Mood"),
        "GC_007 FAIL: system prompt must contain emotion snapshot");

    println!("GC_007 PASS: system prompt is grounded");
}

// ==========================================
// GC_008: Memory reinforcement on retrieval
// ==========================================
#[test]
fn gc_008_memory_reinforcement() {
    let db = test_db();

    let ep_id = db.with_conn(|conn| {
        let id = format!("ep_rein_{}", uuid::Uuid::new_v4().simple());
        db_episodes::insert(conn, &db_episodes::Episode {
            id: id.clone(),
            time: chrono::Utc::now().to_rfc3339(),
            summary: "user went hiking".to_string(),
            emotion: Some("happy".to_string()),
            importance: 0.6,
            is_landmark: false,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: 0.5,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        })?;
        Ok(id)
    }).unwrap();

    let emotion = EmotionState::default();
    let _result = retrieval::retrieve("hiking", &emotion, None, &db, 5).unwrap();

    // Verify strength increased
    db.with_conn(|conn| {
        let ep = db_episodes::get(conn, &ep_id)?.unwrap();
        assert!(ep.memory_strength > 0.5,
            "GC_008 FAIL: strength should increase after retrieval (was {})", ep.memory_strength);
        assert_eq!(ep.recall_count, 1,
            "GC_008 FAIL: recall count should be 1");
        Ok(())
    }).unwrap();

    println!("GC_008 PASS: memory reinforced on retrieval");
}

// ==========================================
// GC_009: Store pipeline stores episode + facts + emotion + pending
// ==========================================
#[test]
fn gc_009_store_pipeline_completeness() {
    let db = test_db();

    let extraction = ExtractionResult {
        episode: Some(EpisodeInput {
            summary: "user is preparing for internship".to_string(),
            emotion: Some("motivated".to_string()),
            importance: 0.8,
            participants: vec![],
            topics: vec!["work".to_string()],
        }),
        facts: vec![
            FactInput {
                category: "goal".to_string(),
                key: "current".to_string(),
                value: "find internship".to_string(),
                confidence: 0.9,
            },
        ],
        emotion_delta: Some(desktop_pet_lib::mind::extractor::EmotionDelta {
            mood: 0.05,
            stress: 0.0,
            energy: 0.03,
        }),
        pending_event: Some(desktop_pet_lib::mind::extractor::PendingInput {
            title: "find internship".to_string(),
            event_date: "2026-08-01".to_string(),
        }),
    };

    let ep_id = store::store(&extraction, "conv_test", 0, &db, None).unwrap();

    // Verify episode stored
    assert!(ep_id.is_some(), "GC_009 FAIL: episode should be stored");

    // Verify fact stored
    let facts = db.with_conn(|conn| {
        db_facts::get_active(conn, "goal", "current")
    }).unwrap();
    assert_eq!(facts.len(), 1, "GC_009 FAIL: fact should be stored");
    assert_eq!(facts[0].value, "find internship");

    // Verify emotion updated
    let emo = db.with_conn(|conn| db_emotion::get(conn)).unwrap();
    assert!(emo.mood > 0.5, "GC_009 FAIL: mood should have increased");

    // Verify pending event stored with remind_date
    let due = db.with_conn(|conn| {
        db_pending::get_due(conn, "2099-01-01T00:00:00+00:00")
    }).unwrap();
    assert_eq!(due.len(), 1, "GC_009 FAIL: pending event should be stored and due");
    assert_eq!(due[0].title, "find internship");

    println!("GC_009 PASS: store pipeline stores all components");
}

// ==========================================
// GC_010: Dedup — same fact twice doesn't create duplicates
// ==========================================
#[test]
fn gc_010_fact_dedup() {
    let db = test_db();

    for _ in 0..3 {
        db.with_conn(|conn| {
            store_fact(conn, "preference", "food", "hotpot", 0.8);
            Ok(())
        }).unwrap();
    }

    let facts = db.with_conn(|conn| {
        db_facts::get_active(conn, "preference", "food")
    }).unwrap();

    assert_eq!(facts.len(), 1, "GC_010 FAIL: should have 1 fact (deduped)");
    assert_eq!(facts[0].mention_count, 3, "GC_010 FAIL: mention count should be 3");

    println!("GC_010 PASS: fact dedup works correctly");
}

// ==========================================
// GC_011: Budget compression — token limit enforced
// ==========================================
#[test]
fn gc_011_budget_token_limit() {
    use desktop_pet_lib::mind::budget::{allocate_and_compress, estimate_messages_tokens};
    use desktop_pet_lib::llm::client::ChatMessage;
    use desktop_pet_lib::mind::retrieval::RetrievalResult;

    let retrieval = RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, persona_traits: vec![],
    };

    let mut wm = vec![];
    for i in 0..30 {
        wm.push(ChatMessage {
            role: "user".to_string(),
            content: format!("this is message number {} with extra padding words to fill context", i),
        });
        wm.push(ChatMessage {
            role: "assistant".to_string(),
            content: format!("reply {} with more padding words to fill the context window up", i),
        });
    }

    let messages = allocate_and_compress(&retrieval, &wm, &EmotionState::default(), &Intent::default());
    let total = estimate_messages_tokens(&messages);

    assert!(total <= 4100,
        "GC_011 FAIL: total tokens {} exceeds 4100 budget", total);

    println!("GC_011 PASS: budget compression enforces {} token limit (actual: {})", 4100, total);
}

// ==========================================
// GC_012: First run seeds persona traits
// ==========================================
#[test]
fn gc_012_first_run_seeds_persona() {
    use desktop_pet_lib::db::persona as db_persona;

    let db = test_db();

    // First run should seed traits
    let is_first = desktop_pet_lib::lifecycle::run_firstrun_checks(&db).unwrap();
    assert!(is_first, "GC_012 FAIL: should detect first run");

    let traits = db.with_conn(|conn| {
        db_persona::get_traits_by_type(conn, "core")
    }).unwrap();

    assert!(traits.len() >= 5, "GC_012 FAIL: should seed at least 5 core traits");
    assert!(traits.iter().any(|t| t.trait_key == "gentle"),
        "GC_012 FAIL: should seed 'gentle' trait");

    // Second run should NOT re-seed
    let is_first_again = desktop_pet_lib::lifecycle::run_firstrun_checks(&db).unwrap();
    assert!(!is_first_again, "GC_012 FAIL: should detect not-first run");

    println!("GC_012 PASS: first run seeds persona traits");
}
