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
        // Cooldown state for the loneliness-driven nudge (thread-local, like
        // `away_since`). Resets on app start — acceptable: she simply skips the
        // first cooldown window after launch.
        let mut last_lonely_nudge: Option<std::time::Instant> = None;
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            medium_tick(&app);
            // Rituals (早安) run BEFORE presence-transition so that an overnight
            // return fires 早安 (date-driven) first; the welcome-back path then
            // sees today's 早安 already done and yields to it.
            check_goodmorning(&app);
            // 晚安 runs after 早安 (the windows never overlap: Morning/Afternoon
            // vs 21:00+). Once 晚安 fires, the frontend "该睡了" nudge also
            // yields for the rest of the day via ritual_done_today.
            check_goodnight(&app);
            check_presence_transition(&app, &mut last_presence, &mut away_since);
            check_lonely_nudge(&app, &mut last_lonely_nudge);
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

/// The global proactive-bubble budget gate (2026-08-14): atomic check-and-occupy
/// of the shared, persisted interval. Every proactive EMIT point (pending
/// follow-up / welcome-back / lonely nudge) must pass this before emitting, so
/// bubbles can't stack across paths (previously each path gated only on its own
/// conditions → several bubbles within minutes). Returns false when the budget
/// is not available — the caller stays silent (Architecture #12).
fn bubble_budget_ok(app: &AppHandle) -> bool {
    let Some(db) = get_db(app) else {
        return false;
    };
    let min = app
        .try_state::<AppState>()
        .map(|s| s.config.proactive.min_interval_secs)
        .unwrap_or(3600);
    crate::pending::budget::try_occupy_budget(&db, min, chrono::Utc::now())
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
        .unwrap_or_else(|e| {
            crate::lifecycle::scheduler::record(
                "homeostasis",
                true,
                "error",
                Some(e.clone()),
            );
            0.0
        });

    crate::lifecycle::scheduler::record("homeostasis", true, "ok", None);

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
            // Global bubble budget: a due reminder still has to wait for the
            // interval if another bubble just fired (it stays pending — get_due
            // keeps returning it, so it fires at the next window, no loss).
            if bubble_budget_ok(app) {
                if let Some(first) = events.first() {
                    let _ = app.emit(
                        "proactive-prompt",
                        serde_json::json!({
                            "title": &first.title,
                            "event_id": &first.id,
                        }),
                    );
                }
                crate::lifecycle::scheduler::record(
                    "pending_check",
                    true,
                    "ok",
                    Some(format!("{} due", events.len())),
                );
            } else {
                crate::lifecycle::scheduler::record(
                    "pending_check",
                    true,
                    "ok",
                    Some(format!("{} due, budget held", events.len())),
                );
            }
        }
        Ok(_) => {
            crate::lifecycle::scheduler::record("pending_check", true, "ok", None);
        }
        Err(e) => {
            log::warn!("Pending event check failed: {}", e);
            crate::lifecycle::scheduler::record(
                "pending_check",
                true,
                "error",
                Some(e.to_string()),
            );
        }
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
                    "rest_need": emo.rest_need,
                }),
            );
            crate::lifecycle::scheduler::record("emotion_push", true, "ok", None);
        }
        Err(e) => {
            log::warn!("Emotion push failed: {}", e);
            crate::lifecycle::scheduler::record(
                "emotion_push",
                true,
                "error",
                Some(e.to_string()),
            );
        }
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
            // Yield to 早安 when both qualify (overnight return during
            // Morning/Afternoon): if today's 早安 has already fired (or is
            // about to, having just run in check_goodmorning above), the
            // ritual greeting is more fitting than a generic welcome-back.
            // should_run_goodmorning == false ⇒ already done today.
            let tod = crate::perception::time::current_time_of_day();
            let morning_window = matches!(
                tod,
                crate::perception::time::TimeOfDay::Morning
                    | crate::perception::time::TimeOfDay::Afternoon
            );
            let goodmorning_already_done = get_db(app)
                .and_then(|db| {
                    db.with_conn(|conn| Ok(!crate::soul::ritual::should_run_goodmorning(conn)))
                        .ok()
                })
                .unwrap_or(false);
            if morning_window && goodmorning_already_done {
                log::info!(
                    "Life loop: user returned after {}s, but 早安 already fired today — yielding to ritual",
                    away_secs
                );
            } else if !bubble_budget_ok(app) {
                // Global bubble budget: another bubble fired within the interval
                // (e.g. a pending reminder moments ago) — stay silent; the
                // return is still acknowledged by presence itself (Arch #12).
                log::info!(
                    "Life loop: user returned after {}s, but bubble budget held — skipping welcome-back",
                    away_secs
                );
            } else {
                log::info!(
                    "Life loop: user returned after {}s away — emitting welcome-back",
                    away_secs
                );
                let _ = app.emit(
                    "welcome-back",
                    serde_json::json!({ "away_secs": away_secs }),
                );
            }
        }
        // Reset: the return has been acted on (or skipped). Next away starts fresh.
        *away_since = None;
    }

    *last_presence = now_presence;
    crate::lifecycle::scheduler::record("presence_watch", true, "ok", None);
}

/// Loneliness above which she may proactively reach out (matches planner Rule 4).
const LONELY_NUDGE_THRESHOLD: f64 = 0.6;
/// Closeness required before a lonely nudge (matches planner Rule 4: an early
/// relationship earns no unprompted reach-out, respecting Liri's non-clingy
/// nature — she doesn't pine for strangers).
const LONELY_NUDGE_CLOSENESS: f64 = 20.0;
/// Min seconds between lonely nudges — keeps it a rare surprise, not spam.
const LONELY_NUDGE_COOLDOWN_SECS: u64 = 30 * 60;

/// Loneliness-driven proactive nudge. When homeostasis has let loneliness climb
/// (the user has been idle, not talking) AND the relationship is established
/// (closeness >= 20, mirroring planner Rule 4) AND the user is actually at the
/// desk (presence Active) but not mid-conversation, she occasionally reaches
/// out — a gentle "想你了" bubble. Cooldown-bounded so it stays a rare
/// surprise, not spam (Architecture #10 liveliness, #8 cost, #6 graceful:
/// failure logged, never fatal). Mirror of `check_presence_transition`.
fn check_lonely_nudge(app: &AppHandle, last_nudge: &mut Option<std::time::Instant>) {
    use crate::perception::presence::{current_presence, PresenceState};

    // Only when the user is actually present — nudging an empty desk is wasted.
    if current_presence() != PresenceState::Active {
        return;
    }

    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };

    // Loneliness + closeness gating (mirror planner Rule 4 thresholds).
    let (loneliness, closeness) = db
        .with_conn(|conn| {
            let emo = crate::db::emotion::get(conn)?;
            let closeness = crate::db::relationship::get(conn)
                .map(|r| r.closeness)
                .unwrap_or(0.0);
            Ok::<_, String>((emo.loneliness, closeness))
        })
        .unwrap_or((0.0, 0.0));

    if loneliness <= LONELY_NUDGE_THRESHOLD || closeness < LONELY_NUDGE_CLOSENESS {
        return;
    }

    // Don't nudge mid-conversation.
    let just_talked = recent_interaction_secs(app)
        .map(|s| s < 120)
        .unwrap_or(false);
    if just_talked {
        return;
    }

    // Cooldown: a lonely nudge is a rare surprise, not every 30s tick.
    let now = std::time::Instant::now();
    if let Some(last) = *last_nudge {
        if now.duration_since(last).as_secs() < LONELY_NUDGE_COOLDOWN_SECS {
            return;
        }
    }

    // Global bubble budget (2026-08-14): the lonely nudge's own cooldown is
    // subsumed by the shared interval — if any bubble fired recently (welcome-
    // back / reminder / lively), she stays quiet instead of stacking on top.
    if !bubble_budget_ok(app) {
        log::info!(
            "Life loop: loneliness={:.2} but bubble budget held — skipping lonely-nudge",
            loneliness
        );
        return;
    }

    log::info!(
        "Life loop: loneliness={:.2} closeness={:.0} — emitting lonely-nudge",
        loneliness, closeness
    );
    let _ = app.emit("lonely-nudge", serde_json::json!({ "loneliness": loneliness }));
    crate::lifecycle::scheduler::record(
        "lonely_nudge",
        true,
        "ok",
        Some(format!("loneliness={:.2}", loneliness)),
    );
    *last_nudge = Some(now);
}

/// 早安 ritual: the first meeting each day (Morning/Afternoon, user present).
/// Date-driven (at most once per local day), persisted in app_config. Fires a
/// "ritual-bubble" event the frontend turns into a greeting; the LLM generation
/// happens on-demand in the `ritual_bubble` command (mirrors welcome-back).
fn check_goodmorning(app: &AppHandle) {
    use crate::perception::time::{current_time_of_day, TimeOfDay};

    // Capability toggle (Architecture #6).
    let enabled = app
        .try_state::<AppState>()
        .map(|s| s.config.scheduler.enable_rituals)
        .unwrap_or(true);
    if !crate::lifecycle::scheduler::should_run(enabled) {
        crate::lifecycle::scheduler::record("ritual_goodmorning", false, "skipped", None);
        return;
    }

    // Only during the daytime greeting window.
    let tod = current_time_of_day();
    if !matches!(tod, TimeOfDay::Morning | TimeOfDay::Afternoon) {
        return;
    }

    // Only when the user is actually at the desk.
    if crate::perception::presence::current_presence()
        != crate::perception::presence::PresenceState::Active
    {
        return;
    }

    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };
    let due = db
        .with_conn(|conn| Ok(crate::soul::ritual::should_run_goodmorning(conn)))
        .unwrap_or(false);
    if !due {
        return;
    }

    // Mark done BEFORE emitting so a crash or rapid re-tick can't double-fire.
    // (Idempotent: re-writing today is a no-op.)
    let _ = db.with_conn(|conn| crate::soul::ritual::mark_goodmorning_done(conn));

    // 早安 is a date-driven ritual — it fires regardless of the interval gate
    // (the user's first meeting of the day is the most expected bubble), but it
    // OCCUPIES the shared budget so no other bubble follows within the interval
    // (proactive bubble governance 2026-08-14).
    crate::pending::budget::occupy_budget_always(&db);

    log::info!("Life loop: 早安 ritual firing (tod={})", tod);
    let _ = app.emit(
        "ritual-bubble",
        serde_json::json!({ "kind": "goodmorning" }),
    );
    crate::lifecycle::scheduler::record(
        "ritual_goodmorning",
        true,
        "ok",
        Some(format!("tod={}", tod)),
    );
}

/// 晚安 ritual: the day's closing (21:00-23:59, user present, once per local
/// day). Mirrors check_goodmorning: date-driven, fires regardless of the
/// interval gate but OCCUPIES the shared budget; once fired, the frontend
/// "该睡了" nudge yields for the rest of the day (one bedtime voice per day).
fn check_goodnight(app: &AppHandle) {
    use chrono::Timelike;

    // Capability toggle — same switch as all rituals (Architecture #6).
    let enabled = app
        .try_state::<AppState>()
        .map(|s| s.config.scheduler.enable_rituals)
        .unwrap_or(true);
    if !crate::lifecycle::scheduler::should_run(enabled) {
        crate::lifecycle::scheduler::record("ritual_goodnight", false, "skipped", None);
        return;
    }

    // Only in the bedtime window (21:00-23:59 local).
    if !(21..=23).contains(&chrono::Local::now().hour()) {
        return;
    }

    // Only when the user is actually at the desk (a goodnight to an empty
    // room is wasted).
    if crate::perception::presence::current_presence()
        != crate::perception::presence::PresenceState::Active
    {
        return;
    }

    let db = match get_db(app) {
        Some(s) => s,
        None => return,
    };
    let due = db
        .with_conn(|conn| Ok(crate::soul::ritual::should_run_goodnight(conn)))
        .unwrap_or(false);
    if !due {
        return;
    }

    // Mark done BEFORE emitting (same crash-safety contract as 早安).
    let _ = db.with_conn(|conn| crate::soul::ritual::mark_goodnight_done(conn));

    // Date-driven ritual: exempt from the interval gate, but occupies the
    // shared budget so nothing else bubbles right after the goodnight.
    crate::pending::budget::occupy_budget_always(&db);

    let hour = chrono::Local::now().hour();
    log::info!("Life loop: 晚安 ritual firing (hour={})", hour);
    let _ = app.emit(
        "ritual-bubble",
        serde_json::json!({ "kind": "goodnight" }),
    );
    crate::lifecycle::scheduler::record(
        "ritual_goodnight",
        true,
        "ok",
        Some(format!("hour={}", hour)),
    );
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

    // Scheduler capability toggles (Architecture #6). Defaults all-on.
    let sched_cfg = app
        .try_state::<AppState>()
        .map(|s| s.config.scheduler.clone())
        .unwrap_or_default();

    // 1. Episode memory decay.
    match db.with_conn(crate::db::episodes::decay_strength) {
        Ok(count) => {
            log::info!("Life loop: decayed {} episodes", count);
            crate::lifecycle::scheduler::record(
                "memory_decay",
                true,
                "ok",
                Some(format!("{} episodes", count)),
            );
        }
        Err(e) => {
            log::warn!("Memory decay failed: {}", e);
            crate::lifecycle::scheduler::record(
                "memory_decay",
                true,
                "error",
                Some(e.to_string()),
            );
        }
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
            crate::lifecycle::scheduler::record("closeness_drift", true, "ok", None);
        }
        Err(e) => {
            log::warn!("Relationship check failed: {}", e);
            crate::lifecycle::scheduler::record(
                "closeness_drift",
                true,
                "error",
                Some(e.to_string()),
            );
        }
    }

    // 3. Lifecycle cleanup: remove old low-importance episodes (capability #6).
    if crate::lifecycle::scheduler::should_run(sched_cfg.enable_lifecycle_cleanup) {
        match crate::soul::consolidation::lifecycle_cleanup(&db) {
            Ok(count) if count > 0 => {
                log::info!("Life loop: cleaned up {} old episodes", count);
                crate::lifecycle::scheduler::record(
                    "lifecycle_cleanup",
                    true,
                    "ok",
                    Some(format!("{} removed", count)),
                );
            }
            Ok(_) => {
                crate::lifecycle::scheduler::record("lifecycle_cleanup", true, "ok", None);
            }
            Err(e) => {
                log::warn!("Lifecycle cleanup failed: {}", e);
                crate::lifecycle::scheduler::record(
                    "lifecycle_cleanup",
                    true,
                    "error",
                    Some(e.to_string()),
                );
            }
        }
    } else {
        crate::lifecycle::scheduler::record("lifecycle_cleanup", false, "skipped", None);
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
            // Reflection (capability #6: toggleable).
            if crate::lifecycle::scheduler::should_run(sched_cfg.enable_reflection) {
                match crate::soul::reflection::maybe_run_if_due(&db, &llm).await {
                    Ok(true) => {
                        log::info!("Life loop: reflection ran (daily)");
                        crate::lifecycle::scheduler::record("reflection", true, "ok", None);
                    }
                    Ok(false) => {
                        crate::lifecycle::scheduler::record(
                            "reflection",
                            true,
                            "ok",
                            Some("not due".to_string()),
                        );
                    }
                    Err(e) => {
                        log::warn!("Life loop reflection failed: {}", e);
                        crate::lifecycle::scheduler::record(
                            "reflection",
                            true,
                            "error",
                            Some(e.to_string()),
                        );
                    }
                }
            } else {
                crate::lifecycle::scheduler::record("reflection", false, "skipped", None);
            }
            // Consolidation (capability #6). No-op below the 100-episode threshold
            // (internal guard in consolidate()), so calling it hourly is cheap.
            if crate::lifecycle::scheduler::should_run(sched_cfg.enable_consolidation) {
                match crate::soul::consolidation::consolidate(&db, &llm).await {
                    Ok(count) => {
                        crate::lifecycle::scheduler::record(
                            "consolidation",
                            true,
                            "ok",
                            if count > 0 {
                                Some(format!("{} episodes", count))
                            } else {
                                None
                            },
                        );
                    }
                    Err(e) => {
                        log::warn!("Life loop consolidation failed: {}", e);
                        crate::lifecycle::scheduler::record(
                            "consolidation",
                            true,
                            "error",
                            Some(e.to_string()),
                        );
                    }
                }
            } else {
                crate::lifecycle::scheduler::record("consolidation", false, "skipped", None);
            }
            // Relationship review: summarize where the relationship stands every
            // N new conversation episodes. Episode-gated (rare), so the extra
            // LLM call is acceptable (Architecture #8). Failure is logged, never
            // fatal (Principle #6). Capability #6: toggleable.
            if crate::lifecycle::scheduler::should_run(sched_cfg.enable_relationship_review) {
                match crate::soul::review::maybe_run_review_if_due(&db, &llm).await {
                    Ok(true) => {
                        log::info!("Life loop: relationship review ran");
                        crate::lifecycle::scheduler::record(
                            "relationship_review",
                            true,
                            "ok",
                            None,
                        );
                    }
                    Ok(false) => {
                        crate::lifecycle::scheduler::record(
                            "relationship_review",
                            true,
                            "ok",
                            Some("not due".to_string()),
                        );
                    }
                    Err(e) => {
                        log::warn!("Life loop relationship review failed: {}", e);
                        crate::lifecycle::scheduler::record(
                            "relationship_review",
                            true,
                            "error",
                            Some(e.to_string()),
                        );
                    }
                }
            } else {
                crate::lifecycle::scheduler::record(
                    "relationship_review",
                    false,
                    "skipped",
                    None,
                );
            }
        });
    }
}
