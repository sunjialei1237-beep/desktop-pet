pub mod correction;
pub mod extractor;
pub mod forget;
pub mod gate;
pub mod memory_gate;
pub mod store;
pub mod working;
pub mod budget;
pub mod brain_state;
pub mod grounding;
pub mod retrieval;
pub mod trigger;
pub mod planner;
pub mod converse;
pub mod pacing;
pub mod evaluation;

pub use correction::{handle_correction, CorrectionResult};
pub use forget::{
    execute_candidate, forget_best_match, forget_episode, is_off_topic, resolve_candidate,
    ForgetCandidate, ForgetOutcome, ForgetTarget, PendingForget,
};
pub use extractor::{extract, EmotionDelta, EpisodeInput, ExtractionResult, FactInput, PendingInput};
pub use gate::{classify, GateRoute};
pub use store::store as store_extraction;
pub use working::WorkingMemory;
pub use budget::{allocate_and_compress, estimate_tokens};
pub use planner::{plan, Intent};
pub use brain_state::BrainState;
pub use grounding::{build_system_prompt, check_groundedness};
pub use retrieval::{retrieve, RetrievalResult, ScoreBreakdown, ScoredEpisode};
pub use trigger::{should_retrieve, TriggerDecision};

use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::llm::client::LlmClient;

/// Full ingestion pipeline: Gate -> Extractor -> Store.
/// This is the synchronous chain called per user message (architecture principle #4).
///
/// Returns the route taken, the extraction result (if any), and the stored episode ID.
pub async fn ingest(
    text: &str,
    conversation_id: &str,
    turn: i32,
    known_facts: &str,
    llm: &LlmClient,
    db: &DbState,
    embedding: Option<&EmbeddingService>,
) -> Result<IngestionOutcome, String> {
    // Step 1: Memory Gate classifies the input.
    let route = classify(text, llm).await?;

    match route {
        GateRoute::StoreFull => {
            // Step 2: Extract structured memory.
            let extraction = extract(text, known_facts, llm).await?;
            // Step 3: Store into DB + vectors.
            let episode_id = store_extraction(&extraction, conversation_id, turn, db, embedding)?;

            Ok(IngestionOutcome {
                route,
                extraction: Some(extraction),
                episode_id,
                correction: None,
                forget: None,
            })
        }

        GateRoute::EmotionOnly | GateRoute::Silence => {
            // For emotion-only and silence: extract just the emotion delta.
            let extraction = extract(text, known_facts, llm).await?;
            if let Some(delta) = &extraction.emotion_delta {
                let now = chrono::Utc::now().to_rfc3339();
                db.with_conn(|conn| store::apply_emotion_delta(conn, delta, &now))?;
            }

            Ok(IngestionOutcome {
                route,
                extraction: Some(extraction),
                episode_id: None,
                correction: None,
                forget: None,
            })
        }

        GateRoute::PendingEvent => {
            // Extract and store the pending event.
            let extraction = extract(text, known_facts, llm).await?;
            if let Some(pe) = &extraction.pending_event {
                let now = chrono::Utc::now().to_rfc3339();
                let pe_id = format!("pe_{}", uuid::Uuid::new_v4().simple());
                let remind_date = crate::mind::store::compute_remind_date(pe, &now);
                let event_date = pe
                    .event_date
                    .clone()
                    .unwrap_or_else(|| remind_date.clone().unwrap_or_else(|| now.clone()));
                let event = crate::db::pending::PendingEvent {
                    id: pe_id,
                    title: pe.title.clone(),
                    event_date,
                    remind_date,
                    source_episode: None,
                    status: "pending".to_string(),
                    importance: 0.5,
                    followup_count: 0,
                    created_at: now,
                    triggered_at: None,
                    resolved_at: None,
                };
                db.with_conn(|conn| crate::db::pending::insert(conn, &event))?;
            }

            Ok(IngestionOutcome {
                route,
                extraction: Some(extraction),
                episode_id: None,
                correction: None,
                forget: None,
            })
        }

        GateRoute::Correction => {
            // Hand off to the correction loop.
            let correction = handle_correction(text, known_facts, llm, db).await?;
            Ok(IngestionOutcome {
                route,
                extraction: None,
                episode_id: None,
                correction,
                forget: None,
            })
        }

        GateRoute::Forget => {
            // User asked to forget something. Scan episodes, facts, and pending
            // reminders for the single best confident match and erase it
            // (episode: hard delete + vector cleanup; fact: soft expire;
            // pending: resolve). Rust decides what to delete and refuses when
            // uncertain or for landmarks; the LLM only classified the intent
            // (Architecture Principle #1). No confident match -> she honestly
            // tells the user she doesn't remember it (never deletes the wrong
            // thing). The result is surfaced to converse so she can acknowledge
            // naturally without repeating the deleted content.
            let forget = forget_best_match(text, db, embedding)?;
            Ok(IngestionOutcome {
                route,
                extraction: None,
                episode_id: None,
                correction: None,
                forget: Some(forget),
            })
        }

        GateRoute::Discard => {
            // Pure noise, do nothing.
            Ok(IngestionOutcome {
                route,
                extraction: None,
                episode_id: None,
                correction: None,
                forget: None,
            })
        }

        GateRoute::Question => {
            // General-knowledge question: no memory operation. Skip the
            // extractor entirely (saves an LLM call) — extraction would only
            // return empty facts for a question anyway. Direct-answer mode is
            // handled downstream in converse.
            Ok(IngestionOutcome {
                route,
                extraction: None,
                episode_id: None,
                correction: None,
                forget: None,
            })
        }
    }
}

/// The outcome of processing a user message through the ingestion pipeline.
#[derive(Debug)]
pub struct IngestionOutcome {
    /// The route the gate assigned.
    pub route: GateRoute,
    /// The extraction result (if extraction was performed).
    pub extraction: Option<ExtractionResult>,
    /// The stored episode ID (if an episode was stored).
    pub episode_id: Option<String>,
    /// The correction result (if a correction was handled).
    pub correction: Option<CorrectionResult>,
    /// The forget outcome (if the user asked to forget a memory): Deleted /
    /// Declined / Ambiguous. Ambiguous triggers cross-turn disambiguation in
    /// converse (store candidates → ask back → resolve next turn).
    pub forget: Option<ForgetOutcome>,
}
