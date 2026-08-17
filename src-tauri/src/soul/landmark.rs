//! Relationship landmarks (关系里程碑): one-off celebrations of how far
//! you've come together — 认识 7/30/100/365 天, the 100th/1000th conversation.
//!
//! Design §7.5 — "第一次叫她名字、第一次一起过生日"; §7.2 纪念仪式 —
//! "认识 100 天: 「今天是我们认识第100天哦!」".
//!
//! `relationship.days_known` is a dead column (nothing ever writes it), so the
//! anchor is `first_met_date` in app_config, backfilled ONCE from the earliest
//! change_log row (lightweight event sourcing: the system's first memory write
//! ≈ the day you met). Celebrated milestones are remembered in app_config so
//! each fires at most once ever (idempotent per milestone id).
//!
//! Principles: #6 disableable via `[scheduler] enable_landmarks`; #8 one LLM
//! call per milestone (a handful per year); #11 every firing recorded via
//! scheduler::record.

use crate::db::DbState;
use crate::db::onboarding;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::planner::Intent;

const FIRST_MET_KEY: &str = "first_met_date";
const CELEBRATED_KEY: &str = "celebrated_landmarks";

/// Days-known milestones worth celebrating, ascending.
const DAYS_MILESTONES: [i64; 4] = [7, 30, 100, 365];
/// Conversation-count milestones worth celebrating, ascending.
const CONV_MILESTONES: [i64; 2] = [100, 1000];

/// A milestone that is due: `id` ("days:30" / "conv:100") is the idempotency
/// key, `description` is the Chinese phrase the prompt voices.
#[derive(Debug, Clone, PartialEq)]
pub struct Landmark {
    pub id: String,
    pub description: String,
}

/// Resolves (and persists on first call) the day you met: the earliest
/// change_log timestamp, falling back to now for a fresh install (day 1 —
/// the first milestone then lands naturally 7 days later).
pub fn resolve_first_met(conn: &rusqlite::Connection) -> String {
    if let Ok(Some(cached)) = onboarding::get(conn, FIRST_MET_KEY) {
        return cached;
    }
    let earliest: Option<String> = conn
        .query_row("SELECT MIN(timestamp) FROM change_log", [], |r| r.get(0))
        .ok()
        .flatten();
    let first_met = earliest.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let _ = onboarding::save(conn, FIRST_MET_KEY, &first_met);
    first_met
}

/// Calendar days between `t` and now, counted in LOCAL dates — the way humans
/// count "认识多少天" (a July-16 evening first-met is a full day older on Aug 15
/// even though only 29.x UTC hours-periods elapsed). Shared by the landmark
/// due-check and retrieval's days_known backfill (was UTC `.num_days()`, which
/// undercounted by one for evening-first-met UTC+8 users — she confidently
/// answered "29天" when the calendar said 30).
pub fn calendar_days_since(t: chrono::DateTime<chrono::Utc>) -> i64 {
    use chrono::Datelike;
    let now_local = chrono::Local::now().date_naive();
    let then_local = t.with_timezone(&chrono::Local).date_naive();
    (now_local - then_local).num_days()
}

/// Read-only resolution of the first-met moment: cached app_config key, else
/// the earliest change_log write. Does NOT persist (unlike `resolve_first_met`),
/// so read paths (retrieval's days_known backfill) can derive the real
/// relationship age without a write side effect.
pub fn first_met_readonly(conn: &rusqlite::Connection) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = onboarding::get(conn, FIRST_MET_KEY)
        .ok()
        .flatten()
        .or_else(|| {
            conn.query_row("SELECT MIN(timestamp) FROM change_log", [], |r| r.get(0))
                .ok()
                .flatten()
        });
    raw.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc))
    })
}

fn celebrated_ids(conn: &rusqlite::Connection) -> Vec<String> {
    onboarding::get(conn, CELEBRATED_KEY)
        .ok()
        .flatten()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

/// Thresholds already covered for one axis ("days" / "conv"): having
/// celebrated a LARGER milestone covers the smaller ones beneath it, so a
/// late-discovered days:30 doesn't leave a stale days:7 pending forever.
fn covered_thresholds(celebrated: &[String], prefix: &str) -> Vec<i64> {
    celebrated
        .iter()
        .filter_map(|id| id.strip_prefix(prefix).and_then(|n| n.parse::<i64>().ok()))
        .collect()
}

/// The next milestone worth celebrating, if any. Days-known milestones take
/// precedence over conversation-count ones (at most one celebration at a
/// time; the other stays due for the next tick). `>=` semantics with cover:
/// celebrate the LARGEST milestone already reached but not yet covered — a
/// missed days:7 discovered on day 31 reads naturally as "认识满 30 天",
/// never as a stale "满 7 天", and never fires twice.
pub fn due_landmark(conn: &rusqlite::Connection) -> Option<Landmark> {
    let celebrated = celebrated_ids(conn);
    let covered_days = covered_thresholds(&celebrated, "days:");
    let covered_conv = covered_thresholds(&celebrated, "conv:");

    let first_met = resolve_first_met(conn);
    let days_known = chrono::DateTime::parse_from_rfc3339(&first_met)
        .map(|t| calendar_days_since(t.with_timezone(&chrono::Utc)))
        .unwrap_or(0);
    for m in DAYS_MILESTONES.iter().rev() {
        let m = *m;
        if days_known >= m && !covered_days.iter().any(|c| *c >= m) {
            return Some(Landmark {
                id: format!("days:{}", m),
                description: format!("你们认识满 {} 天", m),
            });
        }
    }

    let total_conversations = crate::db::relationship::get(conn)
        .map(|r| r.total_conversations)
        .unwrap_or(0);
    for m in CONV_MILESTONES.iter().rev() {
        let m = *m;
        if total_conversations >= m && !covered_conv.iter().any(|c| *c >= m) {
            return Some(Landmark {
                id: format!("conv:{}", m),
                description: format!("你们第 {} 次对话", m),
            });
        }
    }
    None
}

/// Record the milestone as celebrated (BEFORE emitting, same crash-safety
/// contract as the ritual marks).
pub fn mark_landmark_celebrated(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
    let mut list = celebrated_ids(conn);
    if !list.iter().any(|x| x == id) {
        list.push(id.to_string());
    }
    onboarding::save(conn, CELEBRATED_KEY, &list.join(","))
}

/// Canned landmark fallback when the LLM is unconfigured or returns empty.
/// Generic on purpose: the specific milestone description only exists on the
/// generate path (the check layer marks it before the command runs).
pub fn landmark_canned() -> &'static str {
    "今天是值得纪念的一天呢，我们又一起走过了一个小里程碑。"
}

/// Generates a milestone-celebration bubble. Anchored in your EARLIEST
/// memories ("还记得我们刚认识的时候…" — design: "毕业典礼上突然想起第一天
/// 认识"), merged into the semantic retrieval pool so the [Memories] section
/// and grounding_guard both see them.
pub async fn generate_landmark(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&crate::embedding::EmbeddingService>,
    wm_context: &[ChatMessage],
    landmark: &Landmark,
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

    let mut retrieval = crate::mind::retrieval::retrieve(
        "user's life recent events preferences",
        &emotion,
        embedding,
        db,
        5,
    )?;
    // Merge the earliest episodes into the pool: the celebration wants to
    // look back at the beginning, and grounding_guard must allow referencing
    // them. Dedup by id (an early memory may also be semantically retrieved).
    let earliest = db.with_conn(|conn| {
        let all = crate::db::episodes::get_all(conn)?;
        Ok(all.into_iter().take(3).collect::<Vec<_>>())
    })?;
    let existing: std::collections::HashSet<String> =
        retrieval.episodes.iter().map(|e| e.episode.id.clone()).collect();
    for ep in earliest {
        if existing.contains(&ep.id) {
            continue;
        }
        retrieval.episodes.push(crate::mind::retrieval::ScoredEpisode {
            episode: ep,
            score: 0.0,
            score_breakdown: Default::default(),
        });
    }

    let tone: &str = if emotion.mood >= 0.65 { "playful" } else { "gentle" };
    let intent = Intent {
        goal: "celebrate".to_string(),
        memory_anchor: String::new(),
        tone: tone.to_string(),
        proactive: true,
        action: "landmark".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };
    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);

    messages.push(ChatMessage::user(format!(
        "（今天对你们来说是个特别的日子——{}。你是真的开心，想跟 ta 说说这件事。上面记忆区里有你们刚认识时候的事——如果合适，可以自然地带一句「还记得那时候…」；只提上面真实有的内容，绝不编造。1-2 句，最多一个问句。称呼对方用「你」。按规则回复。）",
        landmark.description
    )));

    log::info!(
        "[landmark] {} tone={} pool={} msgs={}",
        landmark.id,
        tone,
        retrieval.episodes.len(),
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "landmark", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = crate::pending::proactive::grounding_guard(reply, &retrieval, &messages, llm).await;
    if let Some(r) = &reply {
        crate::pending::proactive::log_bubble(db, "landmark_celebration", r, &landmark.description, None);
    }
    match reply {
        Some(reply) => Ok(Some(crate::pending::proactive::BubbleOutcome {
            reply,
            anchor: landmark.description.clone(),
            anchor_reason: Some("今天是个值得纪念的日子".to_string()),
        })),
        None => Ok(None),
    }
}

#[cfg(test)]
mod calendar_days_tests {
    use super::calendar_days_since;
    use chrono::{Duration, Utc};

    #[test]
    fn counts_local_calendar_days_not_utc_periods() {
        // Deterministic through the 00:00-02:00 local window (a 26h lookback
        // can cross TWO date lines right after midnight): take LOCAL noon
        // yesterday — exactly one calendar day ago at any clock time.
        use chrono::{Datelike, Local, TimeZone};
        let noon_today = Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let noon = Local
            .from_local_datetime(&noon_today)
            .single()
            .expect("local noon is unambiguous");
        let yesterday_noon = noon - Duration::days(1);
        assert_eq!(
            calendar_days_since(yesterday_noon.with_timezone(&Utc)),
            1
        );
    }

    #[test]
    fn same_instant_is_zero() {
        assert_eq!(calendar_days_since(Utc::now()), 0);
    }
}

mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    #[test]
    fn first_met_falls_back_to_now_on_empty_log() {
        let db = test_db();
        db.with_conn(|conn| {
            let fm = resolve_first_met(conn);
            assert!(chrono::DateTime::parse_from_rfc3339(&fm).is_ok(), "RFC3339");
            // Cached after first resolve.
            assert_eq!(resolve_first_met(conn), fm);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn first_met_backfills_from_earliest_change_log() {
        let db = test_db();
        let early = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        db.with_conn(|conn| {
            // An older row first, then the earliest must win.
            crate::db::changelog::append(conn, "facts", "insert", None, None, None, None, None)?;
            conn.execute(
                "UPDATE change_log SET timestamp = ?1 WHERE id = (SELECT MIN(id) FROM change_log)",
                rusqlite::params![early],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
        .unwrap();
        db.with_conn(|conn| {
            assert_eq!(resolve_first_met(conn), early);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn days_landmark_due_then_celebrated_once() {
        let db = test_db();
        let thirty_days_ago = (chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339();
        db.with_conn(|conn| {
            onboarding::save(conn, FIRST_MET_KEY, &thirty_days_ago)?;
            Ok(())
        })
        .unwrap();
        db.with_conn(|conn| {
            let lm = due_landmark(conn).expect("30-day landmark is due");
            assert_eq!(lm.id, "days:30");
            assert!(lm.description.contains("30"));
            mark_landmark_celebrated(conn, &lm.id)?;
            assert_eq!(due_landmark(conn), None, "celebrated once => silent");
            Ok::<_, String>(())
        })
        .unwrap();
    }

    #[test]
    fn conversation_landmark_fires_at_threshold() {
        let db = test_db();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE relationship SET total_conversations = 100 WHERE id = 1",
                [],
            )
            .map_err(|e| e.to_string())?;
            let lm = due_landmark(conn).expect("100th conversation is due");
            assert_eq!(lm.id, "conv:100");
            mark_landmark_celebrated(conn, &lm.id)?;
            assert_eq!(due_landmark(conn), None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn fresh_install_no_landmark() {
        let db = test_db();
        db.with_conn(|conn| {
            assert_eq!(due_landmark(conn), None, "day 1, nothing to celebrate yet");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn canned_non_empty() {
        assert!(!landmark_canned().is_empty());
    }
}
