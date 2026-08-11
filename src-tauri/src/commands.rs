use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

use crate::config::AppConfig;
use crate::db::DbState;
use crate::db::onboarding as db_onboarding;
use crate::embedding::{EmbeddingService, ModelDownloader};
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::working::WorkingMemory;

/// Result of `send_message`: the LLM reply plus an optional transient
/// expression id (f00..f07) to flash for ~8s before reverting to the
/// accumulated mood label. None = keep the accumulated expression.
#[derive(serde::Serialize)]
pub struct SendMessageResult {
    pub reply: String,
    pub transient_expression: Option<String>,
}

/// Shared application state.
pub struct AppState {
    pub config: AppConfig,
    pub llm: std::sync::Mutex<Option<LlmClient>>,
    pub embedding: EmbeddingService,
    pub working_memory: Mutex<WorkingMemory>,
    pub question_pacing: std::sync::Mutex<crate::mind::pacing::QuestionPacing>,
    /// Last conversation turn's decision-chain trace for the debug panel
    /// (Architecture #11: "她为什么这么说" — intent + retrieval + trigger +
    /// violations). Written in send_message, read in get_debug_snapshot.
    pub last_decision: std::sync::Mutex<Option<DecisionTrace>>,
    /// Last proactive-bubble emission time (frequency gate). None = never.
    /// Updated in check_proactive the moment a bubble is greenlit — before
    /// proactive_bubble runs — so the 5-min frontend poll can't re-fire within
    /// the interval even if generation later fails (conservative: 宁少勿突兀).
    pub last_proactive_bubble: std::sync::Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    /// Pending cross-turn forget disambiguation (None normally). Holds the ≥2
    /// candidate memories from a "忘掉X" that matched several, awaiting the
    /// user's clarifying reply. Mirrors `question_pacing` as a Mutex slot.
    pub pending_forget: std::sync::Mutex<Option<crate::mind::forget::PendingForget>>,
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
    pub rest_need: f64,
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
            rest_need: e.rest_need,
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
    on_chunk: tauri::ipc::Channel<String>,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<SendMessageResult, String> {
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
        .lock()
        .map_err(|e| format!("LLM lock error: {}", e))?
        .as_ref()
        .cloned()
        .ok_or("LLM not configured. Please set your API key in settings.")?;

    let ctx = crate::mind::converse::ConverseCtx {
        text: &text,
        conversation_id: &conversation_id,
        turn,
        wm_context: &wm_context,
        llm: &llm,
        db: &db,
        embedding: Some(&state.embedding),
        pacing: &state.question_pacing,
        pending_forget: &state.pending_forget,
    };
    let result = crate::mind::converse::converse(
        &ctx,
        // Forward each streamed content token to the frontend through an
        // ipc::Channel for live bubble rendering (architecture #10). Channel
        // is Tauri's intended path for streaming data *during* a command:
        // unlike emit/listen (whose events fired inside an async command are
        // only delivered after the command returns — by which point a
        // tightly-scoped listener has already unlistened, so chat-chunk was
        // silently lost), Channel is a direct command-parameter pipe that
        // delivers in real time. The fully accumulated reply also comes back
        // in `result.response`.
        {
            let mut logged = false;
            move |delta: &str| {
                if !logged {
                    logged = true;
                    log::info!("[chat-stream] first content chunk forwarded to channel");
                }
                if let Err(e) = on_chunk.send(delta.to_string()) {
                    log::warn!("[chat-stream] channel send failed: {}", e);
                }
            }
        },
    )
    .await?;

    // Transient expression for this turn (computed before `text` is moved into
    // working memory). Pure rules, no LLM (architecture principle #8).
    let transient = crate::emotion::react::transient_expression(&text, &result.intent.goal)
        .map(|s| s.to_string());

    // Persist raw conversation turn(s) to the durable `conversations` table for
    // source traceability (Architecture Principle #11: every memory decision
    // must trace back to the raw wording; episodes link back via
    // source_conversation_id). `working_memory` (below) is in-memory and
    // ephemeral — this is the durable log that lets you recall her exact reply
    // later (e.g. diagnosing a hallucination). Best-effort: a logging failure
    // warns but never breaks the chat. Harnesses call converse() directly (not
    // send_message), so they don't pollute the production table.
    {
        let now_ts = chrono::Utc::now().to_rfc3339();
        let user_row = crate::db::conversations::ConversationRow {
            id: format!("{}_t{}_user", conversation_id, turn),
            turn: turn as i64,
            role: "user".to_string(),
            content: text.clone(),
            created_at: now_ts.clone(),
        };
        if let Err(e) = db.with_conn(|conn| crate::db::conversations::insert(conn, &user_row)) {
            log::warn!("[conversations] failed to log user turn: {}", e);
        }
        if !result.response.is_empty() {
            let asst_row = crate::db::conversations::ConversationRow {
                id: format!("{}_t{}_assistant", conversation_id, turn),
                turn: turn as i64,
                role: "assistant".to_string(),
                content: result.response.clone(),
                created_at: now_ts,
            };
            if let Err(e) = db.with_conn(|conn| crate::db::conversations::insert(conn, &asst_row))
            {
                log::warn!("[conversations] failed to log assistant turn: {}", e);
            }
        }
    }

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

    // Stash this turn's decision chain for the debug panel (Architecture #11:
    // answers "她为什么这么说" — Intent + retrieval + trigger + violations).
    // Best-effort: a lock failure only skips the debug update, never breaks chat.
    {
        let trace = DecisionTrace {
            at: chrono::Utc::now().to_rfc3339(),
            intent_goal: result.intent.goal.clone(),
            intent_tone: result.intent.tone.clone(),
            intent_action: result.intent.action.clone(),
            memory_anchor: result.intent.memory_anchor.clone(),
            trigger_reason: result.trigger_reason.clone(),
            route: format!("{:?}", result.route),
            grounding_violations: result.grounding_violations.clone(),
            retrieved: result
                .retrieved_scores
                .iter()
                .map(|r| DecisionRetrieved {
                    summary: r.summary.clone(),
                    score: r.score,
                    semantic: r.semantic,
                    strength: r.strength,
                    recency: r.recency,
                    emotion: r.emotion,
                })
                .collect(),
            prompt_tokens: result.prompt_tokens.as_ref().map(|p| DecisionPromptToken {
                system_tokens: p.system_tokens,
                input_tokens: p.input_tokens,
                budget: p.budget,
                conversation_turns: p.conversation_turns,
            }),
        };
        if let Ok(mut guard) = state.last_decision.lock() {
            *guard = Some(trace);
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

    Ok(SendMessageResult {
        reply: result.response,
        transient_expression: transient,
    })
}

/// Triggers a reflection cycle if due (> 20 hours since last).
/// Thin wrapper over `soul::reflection::maybe_run_if_due` (Architecture
/// Principle 1: logic lives in modules, the command layer stays thin). Swallows
/// errors to keep the frontend contract — returns bool, never errors to JS.
#[tauri::command]
pub async fn trigger_reflection_if_due(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<bool, String> {
    let llm = match state.llm.lock().ok().and_then(|g| g.clone()) {
        Some(l) => l,
        None => return Ok(false),
    };
    match crate::soul::reflection::maybe_run_if_due(&db, &llm).await {
        Ok(ran) => Ok(ran),
        Err(e) => {
            log::warn!("Reflection failed: {}", e);
            Ok(false)
        }
    }
}


/// Forces a reflection cycle immediately (for development/testing).
/// Bypasses the 20-hour cooldown check.
#[tauri::command]
pub async fn force_reflection(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<bool, String> {
    let llm = match state.llm.lock().ok().and_then(|g| g.clone()) {
        Some(l) => l,
        None => return Ok(false),
    };
    match crate::soul::reflection::run_reflection(
        crate::soul::reflection::ReflectionTrigger::TurnThreshold,
        &db, &llm,
    ).await {
        Ok(r) => {
            log::info!("Forced reflection: {}", r.summary);
            Ok(true)
        }
        Err(e) => {
            log::warn!("Forced reflection failed: {}", e);
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
pub async fn get_perception(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<crate::perception::PerceptionSnapshot, String> {
    let cfg = &state.config.perception;

    let last_interaction = db.with_conn(|conn| {
        let rel = crate::db::relationship::get(conn)?;
        Ok(rel.last_interaction_at)
    })?;

    let since_last = if cfg.enable_time {
        match last_interaction {
            Some(ts) => crate::perception::time::seconds_since(&ts),
            None => 0,
        }
    } else {
        0
    };

    let presence = if cfg.enable_presence {
        crate::perception::presence::current_presence()
    } else {
        crate::perception::PresenceState::Active
    };

    let (proc, category) = if cfg.enable_window {
        let p = crate::perception::window::foreground_process();
        let c = match &p {
            Some(name) => crate::perception::window::classify_process(name),
            None => crate::perception::AppCategory::Other,
        };
        (p, c)
    } else {
        (None, crate::perception::AppCategory::Other)
    };

    // Deep-focus tracking is derived from window perception (P14.3): same
    // Work-category foreground app held >= 25 min. Disabled when window
    // perception is off (Principle 6: degrade to "never deep focus").
    let (continuous_work_secs, is_deep_focus) = if cfg.enable_window {
        (
            crate::perception::focus::continuous_work_secs(),
            crate::perception::focus::is_deep_focus(),
        )
    } else {
        (0, false)
    };

    Ok(crate::perception::PerceptionSnapshot {
        time_of_day: crate::perception::time::current_time_of_day(),
        since_last_interaction_secs: since_last,
        presence,
        active_app: proc,
        app_category: category,
        continuous_work_secs,
        is_deep_focus,
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
        llm_configured: state.llm.lock().map(|g| g.is_some()).unwrap_or(false),
        embedding_configured: state.embedding.is_ready(),
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
        // Genuine affection relieves loneliness — being petted is the direct
        // opposite of being ignored. ~0.1 offsets ~15min of idle growth, so one
        // pet visibly comforts her without trivializing the slow build-up.
        let new_loneliness = (emo.loneliness - 0.1).max(0.0);
        crate::db::emotion::update_fields(
            conn,
            Some(new_mood), None, None,
            Some(new_battery), None, Some(new_loneliness), None, &now,
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
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Option<crate::pending::ProactiveAction>, String> {
    let events = crate::pending::check_due(&db)?;

    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let closeness = db
        .with_conn(crate::db::relationship::get)
        .ok()
        .map(|r| r.closeness)
        .unwrap_or(0.0);

    // Deep-focus suppression (P14.3): stay quiet during sustained work. Disabled
    // when window perception is off (Principle 6).
    let is_deep_focus = if state.config.perception.enable_window {
        crate::perception::focus::is_deep_focus()
    } else {
        false
    };

    let perception = crate::pending::PerceptionState {
        is_deep_focus,
        closeness,
    };

    // Real last-bubble time (was hardcoded to now-31min, which always passed the
    // 30-min gate → bubbles fired every 5-min poll). None = never → sentinel a
    // century back so elapsed is huge and the first bubble is allowed.
    let last_bubble = state
        .last_proactive_bubble
        .lock()
        .map_err(|e| format!("proactive lock error: {}", e))?
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(36500));
    let action = crate::pending::trigger_proactive(
        &events,
        &emotion,
        &perception,
        &last_bubble,
        state.config.proactive.min_interval_secs,
    );
    // Occupy this interval the instant a bubble is greenlit — before
    // proactive_bubble generates the text — so the 5-min frontend poll can't
    // re-trigger within min_interval_secs even if generation later returns None.
    if action.is_some() {
        if let Ok(mut t) = state.last_proactive_bubble.lock() {
            *t = Some(chrono::Utc::now());
        }
    }

    if let Some(a) = &action {
        if let Some(eid) = &a.event_id {
            let _ = crate::pending::mark_triggered(&db, eid);
            let _ = crate::pending::increment_followup(&db, eid);
        }
    }

    Ok(action)
}
/// Generates a memory-grounded proactive bubble. Picks one memory anchor
/// (due pending event > high-confidence fact > top episode), runs the same
/// budget/grounding pipeline as a normal turn but with a proactive intent,
/// and returns the LLM's reply. This is the MVP loop's "主动提起" step.
/// Returns Ok(None) when there is nothing worth proactively bringing up.
#[tauri::command]
pub async fn proactive_bubble(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Option<String>, String> {
    let llm = state
        .llm
        .lock()
        .map_err(|e| format!("LLM lock error: {}", e))?
        .as_ref()
        .cloned()
        .ok_or("LLM not configured")?;

    let wm_context = {
        let wm = state
            .working_memory
            .lock()
            .map_err(|e| format!("WM lock error: {}", e))?;
        wm.get_context()
    };

    // Business logic lives in pending::proactive::generate so the closed-loop-2
    // path is testable without AppState (Architecture Principle 1: thin command
    // layer; logic in modules). The command's IPC contract stays Option<String>
    // (the reply); the anchor is dropped here — it is consumed only by tests and
    // (eventually) the Debug Panel, not the frontend bubble.
    let outcome =
        crate::pending::proactive::generate(&db, &llm, Some(&state.embedding), &wm_context).await?;
    Ok(outcome.map(|o| o.reply))
}

/// Generates a welcome-back bubble after the user returns from >5min away
/// (detected via the presence loop). Tries the memory-grounded LLM path first;
/// falls back to a rule-based line when the LLM is unconfigured or returns
/// nothing (Architecture Principle 8: degrade gracefully, never break).
#[tauri::command]
pub async fn welcome_back_bubble(
    away_secs: u64,
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Option<String>, String> {
    let wm_context = {
        let wm = state
            .working_memory
            .lock()
            .map_err(|e| format!("WM lock error: {}", e))?;
        wm.get_context()
    };

    // Try the memory-grounded LLM path if configured.
    // NOTE: bind the cloned Option to its own `let` so the MutexGuard is dropped
    // at the end of the statement — keeping it alive across `.await` below would
    // make the future non-Send (same pattern as `proactive_bubble`).
    let llm = state
        .llm
        .lock()
        .map_err(|e| format!("LLM lock error: {}", e))?
        .as_ref()
        .cloned();
    if let Some(llm) = llm {
        let outcome = crate::pending::proactive::generate_welcome_back(
            &db,
            &llm,
            Some(&state.embedding),
            &wm_context,
            away_secs,
        )
        .await?;
        if let Some(o) = outcome {
            return Ok(Some(o.reply));
        }
        // LLM returned nothing (empty reply) → fall through to canned rule line.
    }

    // Fallback: rule-based canned line, mood-scaled. Never fails.
    let mood = db
        .with_conn(crate::db::emotion::get)
        .map(|e| e.mood)
        .unwrap_or(0.5);
    Ok(Some(
        crate::emotion::react::welcome_back_canned(mood, away_secs).to_string(),
    ))
}

/// Generates a loneliness-driven "想你了" bubble when the user has been idle at
/// the desk (homeostasis let loneliness climb) and the relationship is
/// established. The loop_runner gates emission (loneliness threshold + closeness
/// + presence + cooldown); this voices it once the frontend asks. Tries the
/// memory-grounded LLM path first; falls back to a rule-based line when the LLM
/// is unconfigured or returns nothing (Architecture Principle 8).
#[tauri::command]
pub async fn lonely_bubble(
    state: State<'_, AppState>,
    db: State<'_, DbState>,
) -> Result<Option<String>, String> {
    let wm_context = {
        let wm = state
            .working_memory
            .lock()
            .map_err(|e| format!("WM lock error: {}", e))?;
        wm.get_context()
    };

    // NOTE: bind the cloned Option to its own `let` so the MutexGuard is dropped
    // at the end of the statement — keeping it alive across `.await` below would
    // make the future non-Send (same pattern as `welcome_back_bubble`).
    let llm = state
        .llm
        .lock()
        .map_err(|e| format!("LLM lock error: {}", e))?
        .as_ref()
        .cloned();
    if let Some(llm) = llm {
        let outcome = crate::pending::proactive::generate_lonely_bubble(
            &db,
            &llm,
            Some(&state.embedding),
            &wm_context,
        )
        .await?;
        if let Some(o) = outcome {
            return Ok(Some(o.reply));
        }
        // LLM returned nothing (empty reply) → fall through to canned rule line.
    }

    // Fallback: rule-based canned line, mood-scaled. Never fails.
    let mood = db
        .with_conn(crate::db::emotion::get)
        .map(|e| e.mood)
        .unwrap_or(0.5);
    Ok(Some(crate::emotion::react::lonely_canned(mood).to_string()))
}
/// Checks for due cold-start interview questions (first 3 days).
/// These bypass the closeness gate to help build the relationship early on.
/// Returns the next due interview question, or None if none are due.
#[tauri::command]
pub async fn check_cold_start(db: State<'_, DbState>) -> Result<Option<String>, String> {
    let now = chrono::Utc::now().to_rfc3339();

    let question: Option<String> = db.with_conn(|conn| {
        let event: Option<crate::db::pending::PendingEvent> = conn
            .query_row(
                "SELECT id, title, event_date, remind_date, source_episode,
                        status, importance, followup_count, created_at, triggered_at, resolved_at
                 FROM pending_events
                 WHERE status = 'interview' AND remind_date IS NOT NULL AND remind_date <= ?1
                 ORDER BY remind_date ASC LIMIT 1",
                rusqlite::params![now],
                |row| {
                    Ok(crate::db::pending::PendingEvent {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        event_date: row.get(2)?,
                        remind_date: row.get(3)?,
                        source_episode: row.get(4)?,
                        status: row.get(5)?,
                        importance: row.get(6)?,
                        followup_count: row.get(7)?,
                        created_at: row.get(8)?,
                        triggered_at: row.get(9)?,
                        resolved_at: row.get(10)?,
                    })
                },
            )
            .ok();

        if let Some(ev) = event {
            // Mark as triggered so it doesn't fire again.
            let _ = crate::db::pending::mark_triggered(conn, &ev.id, &now);
            Ok(Some(ev.title))
        } else {
            Ok(None)
        }
    })?;

    Ok(question)
}

// -- Onboarding (first-launch interview) --
// The pet asks 4 questions on first launch; answers are persisted in app_config
// and injected into the system prompt's [Persona] section.

/// Returns true if the first-launch interview has not been completed yet.
#[tauri::command]
pub async fn needs_onboarding(db: State<'_, DbState>) -> Result<bool, String> {
    db.with_conn(|conn| db_onboarding::needs_onboarding(conn))
}

/// Saves one onboarding answer (key is one of: user_nickname, personality_style,
/// relationship_style, pet_name).
#[tauri::command]
pub async fn save_onboarding_answer(
    db: State<'_, DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    db.with_conn(|conn| db_onboarding::save(conn, &key, &value))
}

/// Marks the interview complete so it never fires again.
#[tauri::command]
pub async fn complete_onboarding(db: State<'_, DbState>) -> Result<(), String> {
    db.with_conn(|conn| db_onboarding::save(conn, "onboard_completed", "true"))
}

/// Loads the full onboarding profile for the system prompt.
#[tauri::command]
pub async fn get_user_profile(
    db: State<'_, DbState>,
) -> Result<db_onboarding::UserProfile, String> {
    db.with_conn(|conn| db_onboarding::load(conn))
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
    log::info!("LLM config saved, reinitializing client");

    // Reinitialize the LLM client immediately so the user can chat without restart.
    let new_llm = crate::llm::client::LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    ).ok();

    if let Ok(mut guard) = state.llm.lock() {
        *guard = new_llm;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_llm_status(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.llm.lock().map(|g| g.is_some()).unwrap_or(false))
}

/// Checks the embedding model status: whether files exist on disk and are loaded.
#[derive(Debug, Serialize)]
pub struct EmbeddingStatus {
    pub ready: bool,
    pub files_present: bool,
    pub model_dir: String,
    pub missing_files: Vec<String>,
}

#[tauri::command]
pub async fn get_embedding_status(
    state: State<'_, AppState>,
) -> Result<EmbeddingStatus, String> {
    let model_dir = state.embedding.model_dir().to_path_buf();
    let downloader = ModelDownloader::new(&model_dir);
    let missing = downloader.missing_files();
    let files_present = missing.is_empty();
    Ok(EmbeddingStatus {
        ready: state.embedding.is_ready(),
        files_present,
        model_dir: model_dir.to_string_lossy().to_string(),
        missing_files: missing,
    })
}

/// Downloads the BGE-M3 embedding model files. Emits progress via events.
/// After download completes, loads the model into memory.
#[tauri::command]
pub async fn download_embedding_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let model_dir = state.embedding.model_dir().to_path_buf();
    let downloader = ModelDownloader::new(&model_dir);

    if downloader.check_complete() {
        log::info!("Embedding model already downloaded");
        if !state.embedding.is_ready() {
            state.embedding.load().map_err(|e| format!("{}", e))?;
        }
        return Ok(true);
    }

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("HTTP client: {}", e))?;

    let app_handle = app.clone();
    let progress: crate::embedding::ProgressCallback = Box::new(move |p| {
        let _ = app_handle.emit(
            "download-progress",
            serde_json::json!({
                "file_name": p.file_name,
                "downloaded": p.downloaded,
                "total": p.total,
                "fraction": p.fraction,
            }),
        );
    });

    downloader
        .download_all(&http, Some(&progress))
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    log::info!("Embedding model downloaded, loading into memory");
    state.embedding.load().map_err(|e| format!("Load failed: {}", e))?;

    let _ = app.emit("embedding-ready", serde_json::json!({ "ready": true }));
    Ok(true)
}

/// Resolves a pending event (user confirmed it).
#[tauri::command]
pub async fn resolve_pending_event(
    db: State<'_, DbState>,
    event_id: String,
) -> Result<(), String> {
    crate::pending::resolve(&db, &event_id)
}

/// Last turn's decision-chain trace (Architecture #11 Explainability).
/// Answers "她为什么这么说": the Intent that directed the reply, what memory
/// retrieval surfaced (with score breakdown), why retrieval fired, and any
/// grounding violations. Captured per turn in send_message.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionTrace {
    pub at: String,
    pub intent_goal: String,
    pub intent_tone: String,
    pub intent_action: String,
    pub memory_anchor: String,
    pub trigger_reason: String,
    pub route: String,
    pub grounding_violations: Vec<String>,
    pub retrieved: Vec<DecisionRetrieved>,
    /// Last turn's prompt token budget (#8/#11). None on silence.
    pub prompt_tokens: Option<DecisionPromptToken>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionRetrieved {
    pub summary: String,
    pub score: f64,
    pub semantic: f64,
    pub strength: f64,
    pub recency: f64,
    pub emotion: f64,
}

/// Last turn's prompt-budget observability (#8/#11). Serialized mirror of
/// converse::PromptTokenDebug so the snapshot stays decoupled from the mind
/// module's types.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionPromptToken {
    pub system_tokens: usize,
    pub input_tokens: usize,
    pub budget: usize,
    pub conversation_turns: usize,
}

/// Latest reflection summary for the debug panel (#11: Soul observability).
#[derive(Debug, Clone, Serialize)]
pub struct DebugReflect {
    pub last_thought: Option<String>,
    pub last_at: Option<String>,
    pub unsurfaced_thoughts: i64,
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
    pub change_log: Vec<crate::db::changelog::ChangeLogEntry>,
    /// Last turn's decision chain (Intent + retrieval + trigger + violations).
    pub last_decision: Option<DecisionTrace>,
    /// Latest reflection + unsurfaced thought count.
    pub reflect: DebugReflect,
    /// Today's LLM call count + token totals (Architecture #8).
    pub cost: crate::llm::client::LlmCostStats,
    pub llm_configured: bool,
    /// Deep-focus tracking (P14.3): sustained same-Work-app foreground time.
    pub continuous_work_secs: u64,
    pub is_deep_focus: bool,
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
    pub id: String,
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
            .prepare("SELECT id, category, key, value, confidence FROM facts WHERE valid_to IS NULL ORDER BY confidence DESC LIMIT 20")
            .map_err(|e| format!("Prepare error: {}", e))?;
        let recent_facts: Vec<DebugFact> = stmt
            .query_map([], |row| Ok(DebugFact {
                id: row.get(0)?,
                category: row.get(1)?,
                key: row.get(2)?,
                value: row.get(3)?,
                confidence: row.get(4)?,
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

        // Soul observability (#11): latest reflection + unsurfaced thought count.
        let last_thought: Option<String> = conn
            .query_row(
                "SELECT thought FROM reflections ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        let last_at: Option<String> = conn
            .query_row(
                "SELECT created_at FROM reflections ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        let unsurfaced_thoughts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM internal_thoughts WHERE surfaced_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let reflect = DebugReflect {
            last_thought,
            last_at,
            unsurfaced_thoughts,
        };
        // Last turn's decision chain (None until the first message is sent).
        let last_decision = state.last_decision.lock().ok().and_then(|g| g.clone());

        // Today's LLM cost (Architecture #8). Unconfigured or lock-poison → empty.
        let cost = match state.llm.lock() {
            Ok(g) => match g.as_ref() {
                Some(c) => c.cost_today(),
                None => crate::llm::client::LlmCostStats::default(),
            },
            Err(_) => crate::llm::client::LlmCostStats::default(),
        };

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
            change_log: crate::db::changelog::recent(conn, 20).unwrap_or_default(),
            last_decision,
            reflect,
            cost,
            llm_configured: state.llm.lock().map(|g| g.is_some()).unwrap_or(false),
            continuous_work_secs: if state.config.perception.enable_window {
                crate::perception::focus::continuous_work_secs()
            } else {
                0
            },
            is_deep_focus: if state.config.perception.enable_window {
                crate::perception::focus::is_deep_focus()
            } else {
                false
            },
        })
    })
}

/// Scheduler registry snapshot (plan §A2, ADR 2026-08-08): one row per
/// scheduled job — cadence / enable flag / last-run time / last status.
/// Surfaced in the Debug Panel for explainability (#11). Read-only: the
/// toggles live in config ([scheduler]).
#[tauri::command]
pub fn get_scheduler_stats() -> Vec<crate::lifecycle::JobStat> {
    crate::lifecycle::scheduler_snapshot()
}

// ── Memory curation commands (Debug Panel: read-only → editable) ──────────
// These let a human correct the pet's memory directly — forget a wrong fact,
// drop a junk episode, cancel a stray reminder, or nudge her mood to test an
// animation. All are best-effort + logged to the change_log (Architecture #11:
// every memory decision traces to a reason). They reuse existing DB accessors
// rather than raw SQL, so the same soft-delete / vector-cleanup semantics as
// the automated flows apply.

/// Forgets one fact by id (precise soft-delete via `valid_to`, preserving the
/// audit trail and the `dedup_insert` revive path — same op the user-directed
/// Forget route uses). The pet simply stops surfacing it until it's restated.
#[tauri::command]
pub async fn forget_fact(db: State<'_, DbState>, id: String) -> Result<bool, String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        let expired = crate::db::facts::expire_by_id(conn, &id, &now)?;
        if expired {
            let _ = crate::db::changelog::log_change(
                conn, "debug", &id, "valid_to", "", &now, "manual forget (debug panel)",
            );
        }
        Ok(expired)
    })
}

/// Deletes one episode by id AND its embedding vector (so retrieval stays
// consistent — a dangling vector would still match). Refuses landmarks (the
/// automated `episodes::delete` guard; a landmark is a core memory).
#[tauri::command]
pub async fn delete_episode(db: State<'_, DbState>, id: String) -> Result<bool, String> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        let removed = crate::db::episodes::delete(conn, &id)?;
        if removed {
            // Keep the vector store in sync — a deleted episode's vector must
            // not survive or it still scores in cosine search.
            let _ = crate::db::vectors::delete(conn, &id);
            let _ = crate::db::changelog::log_change(
                conn, "debug", &id, "deleted", "", &now, "manual delete (debug panel)",
            );
        }
        Ok(removed)
    })
}

/// Cancels (resolves) one pending event by id — a stray or wrong reminder the
/// user doesn't want firing. Reuses the production `resolve_pending_event`
/// command from the frontend (no separate debug path — same resolution logic).

/// Manual emotion fields for the debug editor. Only provided (Some) fields are
/// changed — the rest hold their current value (Architecture #2: partial
/// update, not a full-state overwrite).
#[derive(Deserialize)]
pub struct EmotionEdit {
    pub mood: Option<f64>,
    pub physical_energy: Option<f64>,
    pub social_battery: Option<f64>,
    pub stress: Option<f64>,
    pub loneliness: Option<f64>,
}

/// Sets emotion fields directly — for testing animation/emotion 手感 (e.g. drop
/// mood to 0.1 to confirm the sad face fires). Writes the fields, re-derives
/// the mood label from the merged state, then emits `emotion-update`
/// immediately so the pet's face reflects the override without waiting for the
/// 30s medium tick (Architecture #10 liveliness).
#[tauri::command]
pub async fn set_emotion(
    app: tauri::AppHandle,
    db: State<'_, DbState>,
    edit: EmotionEdit,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let emo = db.with_conn(|conn| {
        crate::db::emotion::update_fields(
            conn, edit.mood, None, edit.physical_energy, edit.social_battery,
            edit.stress, edit.loneliness, None, &now,
        )?;
        // Re-derive the label from the merged values so the panel and pet agree
        // on "calm"/"tense"/etc. for the manually-set mood.
        let raw = crate::db::emotion::get(conn)?;
        let merged = crate::emotion::state::EmotionState {
            mood: raw.mood,
            physical_energy: raw.physical_energy,
            social_battery: raw.social_battery,
            stress: raw.stress,
            loneliness: raw.loneliness,
            rest_need: raw.rest_need,
        };
        let label = crate::emotion::state::derive_mood_label(&merged);
        crate::db::emotion::update_fields(
            conn, None, Some(&label), None, None, None, None, None, &now,
        )?;
        let _ = crate::db::changelog::log_change(
            conn, "debug", "emotion_state", "manual", "", &label,
            "manual set (debug panel)",
        );
        Ok::<_, String>(raw)
    })?;
    // Push immediately — same payload shape as the medium tick's emotion push,
    // so the frontend's existing listener updates the pet's face at once.
    let _ = app.emit(
        "emotion-update",
        serde_json::json!({
            "mood": emo.mood,
            "mood_label": crate::emotion::state::derive_mood_label(&crate::emotion::state::EmotionState {
                mood: emo.mood, physical_energy: emo.physical_energy,
                social_battery: emo.social_battery, stress: emo.stress,
                loneliness: emo.loneliness, rest_need: emo.rest_need,
            }),
            "physical_energy": emo.physical_energy,
            "social_battery": emo.social_battery,
            "stress": emo.stress,
            "loneliness": emo.loneliness,
            "rest_need": emo.rest_need,
        }),
    );
    Ok(())
}

/// Opens the webview developer tools window (used by the context-menu entry).
/// `WebviewWindow::open_devtools()` is a debug-only API in Tauri (the method
/// does not exist in release builds), so this command is a no-op outside debug
/// — it stays registered so the context-menu invoke never errors in release.
#[tauri::command]
pub fn open_devtools(app_handle: tauri::AppHandle) {
    if let Some(win) = app_handle.get_webview_window("main") {
        #[cfg(debug_assertions)]
        win.open_devtools();
        #[cfg(not(debug_assertions))]
        let _ = win; // release: devtools API unavailable, no-op
        log::info!("devtools invoked via context menu");
    } else {
        log::warn!("open_devtools: main webview window not found");
    }
}

/// Open the Debug Panel as a separate OS window (F12 / Ctrl+Shift+D). The main
/// pet window is only 400×760 and transparent, so any in-window debug overlay
/// covers Liri's body — a standalone window can be dragged anywhere on screen
/// via its native title bar and never obscures the pet. It reuses index.html
/// with `?window=debug`; main.tsx branches on that query to render
/// DebugStandalone instead of App. If already open, just focus it.
#[tauri::command]
pub fn open_debug_window(app_handle: tauri::AppHandle) {
    if let Some(win) = app_handle.get_webview_window("debug") {
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    use tauri::webview::WebviewWindowBuilder;
    if let Err(e) = WebviewWindowBuilder::new(
        &app_handle,
        "debug",
        tauri::WebviewUrl::App("index.html?window=debug".into()),
    )
    .title("DesktopPet · Debug")
    .inner_size(360.0, 720.0)
    .min_inner_size(300.0, 400.0)
    .resizable(true)
    .build()
    {
        log::warn!("[debug-window] failed to build: {}", e);
    }
}

/// Quit the whole application. Used by the right-click menu item.
/// `app.exit(0)` terminates the process deterministically.
#[tauri::command]
pub fn quit_app(app_handle: tauri::AppHandle) {
    log::info!("quit_app invoked, exiting process");
    app_handle.exit(0);
}

/// Hide the main window to the system tray. Invoked by the "暂时离开"
/// context-menu item. The process stays alive; clicking the tray icon
/// restores the window (see the tray setup in lib.rs + `restore-from-tray`).
#[tauri::command]
pub async fn hide_to_tray(app_handle: tauri::AppHandle) -> Result<(), String> {
    log::info!("hide_to_tray: hiding main window to system tray");
    if let Some(win) = app_handle.get_webview_window("main") {
        win.hide().map_err(|e| format!("Failed to hide window: {}", e))?;
    } else {
        log::warn!("hide_to_tray: main window not found");
    }
    Ok(())
}
