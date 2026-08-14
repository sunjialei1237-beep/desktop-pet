//! Rituals (仪式感): recurring scheduled greetings that anchor the relationship.
//!
//! Design §7.2 — "最容易被忽略, 也是最能产生感情的". First iteration: 早安
//! (good-morning), the first time the pet sees the user each day. Date-driven
//! (today's first meeting), at most once per day, persisted in app_config.
//!
//! Principles:
//! - #6: disableable via config `[scheduler] enable_rituals`.
//! - #8: one LLM call per ritual firing (mirrors generate_welcome_back).
//! - #11: every firing is recorded via scheduler::record for the Debug Panel.
//!
//! Coordination with welcome-back: welcome-back is *duration-driven* (user
//! returns after >5min away); 早安 is *date-driven* (first meeting today).
//! On an overnight return both qualify — 早安 wins (more specific, a new-day
//! ritual). check_presence_transition suppresses welcome-back when today's
//! 早安 has already fired (or is about to fire this tick).

use crate::db::DbState;
use crate::db::onboarding;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::planner::Intent;
use crate::perception::time::TimeOfDay;
use chrono::Timelike;

/// app_config key holding the last 早安 date (YYYY-MM-DD, local). Absent ⇒
/// never fired ⇒ due.
const LAST_GOODMORNING_KEY: &str = "last_goodmorning_date";

/// Today's local date as the canonical key value (YYYY-MM-DD). Local, not UTC:
/// a ritual is about the user's day, not the server's.
fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Whether 早安 should fire: true when no 早安 has been recorded for today's
/// local date. Pure over the DB read — unit-testable by seeding app_config.
pub fn should_run_goodmorning(conn: &rusqlite::Connection) -> bool {
    match onboarding::get(conn, LAST_GOODMORNING_KEY) {
        Ok(Some(last)) => last != today_local(),
        // Read error or never fired ⇒ due (firing is idempotent + best-effort).
        _ => true,
    }
}

/// Record that today's 早安 has fired. Idempotent: re-writing today's date is a
/// no-op. Call BEFORE emitting the bubble so a crash between mark and emit
/// can't cause a duplicate (worst case: skipped today, fires tomorrow).
pub fn mark_goodmorning_done(conn: &rusqlite::Connection) -> Result<(), String> {
    onboarding::save(conn, LAST_GOODMORNING_KEY, &today_local())
}

/// Canned 早安 fallback when the LLM is unconfigured or returns empty.
/// Mood-scaled + time-of-day-aware, mirroring `welcome_back_canned`.
pub fn goodmorning_canned(mood: f64, tod: TimeOfDay) -> &'static str {
    let playful = mood >= 0.65;
    match tod {
        TimeOfDay::Morning if playful => "早呀！新的一天～",
        TimeOfDay::Morning => "早，醒了吗。",
        // Afternoon first-meeting: a gentle tease about sleeping in.
        TimeOfDay::Afternoon if playful => "都中午啦才来，昨晚熬夜了吧？",
        TimeOfDay::Afternoon => "中午好，刚起呀。",
        // Outside Morning/Afternoon the loop never fires 早安, but keep a sane
        // default so the canned fn is total (defensive, mirrors react::*).
        _ => "你来啦。",
    }
}

/// Generates a 早安 bubble grounded in retrieved memory. Mirrors
/// `generate_welcome_back`: retrieve → optional anchor → Intent → budget →
/// one LLM call → grounding_guard. The prompt is time-of-day-aware (Morning =
/// fresh wake-up greeting; Afternoon = gentle tease about sleeping in).
///
/// Returns None only when grounding_guard suppresses an invented reply (the
/// user never sees a hallucinated 早安). Caller falls back to canned on None
/// when the LLM is unconfigured (the command handles that path).
pub async fn generate_goodmorning(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&crate::embedding::EmbeddingService>,
    wm_context: &[ChatMessage],
) -> Result<Option<crate::pending::proactive::BubbleOutcome>, String> {
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let retrieval = crate::mind::retrieval::retrieve(
        "user's life recent events preferences",
        &emotion,
        embedding,
        db,
        8,
    )?;
    // Genuine recall reinforces the memory used to greet (same as welcome_back).
    crate::mind::retrieval::reinforce_top(db, &retrieval.episodes);

    let (memory_anchor, has_anchor): (String, bool) = {
        let mut rng = rand::thread_rng();
        if let Some(f) = crate::pending::proactive::sample_anchorable_fact(&retrieval.facts, &mut rng) {
            (format!("{}: {}", f.key, f.value), true)
        } else if let Some(i) = crate::mind::retrieval::sample_surface_anchor(
            &retrieval.episodes,
            &chrono::Utc::now(),
            &mut rng,
        ) {
            (retrieval.episodes[i].episode.summary.clone(), true)
        } else {
            (String::new(), false)
        }
    };

    let tone: &str = if emotion.mood >= 0.65 { "playful" } else { "gentle" };
    let intent = Intent {
        goal: "welcome".to_string(),
        memory_anchor: memory_anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "goodmorning".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };
    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);

    // Time-of-day-aware framing. Local hour (the ritual only fires in
    // Morning/Afternoon, but compute defensively).
    let hour = chrono::Local::now().hour();
    let time_clause = if hour < 11 {
        "现在是上午，你是今天第一次注意到对方上线。她可能刚醒、刚开工——自然地说声早，像真的在迎接新的一天。".to_string()
    } else {
        "都中午了你才看到对方上线。她可能起晚了、或者忙了一上午才来——半带嗔怪半带关心地说一句（不是责备，是那种「你可算来了」的语气）。".to_string()
    };
    let anchor_clause = if has_anchor {
        format!("你想起 ta 之前跟你提过的事：{memory_anchor}。可以顺便轻轻带一句，但只能围绕这件事的原意，别换话题、别编没提过的细节。")
    } else {
        String::new()
    };
    messages.push(ChatMessage::user(format!(
        "（{time_clause}{anchor_clause}简短自然，1-2 句早安招呼。称呼对方用「你」，不要用「用户」。按规则回复。）"
    )));

    log::info!(
        "[goodmorning] tod_hour={} has_anchor={} tone={} facts={} episodes={} msgs={}",
        hour,
        has_anchor,
        tone,
        retrieval.facts.len(),
        retrieval.episodes.len(),
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "goodmorning", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = crate::pending::proactive::grounding_guard(reply, &retrieval, &messages, llm).await;
    match reply {
        Some(reply) => Ok(Some(crate::pending::proactive::BubbleOutcome {
            reply,
            anchor: memory_anchor,
        })),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use chrono::Timelike;

    #[test]
    fn should_run_when_never_fired() {
        // No last_goodmorning_date row ⇒ due.
        let db = test_db();
        db.with_conn(|conn| {
            assert!(should_run_goodmorning(conn));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn should_not_run_when_already_fired_today() {
        let db = test_db();
        db.with_conn(|conn| {
            mark_goodmorning_done(conn)?;
            Ok::<_, String>(())
        })
        .unwrap();
        db.with_conn(|conn| {
            assert!(!should_run_goodmorning(conn));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn should_run_when_last_fired_yesterday() {
        let db = test_db();
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        db.with_conn(|conn| onboarding::save(conn, LAST_GOODMORNING_KEY, &yesterday))
            .unwrap();
        db.with_conn(|conn| {
            assert!(should_run_goodmorning(conn));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn canned_is_mood_and_time_aware() {
        // High mood ⇒ playful line; low mood ⇒ quieter. Morning vs Afternoon
        // differ. Just assert they're non-empty and distinct where expected.
        let am_playful = goodmorning_canned(0.8, TimeOfDay::Morning);
        let am_quiet = goodmorning_canned(0.3, TimeOfDay::Morning);
        let pm_playful = goodmorning_canned(0.8, TimeOfDay::Afternoon);
        assert!(!am_playful.is_empty());
        assert!(!am_quiet.is_empty());
        assert_ne!(am_playful, am_quiet, "mood should change the line");
        assert_ne!(am_playful, pm_playful, "time of day should change the line");
        assert!(pm_playful.contains("中午"), "afternoon line teases sleeping in");
    }

    /// Sanity: today_local is the canonical YYYY-MM-DD the gate compares against.
    #[test]
    fn today_local_is_calendar_date() {
        let t = today_local();
        assert_eq!(t.len(), 10, "YYYY-MM-DD is 10 chars");
        assert_eq!(t.chars().nth(4), Some('-'));
    }

    // keep Timelike import used in case future tests need hour arithmetic
    #[test]
    fn _timelike_in_scope() {
        let _ = chrono::Local::now().hour();
    }
}
