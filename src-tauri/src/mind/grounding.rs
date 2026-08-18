//! Grounded Generation: builds the system prompt with memory constraints
//! and formats retrieved memories with confidence/source annotations.
//! Design doc 5.10: LLM may only reference retrieved memories; must say
//! "not sure" rather than fabricate when relevant memory is absent.

use crate::db::relationship as db_relationship;
use crate::emotion::state::EmotionState;
use crate::mind::retrieval::RetrievalResult;
use crate::mind::planner::Intent;

/// Static personality template loaded at compile time.
const SYSTEM_TEMPLATE: &str = include_str!("../../resources/prompts/system.txt");

/// Builds the FIRST (system) message of a conversation request — the STATIC
/// prefix. L2a+ cache discipline: this message must be byte-identical between
/// consecutive turns, because DeepSeek's context cache only hits when a request
/// fully matches a persisted "cache prefix unit" — one changed character in
/// messages[0] invalidates the entire request prefix and bills it as a cache
/// miss (31x price gap on v4-flash).
///
/// Static here: identity template + static persona lines (traits / nickname /
/// pet name / style roots) + the grounding guardrail. Everything that moves
/// per turn — relationship numbers (closeness / days_known / conversation
/// counts change every turn), milestone ledger, relationship review and the
/// retrieved [Memories] block — is rendered by
/// `build_trailing_memory_context` and injected AFTER the history by the budget
/// allocators. A volatile tail may miss; the static head always hits.
///
/// Params are kept in the signature so the ~19 call sites stay unchanged.
fn current_time_section() -> String {
    use chrono::Datelike;
    use crate::perception::time::{current_time_of_day, TimeOfDay};
    let now = chrono::Local::now();
    let weekday_cn = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [now.weekday().num_days_from_monday() as usize];
    let tod_cn = match current_time_of_day() {
        TimeOfDay::Morning => "上午",
        TimeOfDay::Afternoon => "下午",
        TimeOfDay::Evening => "晚上",
        TimeOfDay::LateNight => "深夜",
        TimeOfDay::DeepNight => "凌晨",
    };
    format!(
        "[Current time]\n现在 {} {} {}\n时段：{}",
        now.format("%H:%M"),
        weekday_cn,
        now.format("%Y-%m-%d"),
        tod_cn,
    )
}

pub fn build_system_prompt(
    retrieval: &RetrievalResult,
    _emotion: &EmotionState,
    _intent: &Intent,
) -> String {
    let mut sections = vec![SYSTEM_TEMPLATE.to_string()];

    // 1. Static persona lines (traits + onboarding profile). Relationship
    // live numbers moved to the trailing context — they change every turn.
    sections.push(format_persona_static(retrieval));

    // 2. Grounding constraint (static)
    sections.push(MEMORY_CONSTRAINT.to_string());

    sections.join("\n\n")
}

/// Trailing dynamic context (L2a+ cache discipline): relationship live numbers
/// + milestone ledger + [Relationship] review + [Memories]. Rendered as ONE
/// system message injected after the conversation history by the budget
/// allocators (and before the near-end directive), so it never touches the
/// static first-message prefix. Always non-empty: an empty retrieval still
/// yields the explicit no-fabrication marker (hallucination root-cause fix —
/// the model must be able to tell memory retrieval came up empty).
pub fn build_trailing_memory_context(retrieval: &RetrievalResult) -> String {
    let mut sections: Vec<String> = Vec::new();
    if let Some(rel) = format_relationship(retrieval) {
        sections.push(format!("[Relationship]\n{}", rel));
    }
    let memories = format_memories(retrieval);
    if !memories.is_empty() {
        sections.push(memories);
    }
    sections.join("\n\n")
}

/// QA-mode trailing relationship snapshot (direct-answer route keeps identity
/// but no [Memories]; the live relationship line still rides the tail so the
/// first QA system message stays static too). None when no relationship row.
pub fn build_qa_relationship_section(retrieval: &RetrievalResult) -> Option<String> {
    format_relationship(retrieval).map(|rel| format!("[Relationship]\n{}", rel))
}

/// Near-end directive (Soul v2 plan L2a): current time + current mood (with
/// an optional expressive-permission hint) + this turn's intent, injected as
/// a trailing system message after the conversation history. The recency
/// slot is the strongest steering channel — and keeping volatile content out
/// of the top system message keeps the static prefix cache-hit.
pub fn build_near_end_directive(emotion: &EmotionState, intent: &Intent) -> String {
    let mut s = current_time_section();
    s.push_str("\n\n");
    s.push_str(&format_emotion(emotion));
    if let Some(hint) = tone_hint(emotion, intent) {
        s.push_str(&format!("\n（如果和此刻话题不冲突：{hint}）"));
    }
    s.push_str("\n\n");
    s.push_str(&format_intent(intent));
    s
}

/// The direct-answer directive injected for question routes. Written in
/// Chinese because the pet replies in Chinese and the directive must steer
/// the generation language directly (same rationale as the bilingual
/// anti-fabrication rule).
const QA_MODE_PROMPT: &str = "\
[Direct-Answer Mode]
用户这次问的是知识、技术或事实类问题，ta 想要的是答案，不是闲聊。直接、准确、简短地回答，\
像朋友随口解释一样自然，不要上课、不要绕圈子。不要引用记忆，不要追问，不要往自己或宠物相关话题上联想。\
不要假装记得用户说过什么，不要编造用户的过去、偏好或经历——你不知道的事就别说。\
通常一两句话就够，除非问题本身确实需要展开。不确定就老实说不知道。";

/// Builds the system prompt for direct-answer (question) mode.
///
/// Same persona/emotion scaffold as the normal prompt, but deliberately
/// WITHOUT the retrieved memories section and grounding constraint, so the
/// model cannot be steered into hard-associating an unrelated knowledge
/// question with past pet topics (the "你是不是指我的背带" failure mode).
/// Memory focus is stripped from the intent for the same reason.
pub fn build_qa_system_prompt(
    retrieval: &RetrievalResult,
    _emotion: &EmotionState,
    _intent: &Intent,
) -> String {
    // Soul v2 L2a: static part only — the QA directive / time / mood / intent
    // moved to `build_qa_near_end` (mirrors the main-path split).
    let mut sections = vec![SYSTEM_TEMPLATE.to_string()];

    sections.push(format_persona_static(retrieval));

    sections.join("\n\n")
}

/// QA-mode near-end directive: direct-answer directive + time + mood +
/// neutralized intent (goal=converse, no memory focus).
pub fn build_qa_near_end(emotion: &EmotionState, intent: &Intent) -> String {
    let mut qa_intent = intent.clone();
    qa_intent.goal = "converse".to_string();
    qa_intent.memory_anchor.clear();
    let mut s = QA_MODE_PROMPT.to_string();
    s.push_str("\n\n");
    s.push_str(&current_time_section());
    s.push_str("\n\n");
    s.push_str(&format_emotion(emotion));
    s.push_str("\n\n");
    s.push_str(&format_intent(&qa_intent));
    s
}

/// The grounding guardrail text injected into every system prompt.
const MEMORY_CONSTRAINT: &str = "\
[Grounding Constraint]
The following memories are what you actually retrieved. You may respond based on \
these memories about the user. If you have no relevant memory for something, \
say you are not sure rather than fabricating. Each memory below is annotated \
with its confidence level and source date. Do not present information as \
remembered unless it appears in the memories section below.";

/// Formats the STATIC persona lines: traits + onboarding profile. Relationship
/// live numbers are excluded (`format_relationship`) — they change every turn
/// and would break the static first-message cache prefix.
fn format_persona_static(retrieval: &RetrievalResult) -> String {
    let traits = &retrieval.persona_traits;
    let user_profile = &retrieval.user_profile;
    let mut lines = vec!["[Persona]".to_string()];

    // Core traits
    let core_traits: Vec<&str> = traits
        .iter()
        .filter(|t| t.trait_type == "core")
        .map(|t| t.trait_key.as_str())
        .collect();
    if !core_traits.is_empty() {
        lines.push(format!("Core personality: {}", core_traits.join(", ")));
    }

    // Adaptive traits
    let adaptive_traits: Vec<&str> = traits
        .iter()
        .filter(|t| t.trait_type == "adaptive")
        .map(|t| t.trait_key.as_str())
        .collect();
    if !adaptive_traits.is_empty() {
        lines.push(format!("Adaptive traits: {}", adaptive_traits.join(", ")));
    }

    // Onboarding profile (user-chosen at first launch): nickname, pet name,
    // personality, relationship style. Primary identity signals for the LLM.
    if let Some(nn) = &user_profile.user_nickname {
        lines.push(format!("用户的称呼: {}", nn));
    }
    if let Some(pn) = &user_profile.pet_name {
        lines.push(format!("你的名字: {}", pn));
    }
    if let Some(ps) = &user_profile.personality_style {
        // Not "你被期望的性格" — that framing is a casting-mask cue that
        // licenses performing a role; these settings are the seed she grew
        // her personality from (Soul v2 plan §3.3).
        lines.push(format!("你性格的底子（ta 最初的心愿）: {}", ps));
    }
    if let Some(rs) = &user_profile.relationship_style {
        lines.push(format!("与用户的关系设定: {}", rs));
    }

    if lines.len() == 1 {
        // No traits or relationship data yet
        lines.push("A warm, gentle desktop companion who cares about the user.".to_string());
    }

    lines.join("\n")
}

/// Formats the VOLATILE relationship snapshot line(s) — closeness / trust /
/// days_known / conversation totals all change every turn, so this rides the
/// trailing context (never the static first-message prefix).
pub fn format_relationship(retrieval: &RetrievalResult) -> Option<String> {
    let relationship = retrieval.relationship.as_ref()?;
    // `trust` is a dead column (never written) — injecting "trust 0.0" next to
    // a real closeness is a contradictory signal, so it stays hidden until
    // something actually maintains it (宁缺勿假). `days_known` is backfilled
    // at read time in retrieval.
    let trust_part = if relationship.trust > 0.0 {
        format!(", trust {:.1}/100", relationship.trust)
    } else {
        String::new()
    };
    // days_known rides with the first-met DATE so she never has to do
    // date arithmetic herself — asked "我们认识多久了" she mis-derived
    // "7月18号" from a bare day count (true anchor: 7月16号).
    let known_part = match &retrieval.first_met {
        Some(date) => format!(
            "known each other since {} ({} days)",
            date, relationship.days_known
        ),
        None => format!("known {} days", relationship.days_known),
    };
    Some(format!(
        "Relationship: closeness {}/100{}, {}, {} conversations",
        relationship.closeness as i32,
        trust_part,
        known_part,
        relationship.total_conversations,
    ))
}

/// Formats the emotion state as a concise snapshot.
fn format_emotion(emotion: &EmotionState) -> String {
    let label = crate::emotion::state::derive_mood_label(emotion);
    format!(
        "[Current Mood] {} (mood {:.1}, energy {:.1}, social {:.1}, stress {:.1})",
        label,
        emotion.mood,
        emotion.physical_energy,
        emotion.social_battery,
        emotion.stress,
    )
}

/// Expressive-permission hint from the current emotion (Soul v2 plan §3.2).
/// Pure Rust decision (Principle #1): the LLM only voices, never decides its
/// own state. Phrasing is PERMISSION, not a state command (GPT 评审 C3):
/// "今晚可以慢一点" grants expressive latitude instead of ordering a mood
/// the model would perform. Emotion accumulates mathematically while the
/// live conversation may be mid-flow — hence the weak-hint framing.
fn tone_hint(emotion: &EmotionState, intent: &Intent) -> Option<&'static str> {
    use crate::perception::time::current_time_of_day;
    // Distress yield (报告 §7.4 危机时少说): when the planner routed this
    // turn to emotional support (care/listen) or silence, the reaching-out /
    // playful hints stand down — quieter, steadier, catch the person first.
    let distress =
        matches!(intent.goal.as_str(), "care" | "listen") || intent.action == "silence";
    tone_hint_at(emotion, distress, current_time_of_day())
}

/// Pure, clock-free core of `tone_hint` (testable with an explicit tod).
fn tone_hint_at(
    emotion: &EmotionState,
    distress: bool,
    tod: crate::perception::time::TimeOfDay,
) -> Option<&'static str> {
    use crate::perception::time::TimeOfDay;
    if emotion.stress > 0.65 {
        return Some("今晚可以慢一点，不着急把话说完整");
    }
    if matches!(tod, TimeOfDay::LateNight | TimeOfDay::DeepNight) && emotion.rest_need > 0.6 {
        return Some("困了的话，句子可以更短更糊，没关系");
    }
    if !distress && emotion.loneliness > 0.6 && !matches!(tod, TimeOfDay::DeepNight) {
        return Some("要是想搭句话也可以，一句就好，不追问");
    }
    if !distress && emotion.mood >= 0.7 {
        return Some("心情不错的话，语气可以带点雀跃");
    }
    None
}

/// Formats the Planner's intent as a directive.
fn format_intent(intent: &Intent) -> String {
    let mut s = format!(
        "[Intent] goal: {}",
        if intent.goal.is_empty() { "converse naturally".to_string() } else { intent.goal.clone() }
    );
    if !intent.memory_anchor.is_empty() {
        s.push_str(&format!("\nmemory focus: {}", intent.memory_anchor));
    }
    if !intent.tone.is_empty() {
        s.push_str(&format!("\ntone: {}", intent.tone));
    }
    if intent.proactive {
        s.push_str("\n(be proactive: bring up the memory naturally)");
    }
    if intent.goal == "engage" {
        s.push_str("\n(engage: react specifically to what they just shared — prove you listened with something concrete. You may ask ONE genuine follow-up if you're actually curious, but often a single heartfelt line with no question is more natural. Never ask a generic '怎么样'.)");
    } else if intent.goal == "react" {
        s.push_str("\n(react: react warmly and specifically to what they just shared, but do NOT ask any question this turn — just be present and show you listened)");
    }
    s
}

/// Formats all retrieved memories (facts + episodes) with annotations.
///
/// Hermes-inspired split ledger: landmark episodes (关系账——relationship
/// milestones like "user accepted the job offer") get their own [Milestones]
/// section ahead of ordinary memories, so the pet treats them as
/// relationship anchors rather than one more fact. Regular episodes and
/// facts stay under [Memories].
fn format_memories(retrieval: &RetrievalResult) -> String {
    let now = chrono::Utc::now();
    let mut sections = Vec::new();

    // Landmark episodes = relationship ledger (milestones worth anchoring on).
    let milestones: Vec<&crate::db::episodes::Episode> = retrieval
        .episodes
        .iter()
        .map(|se| &se.episode)
        .filter(|e| e.is_landmark)
        .collect();
    if !milestones.is_empty() {
        let mut lines = vec!["[Milestones]".to_string()];
        for ep in milestones {
            let date = ep.time.split('T').next().unwrap_or("?");
            lines.push(format!(
                "- {} (emotion: {}, date: {})",
                ep.summary,
                ep.emotion.as_deref().unwrap_or("neutral"),
                date,
            ));
        }
        sections.push(lines.join("\n"));
    }

    // Latest relationship review — always-on relationship context (the pet's
    // synthesized understanding of where the relationship stands), independent
    // of what the current topic retrieved. Placed before [Memories] so it acts
    // as a relationship anchor rather than one more fact.
    if let Some(review) = &retrieval.relationship_review {
        sections.push(format!("[Relationship]\n{}", review));
    }

    let mut lines = vec!["[Memories]".to_string()];

    // Facts sorted by confidence (already done in retrieval, but ensure here)
    let mut sorted_facts = retrieval.facts.clone();
    sorted_facts.sort_by(|a, b| {
        b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
    });

    for fact in &sorted_facts {
        let when = memory_when(&fact.created_at, &now);
        lines.push(format!(
            "- [Fact] {} / {}: {} (confidence: {}, {})",
            fact.category,
            fact.key,
            fact.value,
            confidence_label(fact.confidence),
            when,
        ));
    }

    // Episodes sorted by score (already done in retrieval); landmarks already
    // surfaced above in [Milestones], skip them here to avoid duplication.
    for scored_ep in retrieval
        .episodes
        .iter()
        .filter(|se| !se.episode.is_landmark)
    {
        let ep = &scored_ep.episode;
        let when = memory_when(&ep.time, &now);
        lines.push(format!(
            "- [Episode] {} (importance: {}, emotion: {}, {})",
            ep.summary,
            importance_label(ep.importance),
            ep.emotion.as_deref().unwrap_or("neutral"),
            when,
        ));
    }

    if lines.len() == 1 && sections.is_empty() {
        // Explicit empty marker. Without it, no [Memories] section appears in
        // the prompt at all, so the model can't tell memory is empty and
        // fabricates "你上次说…" threads from the personality examples — the
        // measured hallucination root cause. An inline marker is the active
        // signal the model actually reads; a buried rule in the system prompt
        // does not reliably reach it (verified across 4 test rounds).
        return "[Memories]\n（暂无相关记忆——不要提及或编造任何过往，只就当下回应用户。）".to_string();
    }

    if lines.len() > 1 {
        // Inline guard, placed at the point of use (not 1700 tokens away in
        // the system prompt): the model may ONLY cite what is listed above.
        // Targets the G6 failure mode — wrapping a real memory in a fake
        // "你上次说/提过/念叨" source, or inventing extra topics — which a distant
        // rule does not stop under thinking-off, but an adjacent note does.
        // The time clause targets the 2026-08-17 incident: a 4-day-old hotpot
        // memory voiced as "你昨天说想吃火锅" — the ISO date buried in metadata
        // was ignored; the salient relative date + explicit rule now carry it.
        lines.push(
            "（以上即全部记忆。只可引用已列出的内容；不得添加未列出的项目、人名、事件，"
                .to_string()
                + "也不得编造\"你上次说/提过/念叨\"之类的出处——记着就是记着，没有出处别硬安一个。"
                + "每条记忆都标了它是多久前的事：提到它的时间必须照标注说——标着「4天前」的绝不能说成「昨天」，不确定就说\"之前\"；"
                + "也不要主动报日期出处——「上个月你提到」不像聊天，像查档案，被问到再说。）",
        );
        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

/// High-salience relative time for a memory line: "今天" / "昨天" / "前天" /
/// "N天前（M月D日）" / "M月D日" (stale) / "YYYY年M月D日" (last year or older).
/// Replaces the old buried ISO "source: 2026-08-13" — the model ignored it and
/// confabulated "昨天" for a 4-day-old memory (2026-08-17 incident).
fn memory_when(iso: &str, now: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Datelike;
    let dt = match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(d) => d.with_timezone(&chrono::Utc),
        Err(_) => return iso.split('T').next().unwrap_or("?").to_string(),
    };
    let local = dt.with_timezone(&chrono::Local);
    let now_local = now.with_timezone(&chrono::Local);
    let days = (now_local.date_naive() - local.date_naive()).num_days();
    let md = format!("{}月{}日", local.month(), local.day());
    match days {
        0 => "今天".to_string(),
        1 => "昨天".to_string(),
        2 => "前天".to_string(),
        3..=60 => format!("{days}天前（{md}）"),
        _ if local.year() == now_local.year() => md,
        _ => format!("{}年{md}", local.year()),
    }
}

/// Maps a numeric confidence to a human-readable label.
fn confidence_label(confidence: f64) -> &'static str {
    if confidence >= 0.8 {
        "high"
    } else if confidence >= 0.5 {
        "medium"
    } else {
        "low"
    }
}

/// Maps a numeric importance to a human-readable label.
fn importance_label(importance: f64) -> &'static str {
    if importance >= 0.7 {
        "high"
    } else if importance >= 0.4 {
        "medium"
    } else {
        "low"
    }
}

/// Lightweight check for potential hallucination.
/// Scans the LLM response for assertion patterns about the user and checks
/// whether any provided memory supports them. Returns a list of ungrounded
/// references.
///
/// This is intentionally conservative and simple (no LLM post-processing per
/// architecture principle #8). It flags references that look like specific
/// facts ("you said X", "you like Y") but don't match any provided memory.
pub fn check_groundedness(
    response: &str,
    retrieval: &RetrievalResult,
) -> Vec<String> {
    let mut violations = Vec::new();

    // Gather all values from provided facts for matching.
    let fact_values: Vec<&str> = retrieval
        .facts
        .iter()
        .map(|f| f.value.as_str())
        .collect();

    let ep_summaries: Vec<&str> = retrieval
        .episodes
        .iter()
        .map(|e| e.episode.summary.as_str())
        .collect();

    // Claim patterns: phrases that assert something about the user's past or
    // preferences. If the response makes such a claim but no provided memory
    // overlaps the claim window, flag it. English + Chinese — Liri replies in
    // Chinese, so the EN-only set caught nothing. Patterns are kept
    // high-precision (a generic "你的" would flag every normal sentence).
    let claim_patterns = [
        "you said",
        "you mentioned",
        "you told",
        "you like",
        "you prefer",
        "you have",
        "your ",
        // Chinese: assert a prior statement or a stable preference.
        "你说过",
        "你之前说",
        "你之前提到",
        "你之前提过",
        "你不是说",
        "你告诉过我",
        "你跟我说过",
        "你最喜欢",
        "你最爱的",
        "你一直很喜欢",
        // Deictic-time claim variants (2026-08-14): "你说今天在找实习" asserts a
        // past statement with a relative time word — must be grounded too.
        "你说今天",
        "你说昨天",
        "你说明天",
        "你说你",
    ];
    let lower = response.to_lowercase();

    for pattern in &claim_patterns {
        if let Some(pos) = lower.find(pattern) {
            // Extract a window after the claim pattern. The end offset is in
            // BYTES; +40 into Chinese text can land inside a multi-byte CJK
            // code point, and slicing there would panic — step up to the next
            // char boundary (ceil_char_boundary).
            let target = (pos + pattern.len() + 40).min(response.len());
            let window_end = ceil_char_boundary(response, target);
            let window = &response[pos..window_end];
            let window_lower = window.to_lowercase();

            // Check if any fact value or episode summary overlaps.
            let grounded = fact_values
                .iter()
                .any(|v| window_lower.contains(&v.to_lowercase()))
                || ep_summaries
                    .iter()
                    .any(|s| window_lower.contains(&s.to_lowercase()));

            if !grounded {
                violations.push(format!(
                    "Possible hallucination: '{}' references something not in provided memories",
                    window.trim()
                ));
            }
        }
    }

    if !violations.is_empty() {
        log::warn!(
            "Grounding check found {} potential violations: {:?}",
            violations.len(),
            violations
        );
    }

    violations
}

/// Smallest char-boundary byte offset `>= target`, clamped to the string
/// length. `target` may fall inside a multi-byte (CJK) code point; slicing the
/// string there panics, so advance to the next boundary. Mirrors the
/// nightly `str::ceil_char_boundary` so we stay on stable Rust.
fn ceil_char_boundary(s: &str, target: usize) -> usize {
    if target >= s.len() {
        return s.len();
    }
    let mut i = target;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::onboarding::UserProfile;
    use crate::db::episodes::Episode;
    use crate::db::facts::Fact;
    use crate::db::persona::PersonaTrait;
    use crate::db::relationship::Relationship;
    use crate::mind::retrieval::{RetrievalResult, ScoreBreakdown, ScoredEpisode};

    #[test]
    fn memory_when_buckets_relative_to_now() {
        let now = chrono::Utc::now();
        let iso = |d: i64| (now - chrono::Duration::days(d)).to_rfc3339();
        assert_eq!(memory_when(&iso(0), &now), "今天");
        assert_eq!(memory_when(&iso(1), &now), "昨天");
        assert_eq!(memory_when(&iso(2), &now), "前天");
        let four = memory_when(&iso(4), &now);
        assert!(four.starts_with("4天前（"), "relative + calendar date: {four}");
        // Stale memories lose the noisy day count but keep the calendar date.
        let stale = memory_when(&iso(90), &now);
        assert!(!stale.contains("天前"), "stale is calendar-only: {stale}");
        assert!(stale.contains("月"));
        // Unparseable input degrades to the date part, never panics.
        assert_eq!(memory_when("garbage", &now), "garbage");
    }

    #[test]
    fn memory_lines_carry_salient_relative_date() {
        // The 2026-08-17 incident: a 4-day-old memory must render a visible
        // "4天前", not a buried ISO date the model ignores.
        let now = chrono::Utc::now();
        let mut r = empty_retrieval();
        r.facts = vec![Fact {
            id: "f1".into(), category: "preference".into(), key: "favorite_food".into(),
            value: "火锅".into(), confidence: 0.9,
            valid_from: None, valid_to: None, source_episode: None,
            mention_count: 1,
            created_at: (now - chrono::Duration::days(4)).to_rfc3339(),
            updated_at: (now - chrono::Duration::days(4)).to_rfc3339(),
            surfaced_count: 0, last_surfaced_at: None,
        }];
        let s = format_memories(&r);
        assert!(s.contains("4天前"), "salient relative date present: {s}");
        assert!(s.contains("绝不能说成「昨天」"), "inline time rule present");
    }

    fn empty_retrieval() -> RetrievalResult {
        RetrievalResult {
            episodes: vec![],
            facts: vec![],
            relationship: None,
            relationship_review: None,
           persona_traits: vec![],
           user_profile: UserProfile::default(),
           first_met: None,
       }
   }

   fn retrieval_with_data() -> RetrievalResult {
        RetrievalResult {
            episodes: vec![ScoredEpisode {
                episode: Episode {
                    emotion_anchor: None,
                    id: "ep_1".to_string(),
                    time: "2026-07-10T14:00:00+00:00".to_string(),
                    summary: "user ate hotpot with friends".to_string(),
                    emotion: Some("happy".to_string()),
                    importance: 0.8,
                    is_landmark: false,
                    subject: "user".to_string(),
                    participants: None,
                    topics: None,
                    source_type: "conversation".to_string(),
                    source_conversation_id: None,
                    source_turn: None,
                    memory_strength: 0.7,
                    recall_count: 1,
                    last_recalled_at: None,
                    consolidated: false,
                    created_at: "2026-07-10T14:00:00+00:00".to_string(),
                },
                score: 0.85,
                score_breakdown:                     ScoreBreakdown {
                    semantic: 0.9,
                    strength: 0.7,
                    novelty: 0.0,
                    recency: 0.95,
                    emotion: 1.0,
                },
            }],
            facts: vec![Fact {
                id: "f_1".to_string(),
                category: "preference".to_string(),
                key: "drink".to_string(),
                value: "milk tea".to_string(),
                confidence: 0.9,
                valid_from: Some("2026-07-14".to_string()),
                valid_to: None,
                source_episode: None,
                mention_count: 3,
                created_at: "2026-07-14T10:00:00+00:00".to_string(),
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
                surfaced_count: 0,
                last_surfaced_at: None,
            }],
            relationship: Some(Relationship {
                closeness: 35.0,
                trust: 60.0,
                days_known: 7,
                total_conversations: 20,
                shared_events: 3,
                last_interaction_at: None,
                last_interaction_type: None,
                closeness_log: None,
                updated_at: "2026-07-14T10:00:00+00:00".to_string(),
            }),
            relationship_review: None,
            persona_traits: vec![PersonaTrait {
                id: "t_1".to_string(),
                trait_type: "core".to_string(),
                trait_key: "gentle".to_string(),
                confidence: 0.95,
               source: "design".to_string(),
               created_at: "2026-07-14T10:00:00+00:00".to_string(),
               updated_at: "2026-07-14T10:00:00+00:00".to_string(),
           }],
           user_profile: UserProfile::default(),
           first_met: None,
       }
   }

   #[test]
    fn test_relationship_review_injected() {
        // A populated relationship_review must surface as a [Relationship]
        // block in the trailing memory context (always-on relationship context).
        let mut r = empty_retrieval();
        r.relationship_review = Some("你们最近聊到了实习的事，相处轻松".to_string());
        let prompt = build_trailing_memory_context(&r);
        assert!(
            prompt.contains("[Relationship]\n你们最近聊到了实习的事，相处轻松"),
            "relationship review should be injected as a [Relationship] block"
        );
    }

   #[test]
    fn test_system_prompt_contains_constraint() {
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("[Grounding Constraint]"));
        assert!(prompt.contains("Do not present information as remembered unless"));
    }

    #[test]
    fn test_system_prompt_is_static_prefix() {
        // L2a+ cache discipline: the first system message must contain NO
        // per-turn volatile content — memories, relationship numbers, reviews.
        // Only the template + static persona + grounding guardrail.
        let mut r = retrieval_with_data();
        let prompt = build_system_prompt(&r, &EmotionState::default(), &Intent::default());
        assert!(!prompt.contains("milk tea"), "memories must not be in the static first message");
        assert!(!prompt.contains("hotpot"), "memories must not be in the static first message");
        // NB: system.txt 正文会引用 [Memories]/[Milestones] 标签名，所以这里
        // 不用标签名断言，而用记忆内容（milk tea/hotpot/closeness）。
        assert!(!prompt.contains("closeness"), "static system is relationship-number-free");
        assert!(prompt.contains("gentle"), "static persona trait stays in the head");
        // And the trailing context DOES carry them (build_system_prompt alone
        // must stay static; allocate_and_compress appends the tail).
        let tail = build_trailing_memory_context(&r);
        assert!(tail.contains("milk tea"));
        assert!(tail.contains("hotpot"));
        assert!(tail.contains("closeness"));
    }

    #[test]
    fn test_trailing_context_carries_relationship_and_memories() {
        let retrieval = retrieval_with_data();
        let tail = build_trailing_memory_context(&retrieval);
        assert!(tail.contains("milk tea"));
        assert!(tail.contains("hotpot"));
        assert!(tail.contains("closeness 35"));
        assert!(tail.contains("[Persona]") == false, "persona stays in the static head");
    }

    #[test]
    fn test_system_prompt_contains_chinese_grounding_ban() {
        // The anti-fabrication rule must also reach Chinese generation (the pet
        // replies in Chinese), so the ban is bilingual. Guards against the
        // proactive-bubble hallucination regression (e.g. inventing a project).
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("严禁编造"), "Chinese anti-fabrication ban must appear in system prompt");
    }

    #[test]
    fn test_system_prompt_contains_memories() {
        let retrieval = retrieval_with_data();
        // Static head keeps only the template + persona wording...
        let prompt = build_system_prompt(&retrieval, &EmotionState::default(), &Intent::default());
        assert!(prompt.contains("gentle"), "static template keeps persona wording");
        // ...while the trailing context carries the memory ledger.
        let tail = build_trailing_memory_context(&retrieval);
        assert!(tail.contains("milk tea"));
        assert!(tail.contains("hotpot"));
    }

    #[test]
    fn test_system_prompt_contains_intent() {
        let intent = Intent {
            goal: "comfort".to_string(),
            memory_anchor: "user has exam tomorrow".to_string(),
            tone: "gentle".to_string(),
            proactive: true,
            action: "normal".to_string(),
            capability: crate::tools::CapabilityMode::None,
        };
        // Soul v2 L2a: intent moved to the near-end directive.
        let prompt = build_system_prompt(&empty_retrieval(), &EmotionState::default(), &intent);
        assert!(!prompt.contains("goal:"), "static system is intent-free");
        let near = build_near_end_directive(&EmotionState::default(), &intent);
        assert!(near.contains("goal: comfort"));
        assert!(near.contains("exam tomorrow"));
        assert!(near.contains("proactive"));
    }

    #[test]
    fn tone_hint_permission_phrasing_and_yield() {
        use crate::perception::time::TimeOfDay;
        let mut e = EmotionState::default();
        // stress branch (allowed even in distress — 危机时少说)
        e.stress = 0.7;
        assert!(tone_hint_at(&e, false, TimeOfDay::Morning).unwrap().contains("慢一点"));
        assert!(tone_hint_at(&e, true, TimeOfDay::Morning).unwrap().contains("慢一点"));
        // deep-night sleepiness
        e.stress = 0.3;
        e.rest_need = 0.7;
        assert!(tone_hint_at(&e, false, TimeOfDay::DeepNight).unwrap().contains("困"));
        // loneliness reaching-out: midday OK, distress yields
        e.rest_need = 0.3;
        e.loneliness = 0.7;
        assert!(tone_hint_at(&e, false, TimeOfDay::Afternoon).unwrap().contains("搭句话"));
        assert!(tone_hint_at(&e, true, TimeOfDay::Afternoon).is_none(), "distress yields reaching-out");
        // playful: yields under distress too
        e.loneliness = 0.2;
        e.mood = 0.8;
        assert!(tone_hint_at(&e, false, TimeOfDay::Morning).unwrap().contains("雀跃"));
        assert!(tone_hint_at(&e, true, TimeOfDay::Morning).is_none(), "distress yields playful");
        // neutral: no hint
        e.mood = 0.5;
        assert!(tone_hint_at(&e, false, TimeOfDay::Morning).is_none());
    }

    #[test]
    fn test_system_prompt_injects_current_time() {
        // Phase 6: time is prompt-injected so "几点" is answered without a tool.
        // Soul v2 L2a: time moved to the near-end directive (also keeps the
        // static system prefix cache-friendly — time changed every minute).
        let prompt =
            build_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(!prompt.contains("[Current time]"), "static system is time-free");
        let near = build_near_end_directive(&EmotionState::default(), &Intent::default());
        assert!(near.contains("[Current time]"));
        assert!(near.contains("时段"));
    }

    #[test]
    fn test_qa_prompt_injects_current_time() {
        let prompt =
            build_qa_system_prompt(&empty_retrieval(), &EmotionState::default(), &Intent::default());
        assert!(!prompt.contains("[Current time]"), "QA static system is time-free");
        let near = build_qa_near_end(&EmotionState::default(), &Intent::default());
        assert!(near.contains("[Current time]"));
        assert!(near.contains("直接"), "QA direct-answer directive rides near-end");
    }

    #[test]
    fn test_confidence_labels() {
        assert_eq!(confidence_label(0.9), "high");
        assert_eq!(confidence_label(0.6), "medium");
        assert_eq!(confidence_label(0.3), "low");
    }

    #[test]
    fn test_groundedness_clean_response() {
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("That sounds fun! Hope you had a great time.", &retrieval);
        assert!(violations.is_empty(), "no claims about user memory: {:?}", violations);
    }

    #[test]
    fn test_groundedness_grounded_claim() {
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("You like milk tea right? Want to get some?", &retrieval);
        assert!(violations.is_empty(), "milk tea is in provided memories: {:?}", violations);
    }

    #[test]
    fn test_groundedness_hallucination() {
        let retrieval = retrieval_with_data();
        let violations = check_groundedness(
            "You said you love hiking mountains every weekend!",
            &retrieval,
        );
        assert!(!violations.is_empty(), "hiking is NOT in provided memories");
    }

    #[test]
    fn test_groundedness_chinese_hallucination() {
        // Liri replies in Chinese — the EN-only patterns caught nothing.
        let retrieval = retrieval_with_data();
        let violations =
            check_groundedness("你说过你每个周末都去爬山，对吧？", &retrieval);
        assert!(
            !violations.is_empty(),
            "爬山/hiking is NOT in provided memories"
        );
    }

    #[test]
    fn test_groundedness_chinese_grounded() {
        // A Chinese claim that IS backed by a provided memory must pass.
        let mut retrieval = empty_retrieval();
        retrieval.facts.push(Fact {
            id: "f_cn".to_string(),
            category: "preference".to_string(),
            key: "drink".to_string(),
            value: "奶茶".to_string(),
            confidence: 0.9,
            valid_from: None,
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: "2026-08-08T00:00:00+00:00".to_string(),
            updated_at: "2026-08-08T00:00:00+00:00".to_string(),
            surfaced_count: 0,
            last_surfaced_at: None,
        });
        let violations = check_groundedness("你最喜欢奶茶对吧，给你带了一杯。", &retrieval);
        assert!(
            violations.is_empty(),
            "奶茶 IS in provided memories: {:?}",
            violations
        );
    }

    #[test]
    fn test_groundedness_cjk_window_does_not_panic() {
        // A long Chinese response where the claim sits deep in multi-byte text:
        // the +40-byte window end must round up to a char boundary, not panic.
        let retrieval = empty_retrieval();
        let padding = "今天天气真不错呀".repeat(20);
        let response = format!("{padding}你说过你从小就住在月球上呢。");
        let violations = check_groundedness(&response, &retrieval);
        assert!(!violations.is_empty(), "月球 is not in memories");
    }

    #[test]
    fn test_milestones_split_ledger() {
        // Hermes-inspired relationship ledger: landmark episodes surface in
        // [Milestones] and are NOT repeated under [Memories] — in the
        // trailing memory context.
        let mut retrieval = retrieval_with_data();
        retrieval.episodes[0].episode.is_landmark = true;
        let prompt = build_trailing_memory_context(&retrieval);
        assert!(prompt.contains("[Milestones]"), "landmark must surface in [Milestones]");
        assert!(prompt.contains("hotpot"), "milestone summary present");
        // rfind: system.txt 正文也提到这两个标签，真正的区块在 prompt 末尾。
        let m_start = prompt.rfind("[Milestones]").unwrap();
        let m_end = prompt.rfind("[Memories]").unwrap();
        let after_milestones = &prompt[m_start..m_end];
        assert!(after_milestones.contains("hotpot"), "milestone text must be inside [Milestones] block");
        let after_memories = &prompt[m_end..];
        assert!(!after_memories.contains("hotpot"), "landmark must not repeat under [Memories]");
    }

    #[test]
    fn test_empty_memories_section() {
        let retrieval = empty_retrieval();
        let tail = build_trailing_memory_context(&retrieval);
        assert!(
            !tail.contains("- [Fact]"),
            "should list no facts when empty"
        );
        // Empty memory still renders an explicit marker (not omitted) so the
        // model can't fabricate "你上次说…" threads — see format_memories.
        assert!(
            tail.contains("暂无相关记忆"),
            "empty memory must show the explicit no-fabrication marker"
        );
    }

    #[test]
    fn dead_trust_column_hidden_alive_trust_injected() {
        // Soul v2 plan §3.3: trust is a dead column — "trust 0.0" next to a
        // real closeness is a contradictory relationship signal, hide it.
        // The relationship line rides the trailing context (cache discipline).
        let rel = |trust: f64| Relationship {
            closeness: 60.0,
            trust,
            days_known: 30,
            total_conversations: 10,
            shared_events: 0,
            last_interaction_at: None,
            last_interaction_type: None,
            closeness_log: None,
            updated_at: "t".to_string(),
        };
        let mut r = empty_retrieval();
        r.relationship = Some(rel(0.0));
        let p = build_system_prompt(&r, &EmotionState::default(), &Intent::default());
        assert!(!p.contains("trust"), "dead trust column must stay hidden");
        assert!(p.contains("known 30 days") == false, "relationship numbers must live in the trailing context");
        let tail = build_trailing_memory_context(&r);
        assert!(tail.contains("known 30 days"), "relationship line rides the tail");

        r.relationship = Some(rel(40.0));
        let tail2 = build_trailing_memory_context(&r);
        assert!(tail2.contains("trust 40"), "maintained trust stays injected");
    }

    #[test]
    fn first_met_date_rides_with_days_known() {
        // She mis-derived "7月18号" when asked 我们认识多久了 — the fix pairs
        // the day count with the anchor date so no date arithmetic is needed.
        // The relationship line rides the trailing context (cache discipline).
        let rel = || Relationship {
            closeness: 100.0,
            trust: 0.0,
            days_known: 30,
            total_conversations: 759,
            shared_events: 0,
            last_interaction_at: None,
            last_interaction_type: None,
            closeness_log: None,
            updated_at: "t".to_string(),
        };
        let mut r = empty_retrieval();
        r.relationship = Some(rel());
        r.first_met = Some("2026-07-16".to_string());
        let tail = build_trailing_memory_context(&r);
        assert!(
            tail.contains("known each other since 2026-07-16 (30 days)"),
            "date must ride with the day count, got: {}",
            tail.lines().find(|l| l.contains("Relationship:")).unwrap_or("")
        );
    }

    #[test]
    fn personality_style_wording_is_roots_not_expectation() {
        // Soul v2 plan §3.3: "你被期望的性格" is a casting-mask cue that
        // licenses performing a role; the settings are the seed she grew from.
        let mut r = empty_retrieval();
        r.user_profile.personality_style = Some("温柔，又有点调皮".to_string());
        let p = build_system_prompt(&r, &EmotionState::default(), &Intent::default());
        assert!(p.contains("你性格的底子"), "wording should frame roots, got: {}", p);
        assert!(!p.contains("被期望"));
    }

    #[test]
    fn test_emotion_in_prompt() {
        let emotion = EmotionState {
            mood: 0.8,
            ..EmotionState::default()
        };
        // Soul v2 L2a: mood moved to the near-end directive.
        let prompt = build_system_prompt(&empty_retrieval(), &emotion, &Intent::default());
        assert!(!prompt.contains("[Current Mood]"), "static system is mood-free");
        let near = build_near_end_directive(&emotion, &Intent::default());
        assert!(near.contains("[Current Mood]"));
        assert!(near.contains("开心"));
    }
}

