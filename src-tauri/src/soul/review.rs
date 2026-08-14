//! Relationship Review: periodically summarizes how the relationship with the
//! user is progressing, in the pet's own voice.
//!
//! Hermes-inspired background review (HANDOFF §最近一轮 2026-08-04 续③):
//! every N new conversation episodes the pet steps back and synthesizes a
//! relationship-level summary ("你们最近的关系状态"). The latest summary is
//! always injected into the prompt as always-on relationship context — so the
//! pet carries an understanding of where the relationship stands even when the
//! current topic retrieves no relevant memory.
//!
//! Mirrors reflection.rs: pure `should_run_*` predicate + `run_*` LLM call +
//! `maybe_run_*_if_due` scheduler entry. The LLM only writes the summary text;
//! Rust decides when to run and persists it (Principle 1). Uses reflection_model
//! off the hourly slow tick (Principle 8).

use crate::db::relationship_reviews::{self, RelationshipReview};
use crate::db::DbState;
use crate::llm::client::{ChatMessage, LlmClient};
use serde::{Deserialize, Serialize};

/// New conversation episodes accumulated since the last review that trigger a
/// new one. Sized so a review runs roughly every few active sessions (Principle
/// 8: low-frequency background task). Tunable after real-session observation.
const REVIEW_EPISODE_THRESHOLD: i64 = 15;

/// Outcome of a review run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub review_id: String,
    pub summary: String,
}

/// Whether enough new conversation episodes have accumulated since the last
/// review to warrant another one. Pure (sync, no LLM) so the threshold logic is
/// unit-testable independently of the LLM call.
pub fn should_run_review(db: &DbState) -> bool {
    let since = last_review_at(db);
    let count = count_conversation_episodes_since(db, since.as_deref());
    count >= REVIEW_EPISODE_THRESHOLD
}

/// Runs a single relationship review cycle: gathers recent context, asks the
/// LLM to synthesize a short relationship summary in the pet's voice, and
/// persists it as the new latest review.
pub async fn run_review(db: &DbState, llm: &LlmClient) -> Result<ReviewResult, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let review_id = format!("rev_{}", chrono::Utc::now().timestamp_millis());

    // 1. Gather context: recent episodes, active facts, relationship snapshot,
    //    and the user's chosen nickname (so the summary names them naturally).
    let (episodes_text, facts_text, rel_text, user_name) = db.with_conn(|conn| {
        let episodes = crate::db::episodes::get_recent(conn, 30)?;
        let episodes_text = episodes
            .iter()
            .map(|e| format!("- {}", e.summary))
            .collect::<Vec<_>>()
            .join("\n");
        let facts = crate::db::facts::get_all_active(conn, 20)?;
        let facts_text = if facts.is_empty() {
            "（暂无）".to_string()
        } else {
            facts
                .iter()
                .map(|f| format!("- {}: {}", f.key, f.value))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let rel = crate::db::relationship::get(conn)?;
        let rel_text = format!(
            "亲密度 {}/100，认识 {} 天，聊过 {} 次",
            rel.closeness as i64, rel.days_known, rel.total_conversations
        );
        let profile = crate::db::onboarding::load(conn)?;
        let user_name = profile
            .user_nickname
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "你".to_string());
        Ok::<_, String>((episodes_text, facts_text, rel_text, user_name))
    })?;

    // 2. Build the prompt. Chinese because the pet replies in Chinese. The pet
    //    summarizes in its own voice (璃) and must only use real memories
    //    (Principle 3: no fabrication).
    let prompt = format!(
        "你在回顾和「{user_name}」最近的相处。用一两句话总结你们现在的关系状态、最近发生了什么、\
相处的氛围如何。语气要像你自己——温柔、安静、具体、不煽情。\
只根据下面的记忆总结真实发生过的事，记忆里没有的不要写，不要编造。\
直接输出总结本身，不要加「总结：」之类的前缀，不要解释。\n\n\
[关系状态] {rel_text}\n\
[关于 ta 的事实]\n{facts_text}\n\
[最近的记忆]\n{episodes_text}",
        user_name = user_name,
        rel_text = rel_text,
        facts_text = facts_text,
        episodes_text = episodes_text,
    );

    // 3. Call LLM with the reflection model (cheapest). 4096: generation task
    //    — DeepSeek-v4 is a reasoning model, so reasoning_content eats most of
    //    a smaller budget and leaves `content` empty (pitfall #3).
    let messages = vec![ChatMessage::system(prompt)];
    let result = llm
        .chat_reflection(&messages, Some(0.5), Some(4096))
        .await
        .map_err(|e| format!("Relationship review LLM call failed: {}", e))?;

    let summary = result.content.trim().to_string();
    // Empty content (reasoning ate the budget, or transient LLM failure) must
    // not be persisted as a garbage review. Propagate as an error so the
    // scheduler logs it and retries next cycle. Mirrors consolidation's guard.
    if summary.is_empty() {
        log::warn!("[review] LLM returned empty summary; skipping (will retry next cycle)");
        return Err("Relationship review produced empty summary".to_string());
    }

    // 4. Persist.
    db.with_conn(|conn| {
        relationship_reviews::insert(
            conn,
            &RelationshipReview {
                id: review_id.clone(),
                summary: summary.clone(),
                created_at: now,
            },
        )
    })?;

    log::info!("[review] relationship review generated: {}", summary);
    Ok(ReviewResult { review_id, summary })
}

/// Runs a relationship review if one is due. Returns true when a review ran,
/// false when skipped. Errors propagate so the caller (life-loop slow tick) can
/// log them without crashing (Principle 6: degrade gracefully).
pub async fn maybe_run_review_if_due(db: &DbState, llm: &LlmClient) -> Result<bool, String> {
    if !should_run_review(db) {
        return Ok(false);
    }
    let r = run_review(db, llm).await?;
    log::info!("[review] ran: {}", r.summary);
    Ok(true)
}

/// Timestamp (rfc3339) of the most recent review, or None if none recorded.
fn last_review_at(db: &DbState) -> Option<String> {
    db.with_conn(|conn| relationship_reviews::latest_created_at(conn))
        .ok()
        .flatten()
}

/// Counts conversation episodes created after `since` (or all conversation
/// episodes if None). Mirrors reflection's TurnThreshold counter. A DB error
/// degrades to 0 (no review) rather than crashing the scheduler.
fn count_conversation_episodes_since(db: &DbState, since: Option<&str>) -> i64 {
    db.with_conn(|conn| {
        let count: i64 = if let Some(s) = since {
            conn.query_row(
                "SELECT COUNT(*) FROM episodes WHERE source_type='conversation' AND created_at > ?1",
                rusqlite::params![s],
                |row| row.get(0),
            )
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM episodes WHERE source_type='conversation'",
                [],
                |row| row.get(0),
            )
        }
        .unwrap_or(0);
        Ok(count)
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::test_db;

    fn insert_episode_at(db: &DbState, id: &str, created_at: &str, source_type: &str) {
        db.with_conn(|conn| {
            crate::db::episodes::insert(conn, &crate::db::episodes::Episode {
                id: id.to_string(),
                time: created_at.to_string(),
                summary: "s".to_string(),
                emotion: None,
                importance: 0.5,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: source_type.to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.5,
                recall_count: 0,
                last_recalled_at: None,
                consolidated: false,
                created_at: created_at.to_string(),
            })
        })
        .unwrap();
    }

    fn insert_review_at(db: &DbState, id: &str, created_at: &str) {
        db.with_conn(|conn| {
            relationship_reviews::insert(
                conn,
                &RelationshipReview {
                    id: id.to_string(),
                    summary: "之前的关系总结".to_string(),
                    created_at: created_at.to_string(),
                },
            )
        })
        .unwrap();
    }

    #[test]
    fn review_never_reviewed_with_enough_episodes() {
        let db = test_db();
        for i in 0..REVIEW_EPISODE_THRESHOLD {
            insert_episode_at(&db, &format!("ep{i}"), "2026-08-07T10:00:00", "conversation");
        }
        assert!(should_run_review(&db), "no prior review + enough episodes -> due");
    }

    #[test]
    fn review_not_enough_episodes() {
        let db = test_db();
        for i in 0..(REVIEW_EPISODE_THRESHOLD - 1) {
            insert_episode_at(&db, &format!("ep{i}"), "2026-08-07T10:00:00", "conversation");
        }
        assert!(!should_run_review(&db), "threshold-1 episodes -> not due");
    }

    #[test]
    fn review_counts_only_conversation_episodes() {
        let db = test_db();
        for i in 0..REVIEW_EPISODE_THRESHOLD {
            insert_episode_at(&db, &format!("ep{i}"), "2026-08-07T10:00:00", "consolidation");
        }
        assert!(
            !should_run_review(&db),
            "non-conversation episodes don't count toward the review threshold"
        );
    }

    #[test]
    fn review_skips_when_recent_review_has_few_new_episodes() {
        let db = test_db();
        // A review happened; only a few episodes since.
        insert_review_at(&db, "rev_old", "2026-08-07T10:00:00");
        for i in 0..5 {
            insert_episode_at(&db, &format!("ep{i}"), "2026-08-07T11:00:00", "conversation");
        }
        assert!(!should_run_review(&db), "few episodes since last review -> not due");
    }

    #[test]
    fn review_due_when_enough_new_episodes_after_last_review() {
        let db = test_db();
        insert_review_at(&db, "rev_old", "2026-08-07T10:00:00");
        for i in 0..REVIEW_EPISODE_THRESHOLD {
            // created_at AFTER the review's 10:00:00
            insert_episode_at(&db, &format!("ep{i}"), "2026-08-07T11:00:00", "conversation");
        }
        assert!(should_run_review(&db), ">= threshold episodes after last review -> due");
    }

    #[test]
    fn review_does_not_count_episodes_before_last_review() {
        let db = test_db();
        // Episodes BEFORE the review should not count.
        for i in 0..REVIEW_EPISODE_THRESHOLD {
            insert_episode_at(&db, &format!("ep_old{i}"), "2026-08-07T09:00:00", "conversation");
        }
        insert_review_at(&db, "rev", "2026-08-07T10:00:00");
        assert!(!should_run_review(&db), "episodes before the last review must not count");
    }
}
