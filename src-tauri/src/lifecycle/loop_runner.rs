//! Life Loop background timers.
//! Started after app initialization; runs on background threads.

use crate::commands::AppState;
use crate::db::DbState;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Starts the life loop on background threads.
/// Medium (30s): emotion homeostasis + pending event check + emotion push
/// Slow (1h): memory decay + relationship drift + lifecycle cleanup
pub fn start_life_loop(app: AppHandle) {
    // Medium loop: every 30 seconds.
    {
        let app = app.clone();
        // Presence tracking is thread-local: no lock needed, no shared state.
        // Seeded as Active: in production the user just launched the app (a
        // click), so they are present. In dev the launcher process may start
        // while the user is already idle (idle>300s = LongAway) — seeding with
        // current_presence() there makes the first tick fire a bogus
        // LongAway->Active "return" with away_secs=0 (away_since never set).
        // Treating startup as Active means we only react to a real
        // Active -> away -> Active cycle.
        let mut last_presence = crate::perception::presence::PresenceState::Active;
        let mut away_since: Option<std::time::Instant> = None;
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            medium_tick(&app);
            check_presence_transition(&app, &mut last_presence, &mut away_since);
        });
    }

    // Slow loop: every hour.
    {
        let app = app;
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(3600));
            slow_tick(&app);
        });
    }
}

/// Gets the DbState from the managed state.
fn get_db(app: &AppHandle) -> Option<tauri::State<'_, DbState>> {
    app.try_state::<DbState>()
}

/// Medium tick: emotion homeostasis, pending event check, emotion push.
fn medium_tick(app: &AppHandle) {
    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Emotion homeostasis: time-aware drift toward baselines.
    // Returns elapsed seconds so we can detect suspend/resume.
    let elapsed = db
        .with_conn(|conn| crate::db::emotion::apply_homeostasis_time_aware(conn, &now))
        .unwrap_or(0.0);

    if elapsed > crate::db::emotion::SUSPEND_THRESHOLD_SECS {
        log::info!(
            "Life loop: suspend/resume detected ({:.0}s elapsed), catching up",
            elapsed
        );
        // Signal frontend that the pet woke up after a long absence.
        let _ = app.emit(
            "app-status",
            serde_json::json!({
                "status": "resumed",
                "elapsed_secs": elapsed,
            }),
        );
    }

    // 2. Pending event check.
    match db.with_conn(|conn| crate::db::pending::get_due(conn, &now)) {
        Ok(events) if !events.is_empty() => {
            log::info!("Life loop: {} pending events due", events.len());
            if let Some(first) = events.first() {
                let _ = app.emit(
                    "proactive-prompt",
                    serde_json::json!({
                        "title": &first.title,
                        "event_id": &first.id,
                    }),
                );
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("Pending event check failed: {}", e),
    }

    // 3. Push current emotion state to frontend.
    match db.with_conn(crate::db::emotion::get) {
        Ok(emo) => {
            let _ = app.emit(
                "emotion-update",
                serde_json::json!({
                    "mood": emo.mood,
                    "mood_label": emo.mood_label,
                    "physical_energy": emo.physical_energy,
                    "social_battery": emo.social_battery,
                    "stress": emo.stress,
                    "loneliness": emo.loneliness,
                }),
            );
        }
        Err(e) => log::warn!("Emotion push failed: {}", e),
    }
}

/// Checks for an actionable presence transition (LongAway -> Active = the user
/// came back after >5min away) and emits "welcome-back" when it happens.
///
/// State (`last_presence`, `away_since`) is thread-local and owned by the
/// caller — no lock, no shared mutation. `away_since` is set the moment the user
/// leaves (Active -> away) and consumed when they return, so `away_secs` reflects
/// the real absence length rather than the coarse idle bucket.
///
/// Guard: skips if the user interacted within the last 30s, so a return-greeting
/// never collides with an ongoing conversation (Architecture Principle 10).
fn check_presence_transition(
    app: &AppHandle,
    last_presence: &mut crate::perception::presence::PresenceState,
    away_since: &mut Option<std::time::Instant>,
) {
    use crate::perception::presence::{
        classify_transition, current_presence, PresenceState, Transition,
    };

    let now_presence = current_presence();

    // Mark the start of an away period (Active -> any away).
    if *last_presence == PresenceState::Active && now_presence != PresenceState::Active {
        *away_since = Some(std::time::Instant::now());
    }

    let away_secs = away_since.map(|t| t.elapsed().as_secs()).unwrap_or(0);

    if let Some(Transition::ReturnedBack { away_secs }) =
        classify_transition(*last_presence, now_presence, away_secs)
    {
        let just_talked = recent_interaction_secs(app).map(|s| s < 30).unwrap_or(false);
        if !just_talked {
            log::info!(
                "Life loop: user returned after {}s away — emitting welcome-back",
                away_secs
            );
            let _ = app.emit(
                "welcome-back",
                serde_json::json!({ "away_secs": away_secs }),
            );
        }
        // Reset: the return has been acted on (or skipped). Next away starts fresh.
        *away_since = None;
    }

    *last_presence = now_presence;
}

/// Seconds since the last recorded interaction, or None if unavailable.
fn recent_interaction_secs(app: &AppHandle) -> Option<i64> {
    let db = get_db(app)?;
    db.with_conn(|conn| {
        let rel = crate::db::relationship::get(conn)?;
        let s = rel
            .last_interaction_at
            .ok_or_else(|| "no last_interaction_at".to_string())?;
        let dt = chrono::DateTime::parse_from_rfc3339(&s)
            .map_err(|e| format!("parse last_interaction_at: {}", e))?;
        Ok((chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_seconds())
    })
    .ok()
}

/// Slow tick: memory decay, relationship drift, lifecycle cleanup.
fn slow_tick(app: &AppHandle) {
    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Episode memory decay.
    match db.with_conn(crate::db::episodes::decay_strength) {
        Ok(count) => log::info!("Life loop: decayed {} episodes", count),
        Err(e) => log::warn!("Memory decay failed: {}", e),
    }

    // 2. Relationship closeness drift (after 24h of no interaction).
    match db.with_conn(crate::db::relationship::get) {
        Ok(rel) => {
            if let Some(last) = &rel.last_interaction_at {
                if let Ok(last_dt) = chrono::DateTime::parse_from_rfc3339(last) {
                    let hours_since = (chrono::Utc::now() - last_dt.with_timezone(&chrono::Utc))
                        .num_hours();
                    if hours_since > 24 {
                        let _ = db.with_conn(|conn| {
                            crate::db::relationship::decay_closeness(conn, 0.99, &now)
                        });
                    }
                }
            }
        }
        Err(e) => log::warn!("Relationship check failed: {}", e),
    }

    // 3. Lifecycle cleanup: remove old low-importance episodes.
    match crate::soul::consolidation::lifecycle_cleanup(&db) {
        Ok(count) if count > 0 => log::info!("Life loop: cleaned up {} old episodes", count),
        Ok(_) => {}
        Err(e) => log::warn!("Lifecycle cleanup failed: {}", e),
    }

    // 4. Soul: reflection if due (>20h) + consolidation if threshold met.
    //    Both are async LLM calls; slow_tick runs on a std::thread, so we enter
    //    the runtime via block_on. The slow tick's cadence is 1h, so blocking
    //    here is acceptable (Architecture Principle 5: never blocks Body, which
    //    runs on its own medium loop). LLM unconfigured -> skip silently
    //    (Principle 6: degrade gracefully).
    let llm = app
        .try_state::<AppState>()
        .and_then(|s| s.llm.lock().ok().and_then(|g| g.clone()));
    if let Some(llm) = llm {
        let _ = tauri::async_runtime::block_on(async {
            match crate::soul::reflection::maybe_run_if_due(&db, &llm).await {
                Ok(true) => log::info!("Life loop: reflection ran (daily)"),
                Ok(false) => {}
                Err(e) => log::warn!("Life loop reflection failed: {}", e),
            }
            // Consolidation is a no-op below the 100-episode threshold
            // (internal guard in consolidate()), so calling it hourly is cheap.
            if let Err(e) = crate::soul::consolidation::consolidate(&db, &llm).await {
                log::warn!("Life loop consolidation failed: {}", e);
            }
        });
    }
}
