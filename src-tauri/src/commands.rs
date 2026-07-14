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
        llm_configured: state.llm.is_some(),
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

/// Checks for due pending events and returns a proactive action if appropriate.
#[tauri::command]
pub async fn check_proactive(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Option<crate::pending::ProactiveAction>, String> {
    let events = crate::pending::check_due(&db)?;

    let db_emotion = db.with_conn(|conn| crate::db::emotion::get(conn))?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let closeness = db
        .with_conn(|conn| crate::db::relationship::get(conn))
        .ok()
        .map(|r| r.closeness)
        .unwrap_or(0.0);

    let perception = crate::pending::PerceptionState {
        is_deep_focus: false,
        closeness,
    };

    let last_bubble = chrono::Utc::now() - chrono::Duration::minutes(31);
    let action = crate::pending::trigger_proactive(&events, &emotion, &perception, &last_bubble);

    if let Some(a) = &action {
        if let Some(eid) = &a.event_id {
            let _ = crate::pending::mark_triggered(&db, eid);
            let _ = crate::pending::increment_followup(&db, eid);
        }
    }

    Ok(action)
}

#[derive(Debug, Serialize)]
pub struct LlmConfigResponse {
    pub base_url: String,
    pub api_key_set: bool,
    pub main_model: String,
    pub reflection_model: String,
}

#[tauri::command]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfigResponse, String> {
    Ok(LlmConfigResponse {
        base_url: state.config.llm.base_url.clone(),
        api_key_set: !state.config.llm.api_key.is_empty(),
        main_model: state.config.llm.main_model.clone(),
        reflection_model: state.config.llm.reflection_model.clone(),
    })
}

#[tauri::command]
pub async fn update_llm_config(
    state: State<'_, AppState>,
    base_url: String,
    api_key: String,
    main_model: String,
    reflection_model: String,
) -> Result<(), String> {
    let mut config = state.config.clone();
    config.llm.base_url = base_url;
    if !api_key.is_empty() {
        config.llm.api_key = api_key;
    }
    config.llm.main_model = main_model.clone();
    config.llm.reflection_model = if reflection_model.is_empty() {
        main_model
    } else {
        reflection_model
    };
    crate::config::save_config(&config)?;
    log::info!("LLM config saved (restart to take effect)");
    Ok(())
}

#[tauri::command]
pub async fn get_llm_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.llm.is_some())
}

/// Resolves a pending event (user confirmed it).
#[tauri::command]
pub async fn resolve_pending_event(
    db: State<'_, DbState>,
    event_id: String,
) -> Result<(), String> {
    crate::pending::resolve(&db, &event_id)
}
