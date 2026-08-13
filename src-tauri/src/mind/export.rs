//! Memory export: full JSON backup + human-readable Markdown.
//!
//! The JSON dump contains EVERY table with full fields (suitable for future
//! restore/import). The Markdown dump contains only the core readable memory
//! (episodes timeline, facts by category, pending events) — vectors and
//! internal plumbing are omitted from MD because they aren't human-meaningful.
//!
//! Both builders run all reads inside a single `with_conn` so we take the DB
//! lock once and see a consistent snapshot.

use crate::db as db;
use crate::db::DbState;
use serde::Serialize;

/// Schema version of the export envelope. Bump when the JSON shape changes
/// in a backward-incompatible way; an importer can branch on this.
const SCHEMA_VERSION: u32 = 1;

/// The full export envelope. Every field is the complete table contents.
#[derive(Debug, Serialize)]
pub struct MemoryExport {
    pub schema_version: u32,
    pub exported_at: String,
    pub episodes: Vec<db::episodes::Episode>,
    pub facts: Vec<db::facts::Fact>,
    pub conversations: Vec<db::conversations::ConversationRow>,
    pub pending_events: Vec<db::pending::PendingEvent>,
    pub vectors: Vec<VectorExport>,
    pub relationship: Option<db::relationship::Relationship>,
    pub relationship_reviews: Vec<db::relationship_reviews::RelationshipReview>,
    pub persona_traits: Vec<db::persona::PersonaTrait>,
    pub emotion: Option<db::emotion::EmotionState>,
    pub user_profile: db::onboarding::UserProfile,
    pub reflections: Vec<db::reflections::Reflection>,
    pub internal_thoughts: Vec<db::reflections::InternalThought>,
    pub change_log: Vec<db::changelog::ChangeLogEntry>,
}

/// An episode vector in JSON-friendly form (id + the float array).
#[derive(Debug, Serialize)]
pub struct VectorExport {
    pub episode_id: String,
    pub embedding: Vec<f32>,
}

/// Collects the full snapshot. One DB lock, consistent view.
fn collect(db: &DbState) -> Result<MemoryExport, String> {
    db.with_conn(|conn| {
        let episodes = db::episodes::get_all(conn)?;
        let facts = db::facts::get_all(conn)?;
        let conversations = db::conversations::get_all(conn)?;
        let pending_events = db::pending::get_all(conn)?;
        let vectors = db::vectors::get_all(conn)?
            .into_iter()
            .map(|(id, emb)| VectorExport {
                episode_id: id,
                embedding: emb,
            })
            .collect::<Vec<_>>();
        let relationship = db::relationship::get(conn).ok();
        let relationship_reviews = db::relationship_reviews::get_all(conn)?;
        let persona_traits = db::persona::get_all_traits(conn).unwrap_or_default();
        let emotion = db::emotion::get(conn).ok();
        let user_profile = db::onboarding::load(conn)?;
        let reflections = db::reflections::get_all_reflections(conn)?;
        let internal_thoughts = db::reflections::get_all_thoughts(conn)?;
        // Audit log: keep the most recent 200 (full history is rarely useful in
        // a backup and can be very large).
        let change_log = db::changelog::recent(conn, 200)?;

        Ok(MemoryExport {
            schema_version: SCHEMA_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            episodes,
            facts,
            conversations,
            pending_events,
            vectors,
            relationship,
            relationship_reviews,
            persona_traits,
            emotion,
            user_profile,
            reflections,
            internal_thoughts,
            change_log,
        })
    })
}

/// Builds the full JSON backup as a pretty-printed string.
pub fn build_json(db: &DbState) -> Result<String, String> {
    let snapshot = collect(db)?;
    serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Failed to serialize export: {}", e))
}

/// Builds a human-readable Markdown document covering only the core memory:
/// episodes (timeline), facts (by category), pending events (by status).
pub fn build_markdown(db: &DbState) -> Result<String, String> {
    let snapshot = collect(db)?;
    Ok(render_markdown(&snapshot))
}

fn render_markdown(s: &MemoryExport) -> String {
    let mut out = String::with_capacity(8192);

    out.push_str("# 桌宠记忆导出\n\n");
    out.push_str(&format!("> 导出时间：{}\n\n", s.exported_at));

    // ---- Relationship snapshot ----
    out.push_str("## 关系状态\n\n");
    if let Some(rel) = &s.relationship {
        out.push_str(&format!(
            "- 亲密度：{:.1}　信任：{:.1}　相识天数：{}　总对话轮次：{}\n",
            rel.closeness, rel.trust, rel.days_known, rel.total_conversations
        ));
    } else {
        out.push_str("- （暂无关系数据）\n");
    }
    if let Some(emo) = &s.emotion {
        out.push_str(&format!(
            "- 当前情绪：{}（mood {:.2} / 能量 {:.2} / 社交 {:.2}）\n",
            emo.mood_label, emo.mood, emo.physical_energy, emo.social_battery
        ));
    }
    out.push('\n');

    // ---- Episodes timeline ----
    out.push_str(&format!("## 经历（{} 条）\n\n", s.episodes.len()));
    // Newest first for readability.
    let mut eps_sorted = s.episodes.clone();
    eps_sorted.sort_by(|a, b| b.time.cmp(&a.time));
    for ep in &eps_sorted {
        let landmark = if ep.is_landmark { " ★核心" } else { "" };
        out.push_str(&format!(
            "### {}{}\n", ep.summary, landmark
        ));
        out.push_str(&format!("- 时间：{}\n", ep.time));
        if let Some(e) = &ep.emotion {
            out.push_str(&format!("- 情绪：{}\n", e));
        }
        out.push_str(&format!(
            "- 重要度：{:.2}　记忆强度：{:.2}　回忆次数：{}\n",
            ep.importance, ep.memory_strength, ep.recall_count
        ));
        if let Some(t) = &ep.topics {
            if !t.is_empty() {
                out.push_str(&format!("- 话题：{}\n", t));
            }
        }
        if let Some(p) = &ep.participants {
            if !p.is_empty() {
                out.push_str(&format!("- 参与者：{}\n", p));
            }
        }
        out.push('\n');
    }

    // ---- Facts by category ----
    out.push_str(&format!("## 关于你的事（{} 条）\n\n", s.facts.len()));
    // Group active facts by category; expired ones get a separate bucket.
    use std::collections::BTreeMap;
    let mut by_cat: BTreeMap<String, Vec<&db::facts::Fact>> = BTreeMap::new();
    let mut expired: Vec<&db::facts::Fact> = Vec::new();
    for f in &s.facts {
        if f.valid_to.is_none() {
            by_cat.entry(f.category.clone()).or_default().push(f);
        } else {
            expired.push(f);
        }
    }
    for (cat, facts) in &by_cat {
        out.push_str(&format!("### {}\n\n", cat));
        for f in facts {
            out.push_str(&format!(
                "- **{}**：{}（置信度 {:.2}，提及 {} 次）\n",
                f.key, f.value, f.confidence, f.mention_count
            ));
        }
        out.push('\n');
    }
    if !expired.is_empty() {
        out.push_str(&format!("### 已过期/被修正（{} 条）\n\n", expired.len()));
        for f in &expired {
            out.push_str(&format!(
                "- ~~**{}**：{}~~（过期于 {}）\n",
                f.key,
                f.value,
                f.valid_to.as_deref().unwrap_or("?")
            ));
        }
        out.push('\n');
    }

    // ---- Pending events ----
    out.push_str(&format!("## 待办与提醒（{} 条）\n\n", s.pending_events.len()));
    let mut by_status: BTreeMap<String, Vec<&db::pending::PendingEvent>> = BTreeMap::new();
    for p in &s.pending_events {
        by_status.entry(p.status.clone()).or_default().push(p);
    }
    for (status, events) in &by_status {
        let label = match status.as_str() {
            "pending" => "待处理",
            "triggered" => "已触发",
            "resolved" => "已完成",
            other => other,
        };
        out.push_str(&format!("### {}（{}）\n\n", label, events.len()));
        for ev in events {
            out.push_str(&format!("- **{}**　事件日期：{}", ev.title, ev.event_date));
            if let Some(r) = &ev.remind_date {
                out.push_str(&format!("　提醒：{}", r));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    out
}
