use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

use crate::config::AppConfig;
use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::working::WorkingMemory;

/// Shared application state.
pub struct AppState {
    pub config: AppConfig,
    pub llm: Option<LlmClient>,
    pub working_memory: Mutex<WorkingMemory>,
}

// -- Response types --

#[derive(Debug, Serialize, Deserialize)]
pub struct EmotionResponse {
    pub mood: f64,
    pub mood_label: String,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
}

impl From<crate::db::emotion::EmotionState> for EmotionResponse {
    fn from(e: crate::db::emotion::EmotionState) -> Self {
        EmotionResponse {
            mood: e.mood,
            mood_label: e.mood_label,
            physical_energy: e.physical_energy,
            social_battery: e.social_battery,
            stress: e.stress,
            loneliness: e.loneliness,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DebugData {
    pub llm_configured: bool,
    pub embedding_configured: bool,
    pub db_path: String,
    pub debug: bool,
}

// -- Tauri commands --

/// Handles a user message. For now returns a placeholder reply.
#[tauri::command]
pub async fn send_message(
    text: String,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<String, String> {
    log::info!("Received message: {}", text);

    // Snapshot working memory context.
    let wm_context = {
        let wm = state
            .working_memory
            .lock()
            .map_err(|e| format!("WM lock error: {}", e))?;
        wm.get_context()
    };

    let turn = (wm_context.len() / 2) as i32;
    let conversation_id = format!("conv_{}", chrono::Utc::now().timestamp());

    let llm = state
        .llm
        .as_ref()
        .ok_or("LLM not configured. Please set your API key in settings.")?;

    let result = crate::mind::converse::converse(
        &text,
        &conversation_id,
        turn,
        &wm_context,
        llm,
        &db,
        None,
    )
    .await?;

    // Push user + assistant messages to working memory.
    {
        let mut wm = state
            .working_memory
            .lock()
            .map_err(|e| format!("WM lock error: {}", e))?;
        wm.push(ChatMessage {
            role: "user".to_string(),
            content: text,
        });
        if !result.response.is_empty() {
            wm.push(ChatMessage {
                role: "assistant".to_string(),
                content: result.response.clone(),
            });
        }
    }

    if result.response.is_empty() {
        log::info!("Pet chose silence");
    }
    if !result.grounding_violations.is_empty() {
        log::warn!(
            "Grounding violations: {:?}",
            result.grounding_violations
        );
    }

    Ok(result.response)
}

/// Returns the current emotion state from the database.
#[tauri::command]
pub async fn get_emotion_state(db: State<'_, DbState>) -> Result<EmotionResponse, String> {
    db.with_conn(|conn| {
        let emo = crate::db::emotion::get(conn)?;
        Ok(EmotionResponse::from(emo))
    })
}

/// Returns debug data for the debug panel.
#[tauri::command]
pub async fn get_debug_data(state: State<'_, AppState>) -> Result<DebugData, String> {
    Ok(DebugData {
        llm_configured: !state.config.llm.api_key.is_empty(),
        embedding_configured: !state.config.embedding.model_dir.is_empty(),
        db_path: crate::config::resolve_db_path(&state.config)
            .to_string_lossy()
            .to_string(),
        debug: state.config.app.debug,
    })
}

/// Pet head interaction.
#[tauri::command]
pub async fn pet_head(_state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Pet head triggered");
    Ok(())
}

/// Poke interaction.
#[tauri::command]
pub async fn poke(_state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Poke triggered");
    Ok(())
}
