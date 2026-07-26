//! Full conversation pipeline orchestration.
//! Architecture principle #4: direct call chain, no queues.

use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::gate::GateRoute;
use crate::mind::planner::Intent;
use rand::Rng;

/// Result of a full conversation turn.
#[derive(Debug)]
pub struct ConversationResult {
    /// The pet's reply (empty if silence action was chosen).
    pub response: String,
    /// The Intent that directed the response.
    pub intent: Intent,
    /// The gate route assigned to the user message.
    pub route: GateRoute,
    /// Why retrieval was or was not triggered.
    pub trigger_reason: String,
    /// Potential hallucination violations detected.
    pub grounding_violations: Vec<String>,
}

/// Full conversation pipeline:
/// Ingest -> Trigger -> Retrieve -> Plan -> Budget -> LLM -> Grounding.
pub async fn converse(
    text: &str,
    conversation_id: &str,
    turn: i32,
    wm_context: &[ChatMessage],
    llm: &LlmClient,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
    pacing: &std::sync::Mutex<crate::mind::pacing::QuestionPacing>,
) -> Result<ConversationResult, String> {
    let now = chrono::Utc::now().to_rfc3339();

    // Step 1: Ingest (Gate -> Extract -> Store).
    // Build known_facts summary so the extractor can avoid duplicates.
    let known_facts = db.with_conn(|conn| {
        let facts = crate::db::facts::get_by_category(conn, "preference")?;
        let summary: Vec<String> = facts.iter().take(20).map(|f| format!("{}: {}", f.key, f.value)).collect();
        Ok(summary.join("; "))
    })?;

    let outcome = crate::mind::ingest(text, conversation_id, turn, &known_facts, llm, db, embedding).await?;

    // Step 2: Load emotion state from DB and convert to business type.
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    // Step 3: Load pending events due.
    let pending_due = db.with_conn(|conn| crate::db::pending::get_due(conn, &now))?;

    // Step 4: Memory trigger.
    let trigger_decision = crate::mind::trigger::should_retrieve(text, &emotion, wm_context);

    // Step 5: Retrieve memories.
    let retrieval = if trigger_decision.should_retrieve {
        crate::mind::retrieval::retrieve(text, &emotion, embedding, db, 5)?
    } else {
        log::info!("Retrieval skipped: {}", trigger_decision.reason);
        crate::mind::retrieval::retrieve(text, &emotion, embedding, db, 3)?
    };

    // Step 6: Planner — produce Intent.
    let relationship = db
        .with_conn(crate::db::relationship::get)
        .ok();
    let mut intent = crate::mind::planner::plan(
        text,
        &emotion,
        relationship.as_ref(),
        &pending_due,
        &retrieval,
    );

    // Step 7: Check for silence action.
    if intent.action == "silence" {
        log::info!("Planner chose silence");
        // Silence = user is anxious enough to warrant quiet. Apply the turn's
        // emotion delta (silence goal adds stress+/mood-) before returning.
        let delta = crate::emotion::react::react_to_turn(text, &intent.goal);
        let new_mood = (emotion.mood + delta.mood).clamp(0.0, 1.0);
        let new_energy = (emotion.physical_energy + delta.physical_energy).clamp(0.0, 1.0);
        let new_social = (emotion.social_battery + delta.social_battery).clamp(0.0, 1.0);
        let new_stress = (emotion.stress + delta.stress).clamp(0.0, 1.0);
        let new_loneliness = (emotion.loneliness + delta.loneliness).clamp(0.0, 1.0);
        let new_state = crate::emotion::state::EmotionState {
            mood: new_mood,
            physical_energy: new_energy,
            social_battery: new_social,
            stress: new_stress,
            loneliness: new_loneliness,
            rest_need: emotion.rest_need,
        };
        let new_label = crate::emotion::state::derive_mood_label(&new_state);
        let _ = db.with_conn(|conn| {
            crate::db::emotion::update_fields(
                conn,
                Some(new_mood),
                Some(new_label),
                Some(new_energy),
                Some(new_social),
                Some(new_stress),
                Some(new_loneliness),
                None,
                &now,
            )
        });
        log::info!(
            "[emotion-react] (silence) mood {:.2}->{:.2} ({}) stress {:.2}->{:.2}",
            emotion.mood, new_mood, new_label, emotion.stress, new_stress,
        );
        let _ = db.with_conn(|conn| {
            crate::db::relationship::record_interaction(conn, "silence", &now)
        });
        return Ok(ConversationResult {
            response: String::new(),
            intent,
            route: outcome.route,
            trigger_reason: trigger_decision.reason,
            grounding_violations: vec![],
        });
    }

    // Step 7.5: Follow-up question frequency control. The planner stays pure
    // (architecture #8); the throttle lives in this orchestration layer. A
    // silence turn already returned above, so this only touches speaking turns.
    {
        let roll: f64 = rand::thread_rng().gen();
        let mut guard = pacing
            .lock()
            .map_err(|e| format!("pacing lock error: {}", e))?;
        let snapshot = guard.clone();
        let (new_goal, next) =
            crate::mind::pacing::throttle(&intent.goal, &snapshot, roll);
        log::info!(
            "[pacing] roll={:.3} credit={}->{} last={}->{} goal={}",
            roll, snapshot.credit, next.credit,
            snapshot.last_turn_was_question, next.last_turn_was_question, intent.goal
        );
        intent.goal = new_goal;
        *guard = next;
    }

    // Step 8: Budget — compress context into messages.
    let messages = crate::mind::budget::allocate_and_compress(
        &retrieval,
        wm_context,
        &emotion,
        &intent,
    );

    // Append the CURRENT user message as the final turn. This must come AFTER
    // budget compression so the latest question can never be truncated away,
    // and the LLM always answers *this* turn (not a stale history entry).
    let mut messages = messages;
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: text.to_string(),
    });

    log::info!(
        "[ctx] messages={} last_user={:?} system_tokens~={} history_turns={}",
        messages.len(),
        text.chars().take(40).collect::<String>(),
        crate::mind::budget::estimate_tokens(&messages[0].content),
        wm_context.len(),
    );

    // Step 9: LLM — generate response.
    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096))
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;
    let response = chat_result.content;

    // Step 10: Grounding check.
    let violations = crate::mind::grounding::check_groundedness(&response, &retrieval);

    // Step 11: Record interaction.
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "chat", &now)
    });

    // Step 12: Emotion reactivity — apply rule-based deltas from this turn.
    // Pure rules only (principle #8); no LLM call. Makes the expression reflect
    // the conversation, not just the 30s homeostasis drift.
    let delta = crate::emotion::react::react_to_turn(text, &intent.goal);
    let new_mood = (emotion.mood + delta.mood).clamp(0.0, 1.0);
    let new_energy = (emotion.physical_energy + delta.physical_energy).clamp(0.0, 1.0);
    let new_social = (emotion.social_battery + delta.social_battery).clamp(0.0, 1.0);
    let new_stress = (emotion.stress + delta.stress).clamp(0.0, 1.0);
    let new_loneliness = (emotion.loneliness + delta.loneliness).clamp(0.0, 1.0);
    let new_state = crate::emotion::state::EmotionState {
        mood: new_mood,
        physical_energy: new_energy,
        social_battery: new_social,
        stress: new_stress,
        loneliness: new_loneliness,
        rest_need: emotion.rest_need,
    };
    let new_label = crate::emotion::state::derive_mood_label(&new_state);
    let _ = db.with_conn(|conn| {
        crate::db::emotion::update_fields(
            conn,
            Some(new_mood),
            Some(new_label),
            Some(new_energy),
            Some(new_social),
            Some(new_stress),
            Some(new_loneliness),
            None,
            &now,
        )
    });
    log::info!(
        "[emotion-react] mood {:.2}->{:.2} ({}) social {:.2}->{:.2} stress {:.2}->{:.2}",
        emotion.mood, new_mood, new_label,
        emotion.social_battery, new_social,
        emotion.stress, new_stress,
    );

    Ok(ConversationResult {
        response,
        intent,
        route: outcome.route,
        trigger_reason: trigger_decision.reason,
        grounding_violations: violations,
    })
}
