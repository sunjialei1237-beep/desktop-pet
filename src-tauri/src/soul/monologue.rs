//! Internal Monologue: surfaces internal thoughts at the right moment.
//!
//! Design doc 7.1: She thinks between conversations. These thoughts were
//! generated during Reflection (soul/reflection.rs), not during the live
//! conversation. The key: she REALLY thought of it last night, timestamp proves it.
//!
//! Surfacing conditions:
//! - next_interaction: surface when user comes back (default)
//! - emotion_match: surface when current emotion matches (future)
//! - time_based: surface at a specific time (future)

use crate::db::reflections::InternalThought;
use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};

/// Re-voices a stored internal thought AT THE MOMENT OF SPEAKING (thought-stream
/// plan P0). The 2026-08-27 incident ("今天晚上的你安静得过分") had the startup
/// path display the raw thought verbatim — generated hours earlier under a
/// time-framed template, never re-grounded. This function instead renders the
/// thought through the LLM with the CURRENT time injected, first-person /
/// statement-first, and drops it if the LLM is unavailable or grounding_guard
/// flags it (宁可不显示, Architecture #12).
///
/// Identity-only retrieval (mirror generate_lively): no episodic memories in
/// context, so grounding_guard blocks any invented claim about the user's past.
/// One main-model call, no streaming (Principle 8).
pub async fn voice_thought(
    db: &DbState,
    llm: &LlmClient,
    content: &str,
) -> Result<Option<String>, String> {
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let retrieval = crate::mind::retrieval::load_identity(db);
    let intent = crate::mind::planner::Intent {
        goal: "converse".to_string(),
        memory_anchor: String::new(),
        tone: "gentle".to_string(),
        proactive: true,
        action: "thought_voice".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, &[], &emotion, &intent);

    let now_local = {
        use chrono::{Datelike, Local};
        let local = Local::now();
        let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
            [local.weekday().num_days_from_monday() as usize];
        format!(
            "{}（{}）{}",
            local.format("%Y-%m-%d"),
            weekday,
            local.format("%H:%M")
        )
    };
    messages.push(ChatMessage::user(format!(
        "（现在是{now_local}。你之前独处时心里有过一个念头：「{content}」。如果这个念头此刻还成立，就把它变成你现在随口想说的一句话自然说出来，不照搬原句；如果已经过时了，就只说一句此刻自己的心里话。只说 1 句，第一人称，陈述句优先，不提问。按规则回复，尤其规则 8 严禁编造。）"
    )));

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("voice_thought LLM error: {:?}", e))?;

    let reply = chat_result.content.trim().to_string();
    let reply =
        crate::pending::proactive::grounding_guard(reply, &retrieval, &messages, llm).await;
    if let Some(r) = &reply {
        crate::pending::proactive::log_bubble(db, "thought_voice", r, "", None);
    }
    Ok(reply)
}

/// Checks for unsurfaced internal thoughts that should be expressed now.
/// Returns thoughts matching the `next_interaction` surfacing type,
/// and marks them as surfaced so they are not repeated.
pub fn surface_thoughts(db: &DbState) -> Result<Vec<InternalThought>, String> {
    let now = chrono::Utc::now().to_rfc3339();

    db.with_conn(|conn| {
        let mut unsurfaced = crate::db::reflections::get_unsurfaced(conn)?;
        // Only surface next_interaction type for now.
        unsurfaced.retain(|t| t.surfacing_type == "next_interaction");

        // Mark them as surfaced so they don't repeat.
        for t in &unsurfaced {
            crate::db::reflections::mark_surfaced(conn, &t.id, &now)?;
        }

        // Limit to 1 per interaction (don't dump all thoughts at once).
        if unsurfaced.len() > 1 {
            unsurfaced.truncate(1);
        }

        Ok(unsurfaced)
    })
}

/// Returns the count of unsurfaced thoughts (for debug panel).
pub fn unsurfaced_count(db: &DbState) -> Result<usize, String> {
    db.with_conn(|conn| {
        let thoughts = crate::db::reflections::get_unsurfaced(conn)?;
        Ok(thoughts.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;
    use crate::db::reflections::{insert_thought, InternalThought};

    fn make_thought(id: &str, st: &str) -> InternalThought {
        InternalThought {
            id: id.to_string(),
            content: "test thought".to_string(),
            emotion: Some("happy".to_string()),
            source_reflection: None,
            surfacing_type: st.to_string(),
            created_at: "2026-07-14T22:00:00".to_string(),
            surfaced_at: None,
        }
    }

    #[test]
    fn test_surface_next_interaction() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_thought(conn, &make_thought("t1", "next_interaction"))?;
            insert_thought(conn, &make_thought("t2", "emotion_match"))?;
            Ok(())
        }).unwrap();

        let surfaced = surface_thoughts(&db).unwrap();
        assert_eq!(surfaced.len(), 1);
        assert_eq!(surfaced[0].id, "t1");

        // Second call should return nothing (already surfaced).
        let again = surface_thoughts(&db).unwrap();
        assert_eq!(again.len(), 0);
    }

    #[test]
    fn test_unsurfaced_count() {
        let db = test_db();
        db.with_conn(|conn| {
            insert_thought(conn, &make_thought("t1", "next_interaction"))?;
            insert_thought(conn, &make_thought("t2", "next_interaction"))?;
            Ok(())
        }).unwrap();

        assert_eq!(unsurfaced_count(&db).unwrap(), 2);
    }
}
