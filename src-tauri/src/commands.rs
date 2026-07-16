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

/// Handles a user message through the full conversation pipeline.
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

/// Triggers a reflection cycle if due (> 20 hours since last).
#[tauri::command]
pub async fn trigger_reflection_if_due(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<bool, String> {
    let llm = match state.llm.as_ref() {
        Some(l) => l,
        None => return Ok(false),
    };

    // Check if reflection should run (> 20 hours since last).
    let last_reflection: Option<String> = db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT MAX(created_at) FROM reflections",
            [],
            |row| row.get::<_, Option<String>>(0),
        ).unwrap_or(None))
    }).unwrap_or(None);

    let should_run = match last_reflection {
        None => true,
        Some(last) => chrono::DateTime::parse_from_rfc3339(&last)
            .map(|dt| (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_hours() > 20)
            .unwrap_or(true),
    };

    if !should_run {
        return Ok(false);
    }

    match crate::soul::reflection::run_reflection(
        crate::soul::reflection::ReflectionTrigger::Daily,
        &db, llm,
    ).await {
        Ok(r) => {
            log::info!("Reflection triggered: {}", r.summary);
            Ok(true)
        }
        Err(e) => {
            log::warn!("Reflection failed: {}", e);
            Ok(false)
        }
    }
}


/// Returns unsurfaced internal thought count (for debug panel + proactive surfacing).
#[tauri::command]
pub async fn get_pending_thoughts(
    db: State<'_, DbState>,
) -> Result<Vec<String>, String> {
    let thoughts = crate::soul::monologue::surface_thoughts(&db)?;
    Ok(thoughts.into_iter().map(|t| t.content).collect())
}

/// Returns the current perception snapshot (time, presence, window category).
#[tauri::command]
pub async fn get_perception(db: State<'_, DbState>) -> Result<crate::perception::PerceptionSnapshot, String> {
    let last_interaction = db.with_conn(|conn| {
        let rel = crate::db::relationship::get(conn)?;
        Ok(rel.last_interaction_at)
    })?;

    let since_last = match last_interaction {
        Some(ts) => crate::perception::time::seconds_since(&ts),
        None => 0,
    };

    let presence = crate::perception::presence::current_presence();
    let proc = crate::perception::window::foreground_process();
    let category = match &proc {
        Some(p) => crate::perception::window::classify_process(p),
        None => crate::perception::AppCategory::Other,
    };

    Ok(crate::perception::PerceptionSnapshot {
        time_of_day: crate::perception::time::current_time_of_day(),
        since_last_interaction_secs: since_last,
        presence,
        active_app: proc,
        app_category: category,
        continuous_work_secs: 0,
        is_deep_focus: false,
    })
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
pub async fn pet_head(db: State<'_, DbState>) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        crate::db::relationship::add_closeness(conn, 0.5, &now)?;
        let emo = crate::db::emotion::get(conn)?;
        let new_mood = (emo.mood + 0.05).min(1.0);
        let new_battery = (emo.social_battery - 0.01).max(0.0);
        crate::db::emotion::update_fields(
            conn,
            Some(new_mood), None, None,
            Some(new_battery), None, None, None, &now,
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Poke interaction.
#[tauri::command]
pub async fn poke(db: State<'_, DbState>, count: Option<i32>) -> Result<bool, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let poke_count = count.unwrap_or(1);
    db.with_conn(|conn| {
        crate::db::relationship::add_closeness(conn, 0.1, &now)?;
        let emo = crate::db::emotion::get(conn)?;
        let mood_delta = if poke_count >= 3 { -0.08 } else { -0.02 };
        let new_mood = (emo.mood + mood_delta).max(0.0);
        crate::db::emotion::update_fields(
            conn,
            Some(new_mood), None, None,
            None, None, None, None, &now,
        )?;
        Ok(())
    })?;
    Ok(poke_count >= 3)
}

/// Checks for due pending events and returns a proactive action if appropriate.
#[tauri::command]
pub async fn check_proactive(
    _state: State<'_, AppState>,
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

/// Full debug snapshot for the debug panel.
#[derive(Debug, Serialize)]
pub struct DebugSnapshot {
    pub emotion: EmotionResponse,
    pub closeness: f64,
    pub trust: f64,
    pub days_known: i64,
    pub total_conversations: i64,
    pub episode_count: i64,
    pub fact_count: i64,
    pub pending_count: i64,
    pub recent_episodes: Vec<DebugEpisode>,
    pub recent_facts: Vec<DebugFact>,
    pub pending_events: Vec<DebugPending>,
    pub llm_configured: bool,
}

#[derive(Debug, Serialize)]
pub struct DebugEpisode {
    pub id: String,
    pub summary: String,
    pub strength: f64,
    pub recall_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DebugFact {
    pub category: String,
    pub key: String,
    pub value: String,
    pub confidence: f64,
}

#[derive(Debug, Serialize)]
pub struct DebugPending {
    pub id: String,
    pub title: String,
    pub status: String,
    pub remind_date: Option<String>,
}

/// Returns a full debug snapshot for the debug panel.
#[tauri::command]
pub async fn get_debug_snapshot(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<DebugSnapshot, String> {
    db.with_conn(|conn| {
        let emo = crate::db::emotion::get(conn)?;
        let rel = crate::db::relationship::get(conn).ok();

        let episode_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM episodes", [], |row| row.get(0))
            .unwrap_or(0);
        let fact_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM facts WHERE valid_to IS NULL", [], |row| row.get(0))
            .unwrap_or(0);
        let pending_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_events WHERE status = 'pending'", [], |row| row.get(0))
            .unwrap_or(0);

        // Recent episodes (top 10 by strength).
        let mut stmt = conn
            .prepare("SELECT id, summary, memory_strength, recall_count FROM episodes ORDER BY memory_strength DESC LIMIT 10")
            .map_err(|e| format!("Prepare error: {}", e))?;
        let recent_episodes: Vec<DebugEpisode> = stmt
            .query_map([], |row| Ok(DebugEpisode {
                id: row.get(0)?,
                summary: row.get(1)?,
                strength: row.get(2)?,
                recall_count: row.get(3)?,
            }))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        // Active facts.
        let mut stmt = conn
            .prepare("SELECT category, key, value, confidence FROM facts WHERE valid_to IS NULL ORDER BY confidence DESC LIMIT 20")
            .map_err(|e| format!("Prepare error: {}", e))?;
        let recent_facts: Vec<DebugFact> = stmt
            .query_map([], |row| Ok(DebugFact {
                category: row.get(0)?,
                key: row.get(1)?,
                value: row.get(2)?,
                confidence: row.get(3)?,
            }))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        // Pending events.
        let mut stmt = conn
            .prepare("SELECT id, title, status, remind_date FROM pending_events ORDER BY created_at DESC LIMIT 10")
            .map_err(|e| format!("Prepare error: {}", e))?;
        let pending_events: Vec<DebugPending> = stmt
            .query_map([], |row| Ok(DebugPending {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                remind_date: row.get(3)?,
            }))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(DebugSnapshot {
            emotion: EmotionResponse::from(emo),
            closeness: rel.as_ref().map(|r| r.closeness).unwrap_or(0.0),
            trust: rel.as_ref().map(|r| r.trust).unwrap_or(0.0),
            days_known: rel.as_ref().map(|r| r.days_known).unwrap_or(0),
            total_conversations: rel.as_ref().map(|r| r.total_conversations).unwrap_or(0),
            episode_count,
            fact_count,
            pending_count,
            recent_episodes,
            recent_facts,
            pending_events,
            llm_configured: state.llm.is_some(),
        })
    })
}
