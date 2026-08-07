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
use desktop_pet_lib::db::onboarding::UserProfile;
use desktop_pet_lib::mind::grounding;
use desktop_pet_lib::mind::planner::{self, Intent};
use desktop_pet_lib::mind::store;
use desktop_pet_lib::mind::extractor::{ExtractionResult, EpisodeInput, FactInput};
use rusqlite::Connection;
use desktop_pet_lib::db::vectors as db_vectors;

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
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![], user_profile: UserProfile::default(),
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
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![], user_profile: UserProfile::default(),
    };

    // User is anxious → comfort them (NOT silence). Silence was deliberately
    // removed from the empathy path to break the anxiety → stress → silence
    // feedback loop (planner.rs Rule 2). She now responds with gentle care.
    // The stressed emotion is kept as realistic context; stress no longer
    // gates anxiety routing. Contract pinned by unit test_anxiety_routes_to_care.
    let intent = planner::plan("I am so stressed about everything", &stressed, None, &[], &retrieval_result);
    assert_eq!(intent.goal, "care", "GC_003 FAIL: anxiety should route to care goal");
    assert_eq!(intent.action, "normal", "GC_003 FAIL: anxiety should respond (normal), not silence");
    assert_eq!(intent.tone, "gentle", "GC_003 FAIL: tone should be gentle");

    println!("GC_003 PASS: anxiety routes to gentle care (silence removed to break feedback loop)");
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
            event_date: Some("2026-08-01".to_string()),
            offset_minutes: None,
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
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![], user_profile: UserProfile::default(),
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
    // Liri persona seeds Chinese keys (firstrun.rs seed_persona); '温柔' is
    // Liri's "gentle" dimension. The old 'gentle' English key was replaced in
    // the Liri character migration.
    assert!(traits.iter().any(|t| t.trait_key == "温柔"),
        "GC_012 FAIL: should seed '温柔' (gentle) trait");

    // Second run should NOT re-seed
    let is_first_again = desktop_pet_lib::lifecycle::run_firstrun_checks(&db).unwrap();
    assert!(!is_first_again, "GC_012 FAIL: should detect not-first run");

    println!("GC_012 PASS: first run seeds persona traits");
}


// ==========================================
// GC_013: Planner celebration - good news + happy mood
// ==========================================
#[test]
fn gc_013_planner_celebration() {
    let happy = EmotionState {
        mood: 0.85,
        physical_energy: 0.8,
        social_battery: 0.7,
        stress: 0.1,
        loneliness: 0.0,
        rest_need: 0.0,
    };
    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![],
        facts: vec![],
        relationship: None,
        relationship_review: None,
        persona_traits: vec![],
        user_profile: UserProfile::default(),
    };
    let intent = planner::plan("I passed the exam! So happy!", &happy, None, &[], &retrieval_result);
    assert_eq!(intent.goal, "celebrate",
        "GC_013 FAIL: good news + happy mood should produce celebrate");
    assert_eq!(intent.tone, "excited",
        "GC_013 FAIL: tone should be excited");
    println!("GC_013 PASS: good news triggers celebration");
}

// ==========================================
// GC_014: Planner loneliness - proactive accompany
// ==========================================
#[test]
fn gc_014_planner_loneliness_proactive() {
    let lonely = EmotionState {
        mood: 0.4,
        physical_energy: 0.5,
        social_battery: 0.4,
        stress: 0.3,
        loneliness: 0.75,
        rest_need: 0.2,
    };
    let rel = desktop_pet_lib::db::relationship::Relationship {
        closeness: 35.0, trust: 50.0, days_known: 7,
        total_conversations: 20, shared_events: 3,
        last_interaction_at: None, last_interaction_type: None,
        closeness_log: None, updated_at: "2026-07-14".to_string(),
    };
    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![], user_profile: UserProfile::default(),
    };
    let intent = planner::plan("hi", &lonely, Some(&rel), &[], &retrieval_result);
    assert_eq!(intent.goal, "accompany",
        "GC_014 FAIL: high loneliness + close rel should produce accompany");
    assert!(intent.proactive, "GC_014 FAIL: should be proactive");
    println!("GC_014 PASS: loneliness triggers proactive accompany");
}

// ==========================================
// GC_015: Planner boundary - low closeness prevents proactive outreach
// ==========================================
#[test]
fn gc_015_planner_low_closeness_boundary() {
    let lonely = EmotionState {
        mood: 0.4, physical_energy: 0.5, social_battery: 0.4,
        stress: 0.3, loneliness: 0.75, rest_need: 0.2,
    };
    let rel = desktop_pet_lib::db::relationship::Relationship {
        closeness: 10.0, trust: 20.0, days_known: 2,
        total_conversations: 3, shared_events: 0,
        last_interaction_at: None, last_interaction_type: None,
        closeness_log: None, updated_at: "2026-07-14".to_string(),
    };
    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![], facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![], user_profile: UserProfile::default(),
    };
    let intent = planner::plan("hi", &lonely, Some(&rel), &[], &retrieval_result);
    assert_eq!(intent.goal, "converse",
        "GC_015 FAIL: low closeness should fall through to converse");
    assert!(!intent.proactive, "GC_015 FAIL: should not be proactive");
    println!("GC_015 PASS: low closeness prevents proactive outreach");
}

// ==========================================
// GC_016: Planner memory anchor from high-score retrieval
// ==========================================
#[test]
fn gc_016_planner_memory_anchor() {
    let now = chrono::Utc::now().to_rfc3339();
    let retrieval_result = retrieval::RetrievalResult {
        episodes: vec![retrieval::ScoredEpisode {
            episode: db_episodes::Episode {
                id: "ep_anchor".to_string(),
                time: now.clone(),
                summary: "user likes milk tea".to_string(),
                emotion: Some("happy".to_string()),
                importance: 0.7,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: "conversation".to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.7,
                recall_count: 1,
                last_recalled_at: None,
                consolidated: false,
                created_at: now,
            },
            score: 0.7,
            score_breakdown: retrieval::ScoreBreakdown {
                semantic: 0.8, strength: 0.7, recency: 0.9, emotion: 0.5,
            },
        }],
        facts: vec![], relationship: None, relationship_review: None, persona_traits: vec![],
        user_profile: UserProfile::default(),
    };
    let emotion = EmotionState::default();
    let intent = planner::plan("what should I drink", &emotion, None, &[], &retrieval_result);
    assert!(!intent.memory_anchor.is_empty(),
        "GC_016 FAIL: memory anchor should be set for score > 0.4");
    assert!(intent.memory_anchor.contains("milk tea"),
        "GC_016 FAIL: anchor should mention milk tea");
    println!("GC_016 PASS: planner picks up memory anchor from high-score retrieval");
}

// ==========================================
// GC_017: Pending event full lifecycle
// ==========================================
#[test]
fn gc_017_pending_full_lifecycle() {
    let db = test_db();
    let now = chrono::Utc::now().to_rfc3339();
    let future = "2099-01-01T00:00:00+00:00";

    db.with_conn(|conn| {
        db_pending::insert(conn, &db_pending::PendingEvent {
            id: "pe_life".to_string(),
            title: "presentation".to_string(),
            event_date: "2026-07-20".to_string(),
            remind_date: Some("2026-07-20T08:00:00+00:00".to_string()),
            source_episode: None,
            status: "pending".to_string(),
            importance: 0.8,
            followup_count: 0,
            created_at: now.clone(),
            triggered_at: None,
            resolved_at: None,
        })?;
        Ok(())
    }).unwrap();

    let due = db.with_conn(|conn| db_pending::get_due(conn, future)).unwrap();
    assert_eq!(due.len(), 1, "GC_017 FAIL: should be 1 due event");

    db.with_conn(|conn| db_pending::mark_triggered(conn, "pe_life", &now)).unwrap();
    let due_after = db.with_conn(|conn| db_pending::get_due(conn, future)).unwrap();
    assert_eq!(due_after.len(), 0, "GC_017 FAIL: triggered event should not be due");

    db.with_conn(|conn| db_pending::mark_resolved(conn, "pe_life", &now)).unwrap();
    let due_final = db.with_conn(|conn| db_pending::get_due(conn, future)).unwrap();
    assert_eq!(due_final.len(), 0, "GC_017 FAIL: resolved event should not be due");

    println!("GC_017 PASS: pending event lifecycle works end-to-end");
}

// ==========================================
// GC_019: Needs - loneliness grows then drops on interaction
// ==========================================
#[test]
fn gc_019_needs_loneliness_cycle() {
    let mut s = EmotionState::default();

    desktop_pet_lib::emotion::tick_needs(&mut s, 10800.0, false);
    assert!(s.loneliness > 0.5, "GC_019 FAIL: loneliness should grow over time");

    desktop_pet_lib::emotion::tick_needs(&mut s, 30.0, true);
    assert!(s.loneliness <= 0.5, "GC_019 FAIL: loneliness should drop on interaction");

    println!("GC_019 PASS: loneliness cycle works");
}

// ==========================================
// GC_020: Relationship pace - diminishing returns
// ==========================================
#[test]
fn gc_020_relationship_diminishing_returns() {
    use desktop_pet_lib::emotion::pace::pace_increment;
    let at_zero = pace_increment(0.0, "deep");
    let at_fifty = pace_increment(50.0, "deep");
    let at_ninety = pace_increment(90.0, "deep");
    assert!(at_zero > at_fifty, "GC_020 FAIL: should diminish from 0 to 50");
    assert!(at_fifty > at_ninety, "GC_020 FAIL: should diminish from 50 to 90");
    assert!((at_zero - 2.0).abs() < 0.001, "GC_020 FAIL: at 0 should be 2.0");
    assert!((at_ninety - 0.2).abs() < 0.001, "GC_020 FAIL: at 90 should be 0.2");
    println!("GC_020 PASS: closeness has diminishing returns");
}

// ==========================================
// GC_021: Relationship pace - correction penalty and gradual decay
// ==========================================
#[test]
fn gc_021_relationship_correction_and_decay() {
    use desktop_pet_lib::emotion::pace::{pace_increment, decay_closeness};
    let correction_inc = pace_increment(40.0, "correction");
    assert!(correction_inc < 0.0, "GC_021 FAIL: correction should be negative");

    let decayed = decay_closeness(50.0, 7.0);
    assert!(decayed < 50.0, "GC_021 FAIL: 7 days should decay closeness");
    assert!(decayed > 40.0, "GC_021 FAIL: decay should be gradual");

    println!("GC_021 PASS: correction penalizes, decay is gradual");
}

// ==========================================
// GC_022: Internal monologue surfaces on next interaction, then stops
// ==========================================
#[test]
fn gc_022_internal_monologue_surface() {
    use desktop_pet_lib::db::reflections::{insert_thought, InternalThought};
    use desktop_pet_lib::soul::surface_thoughts;

    let db = test_db();
    db.with_conn(|conn| {
        insert_thought(conn, &InternalThought {
            id: "thought_gc22".to_string(),
            content: "I wonder if they got enough sleep last night".to_string(),
            emotion: Some("concern".to_string()),
            source_reflection: None,
            surfacing_type: "next_interaction".to_string(),
            created_at: "2026-07-14T22:00:00+00:00".to_string(),
            surfaced_at: None,
        })?;
        Ok(())
    }).unwrap();

    let surfaced = surface_thoughts(&db).unwrap();
    assert_eq!(surfaced.len(), 1, "GC_022 FAIL: should surface 1 thought");
    assert!(surfaced[0].content.contains("sleep"), "GC_022 FAIL: content should match");

    let again = surface_thoughts(&db).unwrap();
    assert_eq!(again.len(), 0, "GC_022 FAIL: thought should not resurface");

    println!("GC_022 PASS: internal monologue surfaces once then stops");
}

// ==========================================
// GC_023: Perception - time of day classification
// ==========================================
#[test]
fn gc_023_perception_time_of_day() {
    use desktop_pet_lib::perception::time::{time_of_day, TimeOfDay};
    assert_eq!(time_of_day(8), TimeOfDay::Morning);
    assert_eq!(time_of_day(14), TimeOfDay::Afternoon);
    assert_eq!(time_of_day(20), TimeOfDay::Evening);
    assert_eq!(time_of_day(23), TimeOfDay::LateNight);
    assert_eq!(time_of_day(1), TimeOfDay::LateNight);
    assert_eq!(time_of_day(3), TimeOfDay::DeepNight);

    println!("GC_023 PASS: time of day classification correct");
}

// ==========================================
// GC_024: Perception - presence states from idle time
// ==========================================
#[test]
fn gc_024_perception_presence() {
    use desktop_pet_lib::perception::presence::{classify, PresenceState};
    assert_eq!(classify(10), PresenceState::Active);
    assert_eq!(classify(29), PresenceState::Active);
    assert_eq!(classify(30), PresenceState::BriefAway);
    assert_eq!(classify(120), PresenceState::BriefAway);
    assert_eq!(classify(300), PresenceState::LongAway);
    assert_eq!(classify(3600), PresenceState::LongAway);

    println!("GC_024 PASS: presence classification correct");
}

// ==========================================
// GC_025: Perception - app category classification
// ==========================================
#[test]
fn gc_025_perception_app_category() {
    use desktop_pet_lib::perception::window::{classify_process, AppCategory};
    assert_eq!(classify_process("code.exe"), AppCategory::Work);
    assert_eq!(classify_process("devenv.exe"), AppCategory::Work);
    assert_eq!(classify_process("steam.exe"), AppCategory::Entertainment);
    assert_eq!(classify_process("WeChat.exe"), AppCategory::Social);
    assert_eq!(classify_process("chrome.exe"), AppCategory::Browsing);
    assert_eq!(classify_process("explorer.exe"), AppCategory::Other);

    println!("GC_025 PASS: app category classification correct");
}

// ==========================================
// GC_026: Vector search ranks by cosine similarity
// ==========================================
#[test]
fn gc_026_vector_search_ranking() {
    let db = test_db();
    db.with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        for (id, summary) in [("ep_v1", "hotpot"), ("ep_v2", "coding")] {
            db_episodes::insert(conn, &db_episodes::Episode {
                id: id.to_string(), time: now.clone(), summary: summary.to_string(),
                emotion: None, importance: 0.5, is_landmark: false,
                subject: "user".to_string(), participants: None, topics: None,
                source_type: "conversation".to_string(),
                source_conversation_id: None, source_turn: None,
                memory_strength: 0.5, recall_count: 0,
                last_recalled_at: None, consolidated: false, created_at: now.clone(),
            })?;
        }
        db_vectors::insert(conn, "ep_v1", &[1.0, 0.0, 0.0])?;
        db_vectors::insert(conn, "ep_v2", &[0.0, 1.0, 0.0])?;

        let results = db_vectors::search(conn, &[0.9, 0.1, 0.0], 2)?;
        assert_eq!(results.len(), 2, "GC_026 FAIL: should return 2 results");
        assert!(results[0].1 > results[1].1, "GC_026 FAIL: first should be more similar");
        assert_eq!(results[0].0, "ep_v1", "GC_026 FAIL: ep_v1 should rank first");
        Ok(())
    }).unwrap();

    println!("GC_026 PASS: vector search ranks by similarity");
}

// ==========================================
// GC_027: Memory strength affects retrieval ranking
// ==========================================
#[test]
fn gc_027_memory_strength_ranking() {
    let db = test_db();
    db.with_conn(|conn| {
        store_episode(conn, "user likes hotpot a lot", 0.9);
        store_episode(conn, "user likes hotpot a lot", 0.3);
        Ok(())
    }).unwrap();

    let emotion = EmotionState::default();
    let result = retrieval::retrieve("hotpot", &emotion, None, &db, 5).unwrap();
    assert_eq!(result.episodes.len(), 2, "GC_027 FAIL: should retrieve 2 episodes");
    assert!(result.episodes[0].episode.memory_strength >= result.episodes[1].episode.memory_strength,
        "GC_027 FAIL: higher-strength episode should rank first");

    println!("GC_027 PASS: memory strength affects ranking");
}

// ==========================================
// GC_028: Grounding catches partial hallucination
// ==========================================
#[test]
fn gc_028_grounding_partial_hallucination() {
    let db = test_db();
    db.with_conn(|conn| {
        store_fact(conn, "preference", "drink", "milk tea", 0.9);
        Ok(())
    }).unwrap();

    let emotion = EmotionState::default();
    let result = retrieval::retrieve("what do I like", &emotion, None, &db, 5).unwrap();

    let violations = grounding::check_groundedness(
        "You like milk tea! You said you love hiking mountains!",
        &result,
    );
    assert!(!violations.is_empty(),
        "GC_028 FAIL: hiking claim should be flagged even though milk tea is grounded");

    println!("GC_028 PASS: grounding catches partial hallucination");
}

// ==========================================
// GC_029: Lifecycle cleanup removes trivial, preserves landmarks
// ==========================================
#[test]
fn gc_029_lifecycle_cleanup() {
    let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
    let db = test_db();

    db.with_conn(|conn| {
        db_episodes::insert(conn, &db_episodes::Episode {
            id: "ep_old_low".to_string(), time: old.clone(),
            summary: "trivial old event".to_string(),
            emotion: None, importance: 0.1, is_landmark: false,
            subject: "user".to_string(), participants: None, topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None, source_turn: None,
            memory_strength: 0.1, recall_count: 0,
            last_recalled_at: Some(old.clone()),
            consolidated: false, created_at: old.clone(),
        })?;
        db_episodes::insert(conn, &db_episodes::Episode {
            id: "ep_old_lm".to_string(), time: old.clone(),
            summary: "first meeting".to_string(),
            emotion: None, importance: 0.1, is_landmark: true,
            subject: "user".to_string(), participants: None, topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None, source_turn: None,
            memory_strength: 0.9, recall_count: 5,
            last_recalled_at: Some(old.clone()),
            consolidated: false, created_at: old.clone(),
        })?;
        Ok(())
    }).unwrap();

    let deleted = desktop_pet_lib::soul::lifecycle_cleanup(&db).unwrap();
    assert!(deleted >= 1, "GC_029 FAIL: should delete old low-importance episode");

    db.with_conn(|conn| {
        let lm = db_episodes::get(conn, "ep_old_lm")?;
        assert!(lm.is_some(), "GC_029 FAIL: landmark should survive cleanup");
        Ok(())
    }).unwrap();

    println!("GC_029 PASS: lifecycle cleanup removes trivial, preserves landmarks");
}

// ==========================================
// GC_030: End-to-end memory loop (store -> retrieve -> plan)
// ==========================================
#[test]
fn gc_030_end_to_end_memory_loop() {
    let db = test_db();

    let extraction = ExtractionResult {
        episode: Some(EpisodeInput {
            summary: "user is preparing for an internship interview".to_string(),
            emotion: None,
            importance: 0.8,
            participants: vec![],
            topics: vec!["work".to_string()],
        }),
        facts: vec![FactInput {
            category: "goal".to_string(),
            key: "current".to_string(),
            value: "find internship".to_string(),
            confidence: 0.9,
        }],
        emotion_delta: None,
        pending_event: None,
    };
    store::store(&extraction, "conv_e2e", 0, &db, None).unwrap();

    let emotion = EmotionState::default();
    let result = retrieval::retrieve("internship", &emotion, None, &db, 5).unwrap();
    assert!(result.facts.iter().any(|f| f.value == "find internship"),
        "GC_030 FAIL: stored fact should be retrievable");
    assert!(!result.episodes.is_empty(),
        "GC_030 FAIL: stored episode should be retrievable");

    let intent = planner::plan("how is my job search going", &emotion, None, &[], &result);
    assert!(!intent.memory_anchor.is_empty(),
        "GC_030 FAIL: planner should reference the retrieved memory");

    println!("GC_030 PASS: full memory loop (store -> retrieve -> plan) works");
}
