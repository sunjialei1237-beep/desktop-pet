use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::AppConfig;

/// Shared application state.
pub struct AppState {
    pub config: AppConfig,
}

// -- Response types --

#[derive(Debug, Serialize, Deserialize)]
pub struct EmotionState {
    pub mood: f64,
    pub mood_label: String,
    pub physical_energy: f64,
    pub social_battery: f64,
    pub stress: f64,
    pub loneliness: f64,
}

impl Default for EmotionState {
    fn default() -> Self {
        EmotionState {
            mood: 0.5,
            mood_label: "ping jing".to_string(),
            physical_energy: 0.7,
            social_battery: 0.8,
            stress: 0.2,
            loneliness: 0.0,
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
/// Will be wired to the full conversation pipeline in later phases.
#[tauri::command]
pub async fn send_message(
    text: String,
    _state: State<'_, AppState>,
) -> Result<String, String> {
    log::info!("Received message: {}", text);
    // P0 stub: simple echo-like reply.
    // Real implementation arrives in P5 (ingestion) -> P7 (planner).
    Ok(format!("I heard you: {}", text))
}

/// Returns the current emotion state. Stub for now.
#[tauri::command]
pub async fn get_emotion_state(
    _state: State<'_, AppState>,
) -> Result<EmotionState, String> {
    Ok(EmotionState::default())
}

/// Returns debug data for the debug panel.
#[tauri::command]
pub async fn get_debug_data(
    state: State<'_, AppState>,
) -> Result<DebugData, String> {
    Ok(DebugData {
        llm_configured: !state.config.llm.api_key.is_empty(),
        embedding_configured: !state.config.embedding.model_dir.is_empty(),
        db_path: crate::config::resolve_db_path(&state.config)
            .to_string_lossy()
            .to_string(),
        debug: state.config.app.debug,
    })
}

/// Pet head interaction. Stub for now, will trigger animation + emotion change.
#[tauri::command]
pub async fn pet_head(_state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Pet head triggered");
    Ok(())
}

/// Poke interaction. Stub for now.
#[tauri::command]
pub async fn poke(_state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Poke triggered");
    Ok(())
}
