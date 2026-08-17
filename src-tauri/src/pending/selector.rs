//! LLM anchor selector (2026-08-16 续⁴¹): the judgment layer that decides
//! WHETHER a memory is worth spontaneously surfacing and WHICH one.
//!
//! Prior to this, anchor selection was purely mechanical (round-robin over
//! fewest-surfaced facts — which actively preferred dusty trivial memories) and
//! the LLM was forced to voice whatever it was handed ("只能围绕它原意来聊"),
//! producing bubbles like "你喝雪碧的时候我都在看着". This module gives the
//! judgment to the LLM with an explicit abstain option — silence becomes a
//! first-class output (Architecture #12), echoing ProACT's stay-silent-or-speak
//! decision points. Rust still assembles the candidate pool and maintains all
//! state (Principle #1); the LLM only chooses.
//!
//! Runs on the reflection model (`chat_reflection`: flash tier, thinking
//! disabled — pure classification, no reasoning budget to burn).

use crate::llm::client::{ChatMessage, LlmClient};
use chrono::{DateTime, Local, Utc};

/// One memory the selector may pick, presented with the metadata a person
/// would implicitly know about their own memory (how old, how often voiced).
#[derive(Debug, Clone)]
pub struct AnchorCandidate {
    /// Stable id for the JSON round-trip: "fact:<id>" or "ep:<id>".
    pub id: String,
    /// "fact" | "episode".
    pub kind: &'static str,
    /// Deictic-neutralized text with the date reference, ready to show.
    pub text: String,
    /// One-line metadata: surfacing history + Rust-computed 由头 hint.
    pub hint: String,
}

/// Which decision the selector is making. `Spontaneous` = the standalone
/// proactive memory bubble (decline ⇒ the whole bubble is cancelled, silence).
/// `Garnish` = welcome-back / lonely nudge, which speak regardless — the
/// selector only decides whether a memory rides along (null anchor ⇒ pure
/// emotional greeting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorTask {
    Spontaneous,
    Garnish,
}

/// Dynamic context the judgment needs: when it is and what she last said
/// unprompted (cross-bubble continuity — the thing the old code-driven path
/// structurally lacked). NOTE: mood is deliberately NOT fed here — the smoke
/// runs showed the selector turning "她此刻平静/犯困" into an atmosphere veto
/// ("氛围不配提起记忆"), which is the wrong question; worthiness is about the
/// USER hearing it, not her current vibe matching it.
pub struct SelectorContext {
    pub task: SelectorTask,
    /// e.g. "2026-08-16（周日）14:32".
    pub now_local: String,
    /// e.g. "下午".
    pub tod: String,
    /// Her recent unprompted bubbles, newest first (already formatted lines).
    pub last_bubbles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SelectorDecision {
    /// Picked candidate id; None = decline (Spontaneous: stay silent;
    /// Garnish: speak without a memory anchor).
    pub anchor_id: Option<String>,
    /// Why (not) — feeds the voicing prompt's 由头 and the Debug Panel log.
    pub reason: String,
}

/// Runs the selector. `Err` (LLM failure / unparseable output after retries)
/// means the CALLER falls back to the mechanical round-robin pick — the
/// feature degrades gracefully, never blocks bubbling (Principle #8 spirit).
pub async fn run(
    llm: &LlmClient,
    candidates: &[AnchorCandidate],
    ctx: &SelectorContext,
) -> Result<SelectorDecision, String> {
    let messages = build_messages(candidates, ctx);
    let valid_ids: Vec<&str> = candidates.iter().map(|c| c.id.as_str()).collect();
    for attempt in 1..=2 {
        let result = llm
            .chat_reflection(&messages, Some(0.2), Some(2048))
            .await
            .map_err(|e| format!("Selector LLM call failed: {:?}", e))?;
        if let Some(decision) = parse(&result.content, &valid_ids) {
            log::info!(
                "[selector] anchor={:?} reason={:?}",
                decision.anchor_id,
                decision.reason
            );
            return Ok(decision);
        }
        log::warn!(
            "[selector] unparseable decision (attempt {}): {:?}",
            attempt,
            result.content
        );
    }
    Err("Selector produced no valid JSON after 2 attempts".to_string())
}

/// Parses the selector reply. Tolerates markdown fences and surrounding text
/// (same tolerance as the gate). Returns None on any malformation, including
/// an anchor_id that is not in the candidate pool — never guess.
pub fn parse(raw: &str, valid_ids: &[&str]) -> Option<SelectorDecision> {
    #[derive(serde::Deserialize)]
    struct Raw {
        anchor_id: Option<String>,
        reason: Option<String>,
    }
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    let parsed: Raw = serde_json::from_str(&trimmed[start..=end]).ok()?;
    let anchor_id = match parsed.anchor_id.as_deref() {
        None => None,
        Some("") => None,
        Some(id) => {
            if valid_ids.contains(&id) {
                Some(id.to_string())
            } else {
                log::warn!("[selector] anchor_id {:?} not in candidate pool", id);
                return None;
            }
        }
    };
    let reason = parsed.reason.unwrap_or_default().trim().to_string();
    if anchor_id.is_none() && reason.is_empty() {
        return None; // declining without any reason is not a decision
    }
    Some(SelectorDecision { anchor_id, reason })
}

fn build_messages(candidates: &[AnchorCandidate], ctx: &SelectorContext) -> Vec<ChatMessage> {
    let mut user = format!("现在是 {}（{}）。\n\n", ctx.now_local, ctx.tod);
    if ctx.last_bubbles.is_empty() {
        user.push_str("［她最近没有主动开口过。］\n\n");
    } else {
        user.push_str("［她最近主动说过的话（新→旧）］\n");
        for line in &ctx.last_bubbles {
            user.push_str(&format!("- {}\n", line));
        }
        user.push('\n');
    }
    user.push_str("［候选记忆］\n");
    for c in candidates {
        user.push_str(&format!("- [{}] {}｜{}\n", c.id, c.text, c.hint));
    }
    user.push_str("\n只输出一个 JSON 对象：");
    vec![ChatMessage::system(selector_prompt(ctx.task)), ChatMessage::user(user)]
}

fn selector_prompt(task: SelectorTask) -> String {
    let mission = match task {
        SelectorTask::Spontaneous => {
            "你是「璃」的记忆浮现判断器。她在考虑要不要主动向用户提起一段回忆，你来判断：此刻值得提起吗？值得的话提哪条？"
        }
        SelectorTask::Garnish => {
            "你是「璃」的记忆浮现判断器。她马上要向用户打个招呼（这个招呼一定会说），你在判断：要不要在招呼里顺便带一条关于 ta 的记忆？要的话带哪条？"
        }
    };
    let worthiness = "判断标准：\n\
        - 值得提起的：正在进行的事——目标、计划、项目、备考、在读的书、最近还有动静的健身进度。例：ta 正在准备找实习，你心里惦记着，路过似的带一句「实习准备得怎么样啦」——这就是值得提起。重要节点、有情感分量的经历同理。\n\
        - 判断「进行中」要看新鲜度和分量（每条候选标了几天前记下）：大事（找工作、备考、大项目）惯性长，一个月没动静也还能问一句；小愿望（想去健身、想吃某家店）惯性短，超过两周没下文多半已经翻篇——把一个月前的小愿望翻出来说「你之前说想健身」，像查档案不像聊天，宁可沉默。真正重要的节点和有情感分量的事不受此限。\n\
        - 好的判断问的是「ta 听到这句会不会觉得温暖、自然」，不是「她此刻的氛围配不配」——她刚说了句犯困的闲话，不代表她不能突然想起一件惦记的事，真人就是这样。\n\
        - 不值得单独提起的：琐碎的日常细节——某次吃了什么喝了什么、随口一提的小偏好。这些单独提起会显得奇怪甚至惊悚（反例：「你喝雪碧的时候我都在看着」），除非有特别由头（正好临近相关日子、和眼下的事直接相关）。\n\
        - 敏感的负面经历（失败、失去、焦虑）要谨慎：只有和眼下直接相关才碰，不要凭空翻旧伤。\n\
        - 候选确实都平庸才输出 null——null 是一个判断结果，不是安全答案。";
    let null_meaning = match task {
        SelectorTask::Spontaneous => "anchor_id 为 null 表示这次不开口（沉默也是一种表达）——但只在候选确实都不值得提起时才用它。",
        SelectorTask::Garnish => "anchor_id 为 null 表示不带记忆，纯情感招呼就好——大多数招呼本来就不需要捎带记忆。",
    };
    let anti_repeat = "别选她最近刚主动提起过的内容（见［她最近主动说过的话］）。";
    let contract = r#"{"anchor_id": "<选中的候选id；不选则 null>", "reason": "<一句话中文：此刻为什么提它合适 / 为什么不提>"}"#;
    format!(
        "{mission}\n\n{worthiness}\n{null_meaning}\n{anti_repeat}\n\n只输出 JSON 对象，格式：{contract}\n不要输出 JSON 以外的任何内容。"
    )
}

/// Formats "how long ago" for a bubble line: 刚刚 / N 分钟前 / N 小时前 / N 天前.
pub fn relative_ago(now: &DateTime<Utc>, then: &DateTime<Utc>) -> String {
    let secs = now.signed_duration_since(*then).num_seconds().max(0);
    if secs < 90 {
        "刚刚".to_string()
    } else if secs < 3600 {
        format!("{} 分钟前", (secs / 60).max(1))
    } else if secs < 86400 {
        format!("{} 小时前", secs / 3600)
    } else {
        format!("{} 天前", secs / 86400)
    }
}

/// Formats a bubble timestamp as local clock time "HH:MM".
pub fn local_clock(then: &DateTime<Utc>) -> String {
    then.with_timezone(&Local).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_pick() {
        let d = parse(
            r#"{"anchor_id": "ep:e9", "reason": "面试在即，她一直惦记"}"#,
            &["ep:e9", "fact:f1"],
        )
        .unwrap();
        assert_eq!(d.anchor_id.as_deref(), Some("ep:e9"));
        assert_eq!(d.reason, "面试在即，她一直惦记");
    }

    #[test]
    fn parse_decline_via_null_or_empty() {
        for raw in [
            r#"{"anchor_id": null, "reason": "都是琐碎小事"}"#,
            r#"{"anchor_id": "", "reason": "都是琐碎小事"}"#,
        ] {
            let d = parse(raw, &["fact:f1"]).unwrap();
            assert!(d.anchor_id.is_none());
            assert_eq!(d.reason, "都是琐碎小事");
        }
    }

    #[test]
    fn parse_tolerates_fences_and_surrounding_text() {
        let d = parse(
            "好的，以下是判断：\n```json\n{\"anchor_id\": \"fact:f1\", \"reason\": \"正好临近\"}\n```",
            &["fact:f1"],
        )
        .unwrap();
        assert_eq!(d.anchor_id.as_deref(), Some("fact:f1"));
    }

    #[test]
    fn parse_rejects_foreign_id_garbage_and_empty_decision() {
        // anchor_id not in the pool → not a decision.
        assert!(parse(r#"{"anchor_id": "fact:zz", "reason": "x"}"#, &["fact:f1"]).is_none());
        // Not JSON at all.
        assert!(parse("我觉得挺好的", &["fact:f1"]).is_none());
        // Decline with no reason at all → not a decision.
        assert!(parse(r#"{"anchor_id": null, "reason": ""}"#, &["fact:f1"]).is_none());
        assert!(parse(r#"{}"#, &["fact:f1"]).is_none());
    }

    #[test]
    fn prompt_carries_worthiness_rules_and_contract() {
        for task in [SelectorTask::Spontaneous, SelectorTask::Garnish] {
            let p = selector_prompt(task);
            assert!(p.contains("雪碧"), "the Sprite-class anti-example guides the judgment");
            assert!(p.contains("null"));
            assert!(p.contains("anchor_id"));
            assert!(p.contains("只输出"));
            // Positive few-shot anchors the judgment (smoke run3 fix): a
            // passing "how's the internship going" IS worth it, and her lazy
            // chatter is not an atmosphere veto; null is a verdict, not a
            // safety default.
            assert!(p.contains("实习准备得怎么样啦"));
            assert!(p.contains("不是「她此刻的氛围配不配」"));
            assert!(p.contains("不是安全答案"));
            // Staleness rule: month-old small wishes read as archive-keeping.
            assert!(p.contains("查档案"));
            assert!(p.contains("几天前记下"), "candidates carry age metadata");
            // Sensitive negatives need direct relevance, never dredged cold.
            assert!(p.contains("不要凭空翻旧伤"));
        }
        // Task-specific null semantics.
        assert!(selector_prompt(SelectorTask::Spontaneous).contains("沉默"));
        assert!(selector_prompt(SelectorTask::Garnish).contains("纯情感招呼"));
    }

    #[test]
    fn build_messages_lists_candidates_and_history() {
        let ctx = SelectorContext {
            task: SelectorTask::Spontaneous,
            now_local: "2026-08-16（周日）14:32".into(),
            tod: "下午".into(),
            last_bubbles: vec!["2 小时前（12:05）：「面试加油呀…」（锚定：在准备找实习）".into()],
        };
        let candidates = vec![AnchorCandidate {
            id: "F1".into(),
            kind: "fact",
            text: "偏好：喝雪碧（ta 7月30日 提到的事）".into(),
            hint: "从未主动提起过，用户提过 1 次".into(),
        }];
        let msgs = build_messages(&candidates, &ctx);
        assert_eq!(msgs.len(), 2);
        let user = msgs[1].content.as_deref().unwrap_or("");
        assert!(user.contains("[F1]"));
        assert!(user.contains("喝雪碧"));
        assert!(user.contains("2 小时前"));
        assert!(user.contains("14:32"));
    }

    #[test]
    fn relative_ago_buckets() {
        let now = Utc::now();
        assert_eq!(relative_ago(&now, &(now - chrono::Duration::seconds(30))), "刚刚");
        assert_eq!(relative_ago(&now, &(now - chrono::Duration::minutes(40))), "40 分钟前");
        assert_eq!(relative_ago(&now, &(now - chrono::Duration::hours(3))), "3 小时前");
        assert_eq!(relative_ago(&now, &(now - chrono::Duration::days(2))), "2 天前");
    }
}
