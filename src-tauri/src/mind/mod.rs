pub mod correction;
pub mod extractor;
pub mod gate;
pub mod store;
pub mod working;
pub mod budget;
pub mod grounding;
pub mod retrieval;
pub mod trigger;
pub mod planner;
pub mod converse;

pub use correction::{handle_correction, CorrectionResult};
pub use extractor::{extract, EmotionDelta, EpisodeInput, ExtractionResult, FactInput, PendingInput};
pub use gate::{classify, GateRoute};
pub use store::store as store_extraction;
pub use working::WorkingMemory;
pub use budget::{allocate_and_compress, estimate_tokens};
pub use planner::{plan, Intent};
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
            })
        }

        GateRoute::PendingEvent => {
            // Extract and store the pending event.
            let extraction = extract(text, known_facts, llm).await?;
            if let Some(pe) = &extraction.pending_event {
                let now = chrono::Utc::now().to_rfc3339();
                let pe_id = format!("pe_{}", uuid::Uuid::new_v4().simple());
                let event = crate::db::pending::PendingEvent {
                    id: pe_id,
                    title: pe.title.clone(),
                    event_date: pe.event_date.clone(),
                    remind_date: crate::mind::store::compute_remind_date(&pe.event_date, &now),
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
            })
        }

        GateRoute::Discard => {
            // Pure noise, do nothing.
            Ok(IngestionOutcome {
                route,
                extraction: None,
                episode_id: None,
                correction: None,
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
}
