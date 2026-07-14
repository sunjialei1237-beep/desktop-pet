use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::AppConfig;
use crate::db::DbState;

/// Shared application state.
pub struct AppState {
    pub config: AppConfig,
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
    _state: State<'_, AppState>,
) -> Result<String, String> {
    log::info!("Received message: {}", text);
    Ok(format!("I heard you: {}", text))
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
