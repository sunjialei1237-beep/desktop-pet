//! Weekly summary ritual (周日总结): once per week, on Sunday evening, she
//! casually recaps the week you shared — like a friend, not a status report.
//!
//! Design §7.2 Rituals — "每周日: 本周总结"; "仪式感 = 规律性重复 + 情感锚点".
//!
//! The week's episodes/facts are assembled into a `RetrievalResult`, which does
//! double duty: budget::allocate_and_compress injects them as the [Memories]
//! section, and grounding_guard treats them as the allowed-reference pool (a
//! weekly recap necessarily talks about the user's week, so the pool must be
//! the week itself, not a semantic top-k).
//!
//! Principles:
//! - #6: disableable via the shared `[scheduler] enable_rituals` switch.
//! - #8: one LLM call per week.
//! - #12: an empty week stays silent (the caller marks done without emitting).

use crate::db::DbState;
use crate::db::onboarding;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::planner::Intent;
use chrono::{DateTime, Datelike, Local};

/// app_config key holding the last summarized week's Monday (YYYY-MM-DD).
const LAST_WEEKLY_KEY: &str = "last_weekly_summary_week";

/// This ISO week's Monday as YYYY-MM-DD — the idempotency key (at most one
/// summary per week-key). On Sunday, `num_days_from_monday() == 6`, so the
/// subtraction lands exactly on this week's Monday.
pub fn week_monday_key(now: &DateTime<Local>) -> String {
    let monday =
        now.date_naive() - chrono::Duration::days(now.weekday().num_days_from_monday() as i64);
    monday.format("%Y-%m-%d").to_string()
}

/// Whether the weekly summary is due: Sunday only, and this week's key not yet
/// recorded. `now` is a parameter so the gate is unit-testable (the caller
/// additionally enforces the Evening window and presence).
pub fn should_run_weekly(conn: &rusqlite::Connection, now: &DateTime<Local>) -> bool {
    if now.weekday() != chrono::Weekday::Sun {
        return false;
    }
    let key = week_monday_key(now);
    match onboarding::get(conn, LAST_WEEKLY_KEY) {
        Ok(Some(last)) => last != key,
        _ => true,
    }
}

/// Record this week's summary as done (BEFORE emitting, same crash-safety
/// contract as the 早安/晚安 marks).
pub fn mark_weekly_done(conn: &rusqlite::Connection, now: &DateTime<Local>) -> Result<(), String> {
    onboarding::save(conn, LAST_WEEKLY_KEY, &week_monday_key(now))
}

/// Whether the last 7 days produced anything worth recapping. Pure DB read;
/// the caller uses it to stay silent (and still mark done) on an empty week.
pub fn week_has_content(db: &DbState) -> bool {
    let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    db.with_conn(|conn| {
        let eps = crate::db::episodes::get_since(conn, &since, 1)?;
        if !eps.is_empty() {
            return Ok(true);
        }
        let facts = crate::db::facts::get_since(conn, &since, 1)?;
        Ok(!facts.is_empty())
    })
    .unwrap_or(false)
}

/// Canned weekly fallback when the LLM is unconfigured or returns empty.
pub fn weekly_canned() -> &'static str {
    "这周过得好快呀……周日了，好好休息一下。"
}

/// Generates the weekly recap bubble. The week's episodes (≤50) and new facts
/// (≤20) go in as the retrieval pool; the prompt frames it as a friend's
/// casual recap ("不是工作周报"), grounded by the same guard as every bubble.
pub async fn generate_weekly_summary(
    db: &DbState,
    llm: &LlmClient,
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

    let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let week_episodes = db.with_conn(|conn| crate::db::episodes::get_since(conn, &since, 50))?;
    let week_facts = db.with_conn(|conn| crate::db::facts::get_since(conn, &since, 20))?;

    // The week itself is the retrieval pool: budget injects it as [Memories],
    // grounding_guard accepts references to it (a recap must be allowed to
    // talk about the week's events).
    let retrieval = crate::mind::retrieval::RetrievalResult {
        episodes: week_episodes
            .into_iter()
            .map(|episode| crate::mind::retrieval::ScoredEpisode {
                episode,
                score: 0.0,
                score_breakdown: Default::default(),
            })
            .collect(),
        facts: week_facts,
        ..Default::default()
    };
    let ep_count = retrieval.episodes.len();
    let fact_count = retrieval.facts.len();

    let tone: &str = if emotion.mood >= 0.65 { "playful" } else { "gentle" };
    let intent = Intent {
        goal: "celebrate".to_string(),
        memory_anchor: String::new(),
        tone: tone.to_string(),
        proactive: true,
        action: "weekly_summary".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };
    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);

    messages.push(ChatMessage::user(format!(
        "（今天是周日。上面的记忆区就是这一周你们之间的事——{ep_count} 件小事、{fact_count} 条新记住的偏好。你想跟 ta 随口复盘一下这周：像朋友聊天时那样自然地提一两句最值得说的，可以带一点你自己的感受（开心/心疼/期待），**不是工作周报，不要罗列清单**，不用面面俱到。两件事之间没有自然的连接就只说一件，别硬凑在一起。只提上面真实有的内容，绝不编造没发生过的事；提到某件事时**用它的原意原词说清楚**——它是什么就是什么，绝不能把一件事换成另一件事或换掉关键的名字（比如把「去看流浪狗」说成「去看流星雨」就是失败的复盘）。时间也按记忆里带的日期说，别自己估。最多一个问句，1-3 句。称呼对方用「你」。按规则回复。）"
    )));

    log::info!(
        "[weekly] episodes={} facts={} tone={} msgs={}",
        ep_count,
        fact_count,
        tone,
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "weekly_summary", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = crate::pending::proactive::grounding_guard(reply, &retrieval, &messages, llm).await;
    if let Some(r) = &reply {
        // Log the recap like every other bubble so the next window's selector
        // knows she just talked about these memories (incident 2026-08-16:
        // recap mentioned 糯米, log stayed empty, downstream blind).
        crate::pending::proactive::log_bubble(
            db,
            "weekly_summary",
            r,
            &format!("本周 {ep_count} 件事"),
            Some("周日的每周小结"),
        );
    }
    match reply {
        Some(reply) => Ok(Some(crate::pending::proactive::BubbleOutcome {
            reply,
            anchor: format!("本周 {} 件事", ep_count),
            anchor_reason: Some("周日的每周小结".to_string()),
        })),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    fn sunday_evening() -> DateTime<Local> {
        // 2026-08-16 is a Sunday (2026-08-10 was the Monday).
        use chrono::TimeZone;
        Local.with_ymd_and_hms(2026, 8, 16, 19, 0, 0).unwrap()
    }

    fn saturday() -> DateTime<Local> {
        use chrono::TimeZone;
        Local.with_ymd_and_hms(2026, 8, 15, 19, 0, 0).unwrap()
    }

    #[test]
    fn week_key_is_iso_monday() {
        let k = week_monday_key(&sunday_evening());
        assert_eq!(k, "2026-08-10", "Sunday 2026-08-16 maps to Monday 08-10");
    }

    #[test]
    fn weekly_not_due_on_non_sunday() {
        let db = test_db();
        db.with_conn(|conn| {
            assert!(!should_run_weekly(conn, &saturday()), "Saturday is not recap day");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn weekly_due_on_sunday_then_done() {
        let db = test_db();
        let now = sunday_evening();
        db.with_conn(|conn| {
            assert!(should_run_weekly(conn, &now), "Sunday, never fired => due");
            mark_weekly_done(conn, &now)?;
            assert!(!should_run_weekly(conn, &now), "already summarized this week");
            Ok::<_, String>(())
        })
        .unwrap();
    }

    #[test]
    fn weekly_due_again_next_week() {
        let db = test_db();
        use chrono::TimeZone;
        let this_week = sunday_evening();
        let next_week = Local.with_ymd_and_hms(2026, 8, 23, 19, 0, 0).unwrap();
        db.with_conn(|conn| {
            mark_weekly_done(conn, &this_week)?;
            assert!(should_run_weekly(conn, &next_week), "next Sunday is a new week");
            Ok::<_, String>(())
        })
        .unwrap();
    }

    #[test]
    fn empty_week_has_no_content() {
        let db = test_db();
        assert!(!week_has_content(&db), "clean DB => silent week");
    }

    #[test]
    fn week_with_episode_has_content() {
        let db = test_db();
        db.with_conn(|conn| {
            crate::db::episodes::insert(conn, &crate::db::episodes::Episode {
                id: "ep_w1".into(),
                time: chrono::Utc::now().to_rfc3339(),
                summary: "和糯米去看猫".into(),
                emotion: Some("开心".into()),
                importance: 0.6,
                is_landmark: false,
                subject: "user".into(),
                participants: None,
                topics: None,
                source_type: "conversation".into(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.6,
                recall_count: 0,
                last_recalled_at: None,
                consolidated: false,
                created_at: chrono::Utc::now().to_rfc3339(),
                emotion_anchor: None,
            })
        })
        .unwrap();
        assert!(week_has_content(&db));
    }

    #[test]
    fn canned_non_empty() {
        assert!(!weekly_canned().is_empty());
    }
}
