//! Life Loop background timers.
//! Started after app initialization; runs on background threads.

use crate::db::DbState;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Starts the life loop on background threads.
/// Medium (30s): emotion homeostasis + pending event check + emotion push
/// Slow (1h): memory decay + relationship decay
pub fn start_life_loop(app: AppHandle) {
    // Medium loop: every 30 seconds.
    {
        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            medium_tick(&app);
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

/// Gets the DbState from the managed state. Returns None if not available.
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

    // 1. Emotion homeostasis: drift values toward baselines.
    if let Err(e) = db.with_conn(|conn| crate::db::emotion::apply_homeostasis(conn, 0.01, &now)) {
        log::warn!("Homeostasis tick failed: {}", e);
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
    match db.with_conn(|conn| crate::db::emotion::get(conn)) {
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

/// Slow tick: memory decay, relationship drift.
fn slow_tick(app: &AppHandle) {
    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Episode memory decay.
    match db.with_conn(|conn| crate::db::episodes::decay_strength(conn)) {
        Ok(count) => log::info!("Life loop: decayed {} episodes", count),
        Err(e) => log::warn!("Memory decay failed: {}", e),
    }

    // 2. Relationship closeness drift (after 24h of no interaction).
    match db.with_conn(|conn| crate::db::relationship::get(conn)) {
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
}
