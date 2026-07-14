//! Full conversation pipeline orchestration.
//! Architecture principle #4: direct call chain, no queues.

use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::gate::GateRoute;
use crate::mind::planner::Intent;

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
    let db_emotion = db.with_conn(|conn| crate::db::emotion::get(conn))?;
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
        .with_conn(|conn| crate::db::relationship::get(conn))
        .ok();
    let intent = crate::mind::planner::plan(
        text,
        &emotion,
        relationship.as_ref(),
        &pending_due,
        &retrieval,
    );

    // Step 7: Check for silence action.
    if intent.action == "silence" {
        log::info!("Planner chose silence");
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

    // Step 8: Budget — compress context into messages.
    let messages = crate::mind::budget::allocate_and_compress(
        &retrieval,
        wm_context,
        &emotion,
        &intent,
    );

    // Step 9: LLM — generate response.
    let chat_result = llm
        .chat(&messages, Some(0.8), Some(500))
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;
    let response = chat_result.content;

    // Step 10: Grounding check.
    let violations = crate::mind::grounding::check_groundedness(&response, &retrieval);

    // Step 11: Record interaction.
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "chat", &now)
    });

    Ok(ConversationResult {
        response,
        intent,
        route: outcome.route,
        trigger_reason: trigger_decision.reason,
        grounding_violations: violations,
    })
}
