//! Proactive behavior: decides when the pet should initiate a conversation.
//! Design doc 9.2: bubbles at most every 30 minutes; silent during deep focus.

use crate::db::facts::Fact;
use crate::db::pending::PendingEvent;
use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::emotion::state::EmotionState;
use crate::llm::client::{ChatMessage, LlmClient};
use chrono::{DateTime, Local, Utc};
use rand::Rng;
use serde::Serialize;

/// Rotating retrieval queries for the memory-anchored bubble type. A random
/// one per call avoids always surfacing the single dominant memory topic
/// (user feedback 2026-08-09: "冒泡内容全和糯米有关").
const MEMORY_QUERIES: &[&str] = &[
    "user's life recent events and preferences",
    "what the user mentioned recently about daily life",
    "user's work study plans and hobbies",
    "people pets and relationships in the user's life",
    "user's feelings mood and recent experiences",
];

/// A proactive action the pet wants to take.
#[derive(Debug, Clone, Serialize)]
pub struct ProactiveAction {
    pub event_id: Option<String>,
    pub action_type: String, // "followup" | "random_chat" | "encourage"
    pub message_hint: String,
}

/// Simplified perception state for proactive decisions.
#[derive(Debug, Clone, Default)]
pub struct PerceptionState {
    pub is_deep_focus: bool,
    pub closeness: f64,
}

/// Decides whether the pet should proactively bubble up.
///
/// Rules (priority order):
///   1. Deep focus → None (don't disturb)
///   2. Too soon after last bubble → None (frequency control)
///   3. Closeness < 20 → None (too early in relationship)
///   4. Due event → followup
///   5. High loneliness → random_chat
pub fn trigger_proactive(
    events: &[PendingEvent],
    emotion: &EmotionState,
    perception: &PerceptionState,
    last_bubble_time: &DateTime<Utc>,
    min_interval_secs: i64,
) -> Option<ProactiveAction> {
    // Rule 1: Don't disturb during deep focus.
    if perception.is_deep_focus {
        return None;
    }

    // Rule 2: Frequency control — configurable interval since last bubble.
    // commands.rs feeds the real persisted last-bubble time + the config value;
    // this was hardcoded to now-31min upstream, which always passed and let
    // bubbles fire on every 5-min frontend poll.
    let now = Utc::now();
    let elapsed = (now - *last_bubble_time).num_seconds();
    if elapsed < min_interval_secs {
        return None;
    }

    // Rule 3: Closeness gate — don't proactively bubble to strangers.
    if perception.closeness < 20.0 {
        return None;
    }

    // Rule 4: Due pending event → follow up.
    if let Some(event) = events.first() {
        return Some(ProactiveAction {
            event_id: Some(event.id.clone()),
            action_type: "followup".to_string(),
            message_hint: event.title.clone(),
        });
    }

    // Rule 5: High loneliness → random chat.
    if emotion.loneliness > 0.7 {
        return Some(ProactiveAction {
            event_id: None,
            action_type: "random_chat".to_string(),
            message_hint: String::new(),
        });
    }

    None
}

/// The outcome of a proactive bubble: the voiced reply plus the memory anchor
/// it was grounded on. Exposing the anchor (not just the reply) lets tests
/// verify the reply is actually anchored (proactive-recall standard S1) and,
/// per Principle 11 (Explainability), lets the Debug Panel show *what* she
/// anchored on — not just *that* she spoke.
#[derive(Debug, Clone, Serialize)]
pub struct BubbleOutcome {
    /// The LLM's voiced reply (trimmed, non-empty).
    pub reply: String,
    /// The memory anchor the reply is grounded on — a due pending event title,
    /// an anchorable fact ("key: value"), or a recent episode summary.
    pub anchor: String,
}

/// B-tier runtime grounding guard (plan B1b / Architecture #3 "memory may
/// forget, never fabricate"). The proactive prompts already carry the rule-8
/// "严禁编造" soft constraint (the A-tier fix); this is the runtime backstop.
///
/// Runs AFTER the first non-streaming LLM pass: if `check_groundedness` flags an
/// ungrounded claim about the user, retry once with a correction instruction;
/// if the retry is still fabricated, suppress (return `None`) so the user never
/// sees the hallucination. Cost is bounded (#8): the extra call fires only when
/// a violation is detected (rare).
///
/// The streamed chat path (`converse`) is intentionally NOT guarded here — a
/// hallucinated reply is already token-streamed to the live bubble by the time
/// the full text can be checked, so it can't be cleanly retracted. Its grounding
/// stays warn-only observability (Debug Panel). Proactive bubbles return a full
/// string with no streaming, so the block is clean — this is also where the
/// 07-31 hallucination actually occurred.
pub async fn grounding_guard(
    reply: String,
    retrieval: &crate::mind::retrieval::RetrievalResult,
    messages: &[ChatMessage],
    llm: &LlmClient,
) -> Option<String> {
    if reply.is_empty() {
        return None;
    }
    if crate::mind::grounding::check_groundedness(&reply, retrieval).is_empty() {
        return Some(reply);
    }
    log::warn!("[grounding-B] proactive bubble flagged as ungrounded; retrying once");
    let mut retry = messages.to_vec();
    retry.push(ChatMessage::assistant(reply.clone()));
    retry.push(ChatMessage::system("你上一句话把记忆里没有的事说成了关于 ta 的经历或喜好——这是编造。请重新只说一句：不要编造任何关于 ta 的记忆或偏好，不确定就只表达你此刻的感受，绝不替 ta 编过往。"));
    match llm.chat(&retry, Some(0.8), Some(4096), None).await {
        Ok(r) => {
            let reply2 = r.content.trim().to_string();
            if !reply2.is_empty()
                && crate::mind::grounding::check_groundedness(&reply2, retrieval).is_empty()
            {
                log::info!("[grounding-B] retry produced a clean reply");
                Some(reply2)
            } else {
                log::warn!("[grounding-B] still ungrounded after retry; suppressing bubble");
                None
            }
        }
        Err(e) => {
            log::warn!("[grounding-B] retry LLM error: {:?}; suppressing bubble", e);
            None
        }
    }
}

/// Generates a proactive bubble by picking a memory anchor — a due pending
/// event first, then an anchorable fact, then a recent episode — and running it
/// through the same retrieval + budget + LLM pipeline as a normal turn, with
/// `proactive = true`. Returns `None` when nothing is worth surfacing (the pet
/// stays silent).
///
/// Backend of the `proactive_bubble` command; extracted so the closed-loop-2
/// path ("she brings up your past plan the next day") is testable without
/// constructing AppState / Tauri State.
///
/// Principle 1 (LLM expresses, Rust maintains state): Rust picks the anchor and
/// assembles the prompt; the LLM only voices it.
/// Principle 8 (Cost): at most one LLM call per invocation.
pub async fn generate(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    wm_context: &[ChatMessage],
) -> Result<Option<BubbleOutcome>, String> {
    let now = chrono::Utc::now().to_rfc3339();

    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    let pending_due: Vec<PendingEvent> =
        db.with_conn(|conn| crate::db::pending::get_due(conn, &now))?;

    // 70% lively (anchorless, moment-driven: self-talk / 撒娇 / a passing
    // thought) vs 30% memory-anchored (loop-2 recall). Weighted so bubbles don't
    // default to the single dominant memory topic — lively types voice *this
    // moment*, not a recalled fact (user feedback 2026-08-09: 要像真人突然找你聊天).
    // Pick bubble type + retrieval query up front so the non-Send ThreadRng is
    // dropped before any .await (tauri commands require the future to be Send).
    let (is_lively, query): (bool, &'static str) = {
        let mut rng = rand::thread_rng();
        // A due pending (user-set reminder) is time-sensitive — it must NOT be
        // skipped by a random lively bubble. Roll the lively dice only when
        // nothing is due; when a reminder is due, force the memory branch so
        // pending_due.first() anchors the bubble. The 70/30 lively/memory split
        // is preserved for the no-pending case (diversity untouched). Surfaced
        // by closed-loop-2 harness 2026-08-09.
        let is_lively = pending_due.is_empty() && rng.gen_range(0..100) >= 30;
        let query = MEMORY_QUERIES[rng.gen_range(0..MEMORY_QUERIES.len())];
        (is_lively, query)
    };
    if is_lively {
        return generate_lively(db, llm, wm_context, &emotion).await;
    }

    // Memory-anchored: the rotated query surfaces different memories across
    // calls instead of always the dominant topic.
    let retrieval = crate::mind::retrieval::retrieve(query, &emotion, embedding, db, 8)?;
    // A memory-anchored bubble is genuine recall — strengthen it. ADR 2026-08-09 Part 2.
    crate::mind::retrieval::reinforce_top(db, &retrieval.episodes);

    let (memory_anchor, goal, tone): (String, &'static str, &'static str) =
        if let Some(ev) = pending_due.first() {
            (ev.title.clone(), "care", "gentle")
        } else {
            // Diversity fix (2026-08-13): weighted sampling replaces the old
            // confidence-order argmax (first anchorable fact / first episode).
            // Dominant memories stay more likely, but can no longer win every
            // bubble. rng is scoped to the inner block (sync-only) and dropped
            // before any .await (tauri commands require Send futures).
            let anchor = {
                let mut rng = rand::thread_rng();
                if let Some(f) = sample_anchorable_fact(&retrieval.facts, &mut rng) {
                    Some((format!("{}: {}", f.key, f.value), "accompany", "playful"))
                } else if let Some(i) = crate::mind::retrieval::sample_surface_anchor(
                    &retrieval.episodes,
                    &Utc::now(),
                    &mut rng,
                ) {
                    Some((retrieval.episodes[i].episode.summary.clone(), "accompany", "gentle"))
                } else {
                    None
                }
            };
            match anchor {
                Some((a, g, t)) => (a, g, t),
                None => {
                    // No anchor this turn: fall back to lively rather than staying silent
                    // (user feedback: bubbles should stay lively even without a memory).
                    log::info!("proactive_bubble: no usable memory, falling back to lively");
                    return generate_lively(db, llm, wm_context, &emotion).await;
                }
            }
        };

    let intent = crate::mind::planner::Intent {
        goal: goal.to_string(),
        memory_anchor: memory_anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "proactive_check".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);
    messages.push(ChatMessage::user(format!(
        "（你刚刚突然想起了这件事，想主动跟用户说。你想起来的只有这一件：{}。只能围绕它原意来聊，它是什么就说什么，绝不能换成别的项目、事件或名字，更不能编出记忆里没有的具体事；实在没什么好接的，就说句简单的招呼。按规则回复，尤其规则 8。）",
        memory_anchor
    )));

    log::info!(
        "[proactive] anchor={:?} goal={} facts={} episodes={} msgs={}",
        memory_anchor.chars().take(30).collect::<String>(),
        goal,
        retrieval.facts.len(),
        retrieval.episodes.len(),
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    if let Some(ev) = pending_due.first() {
        let _ = crate::pending::mark_triggered(db, &ev.id);
        let _ = crate::pending::increment_followup(db, &ev.id);
    }
    let _ =
        db.with_conn(|conn| crate::db::relationship::record_interaction(conn, "proactive", &now));

    let reply = chat_result.content.trim().to_string();
    let reply = grounding_guard(reply, &retrieval, &messages, llm).await;
    match reply {
        Some(reply) => Ok(Some(BubbleOutcome {
            reply,
            anchor: memory_anchor,
        })),
        None => Ok(None),
    }
}

/// Generates a lively, anchorless bubble — the 70% path. Voices *this moment*
/// (self-talk / 撒娇 / a passing thought / a small musing) rather than a
/// recalled memory, so she feels like a real person who suddenly wants to chat
/// about anything — not a memory-retrieval machine stuck on one topic. No
/// retrieval call (saves an embedding round-trip); the empty RetrievalResult
/// also lets grounding_guard naturally block any invented claim about the
/// user's past — she may voice her own feelings / the time / her surroundings,
/// but not fabricate "你之前说过的X". Principle 1 (Rust assembles the
/// moment-driven prompt; LLM only voices), Principle 8 (one LLM call).
async fn generate_lively(
    db: &DbState,
    llm: &LlmClient,
    wm_context: &[ChatMessage],
    emotion: &EmotionState,
) -> Result<Option<BubbleOutcome>, String> {
    let retrieval = crate::mind::retrieval::RetrievalResult::default();
    let hour: u32 = Local::now().format("%H").to_string().parse().unwrap_or(12);
    let tone = lively_tone(emotion);

    let intent = crate::mind::planner::Intent {
        goal: "converse".to_string(),
        memory_anchor: String::new(),
        tone: tone.to_string(),
        proactive: true,
        action: "lively_bubble".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, emotion, &intent);
    messages.push(ChatMessage::user(lively_prompt(emotion, hour)));

    log::info!(
        "[lively] hour={} tone={} mood={:.2} loneliness={:.2} msgs={}",
        hour,
        tone,
        emotion.mood,
        emotion.loneliness,
        messages.len(),
    );

    let chat_result = llm
        .chat(&messages, Some(0.9), Some(4096), None)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "proactive", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = grounding_guard(reply, &retrieval, &messages, llm).await;
    Ok(reply.map(|reply| BubbleOutcome {
        reply,
        anchor: String::new(),
    }))
}

/// Lively-bubble tone from the current emotion: high mood → playful, lonely →
/// gentle, otherwise curious (curiosity surfaces fresh, non-repetitive topics).
fn lively_tone(emotion: &EmotionState) -> &'static str {
    if emotion.mood >= 0.7 {
        "playful"
    } else if emotion.loneliness > 0.6 {
        "gentle"
    } else {
        "curious"
    }
}

/// Builds the moment-driven prompt for the lively bubble. `hour` is injected
/// (not read inside) so the time-of-day mapping is a pure, testable function.
/// The prompt forbids fabricating the user's past (rule 8): she voices her
/// *own* moment — feelings, surroundings, time — never "你之前说过的X".
fn lively_prompt(emotion: &EmotionState, hour: u32) -> String {
    // Give the LLM *situation ingredients* (time-of-day + mood) as descriptive
    // hints, NOT ready-made phrases — avoids it lazily copying "快中午了/想你了"
    // into every line (the homogeneity 续⁸'s content check exposed).
    let (time_hint, time_avoid) = match hour {
        5..=10 => ("清晨到上午", "早 / 早上好 / 新的一天"),
        11..=13 => ("中午时分", "快中午了 / 中午 / 该吃饭了"),
        14..=17 => ("下午", "慵懒的下午 / 午后"),
        18..=20 => ("傍晚", "傍晚 / 夕阳 / 一天结束了"),
        21..=22 => ("晚上", "夜色 / 这个点"),
        _ => ("深夜", "这么晚了 / 还不睡 / 夜深了"),
    };
    let mood_hint = if emotion.loneliness > 0.6 {
        "心里莫名有点空，想找个人搭句话"
    } else if emotion.mood >= 0.7 {
        "心情挺轻快"
    } else if emotion.mood >= 0.4 {
        "挺平静，没什么波澜"
    } else {
        "有点闷，提不起劲"
    };
    format!(
        "（此刻大概是{time_hint}，你{mood_hint}。你没有特别的事要跟用户说，也不一定要 ta 回答——就是这一刻脑子里忽然飘过一句话，随口讲出来。\n\n从一个具体的小切入点说起：一个小动作（伸懒腰、打哈欠、拨弄手边的东西）、刚才注意到的一个细节（窗外的声音、屏幕的光、空气的温度）、一个身体感觉（犯困、饿、暖洋洋、肩膀酸）、一个荒唐的小念头、一句没头没尾的自言自语、或者此刻真的有点好奇的一个小问题（ta 这会儿在忙什么、累不累，或一个突然冒出来的小疑问）。要像真人脑子里突然飘过的那一句，不是在打招呼，也不是在表达关心。\n\n别用这些当开头或万能句式：{time_avoid}、「忽然/突然」+「想你/想到你」、「阳光正好/太阳正暖」、「在吗/在干嘛/有事吗」——它们太套路，会让每句都差不多。换个新鲜点的说法。如果你想问句什么，只问一个就好，ta 不回也完全没关系，别追问、别像查岗。\n\n只说 1 句，简短自然。规则 8 严禁编造：只谈你自己此刻的感受、身边的环境、身体，绝不要假装记得用户跟你说过的具体事情或喜好。）"
    )
}

/// Generates a welcome-back bubble when the user returns after being away
/// (>5min, detected via the presence loop). Unlike `generate` — which voices a
/// *recalled* memory ("I just thought of X") — this voices a *return* ("you're
/// back"), so it is a connection moment, not a task follow-up.
///
/// Differences from `generate`:
///   - Does NOT consult pending events. Surfacing "did you drink water?" the
///     instant someone sits down reads as a nag, not a greeting (Principle 10).
///   - The memory anchor is *optional*. A durable fact / recent episode, when
///     retrieved, is offered as a gentle follow-up ("how did that interview
///     go?"); otherwise the bubble is a pure emotional greeting. Unlike
///     `generate`, no anchor → still speak (a welcome is always worth saying).
///   - Never fabricates: the anchor comes only from retrieval (Principle 3).
///
/// `away_secs` scales the prompt ("gone for 2 hours" vs "5 minutes") so the
/// tone fits the absence. Principle 1 (Rust picks anchor + assembles prompt;
/// LLM only voices), Principle 8 (at most one LLM call).
pub async fn generate_welcome_back(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    wm_context: &[ChatMessage],
    away_secs: u64,
) -> Result<Option<BubbleOutcome>, String> {
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = EmotionState {
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
    // A proactive message grounded in retrieved memory is genuine recall.
    crate::mind::retrieval::reinforce_top(db, &retrieval.episodes);

    // Optional anchor: a durable fact, else a recent episode. Weighted sampling
    // (2026-08-13) instead of confidence-order argmax — see generate(). Empty
    // if neither, a welcome back with no anchor is still a valid greeting.
    let (memory_anchor, has_anchor): (String, bool) = {
        let mut rng = rand::thread_rng();
        if let Some(f) = sample_anchorable_fact(&retrieval.facts, &mut rng) {
            (format!("{}: {}", f.key, f.value), true)
        } else if let Some(i) = crate::mind::retrieval::sample_surface_anchor(
            &retrieval.episodes,
            &Utc::now(),
            &mut rng,
        ) {
            (retrieval.episodes[i].episode.summary.clone(), true)
        } else {
            (String::new(), false)
        }
    };

    // Tone tracks mood: a high-mood pet greets playfully, otherwise gentle.
    let tone: &str = if emotion.mood >= 0.65 { "playful" } else { "gentle" };

    let intent = crate::mind::planner::Intent {
        goal: "welcome".to_string(),
        memory_anchor: memory_anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "welcome_back".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);

    let mins = away_secs / 60;
    let absence_phrase = if mins >= 60 {
        format!("{} 个小时", mins / 60)
    } else {
        format!("{} 分钟", mins.max(1))
    };
    let anchor_clause = if has_anchor {
        format!("你想起 ta 之前跟你提过的事：{memory_anchor}。可以顺便轻轻关心一句，但只能围绕这件事的原意，别把它换成别的话题、别编出没提过的项目或细节，别像在完成任务。")
    } else {
        String::new()
    };

    // Surface any internal thought the last reflection left for "next time the
    // user shows up" (Design 7.1 / P13.2: she really thought of it last night,
    // timestamp proves it). Consumed: surface_thoughts marks them surfaced, so
    // this fires once per thought. Folded into the same LLM turn — no extra
    // call (Principle 8). Empty string when nothing is pending.
    let thought_clause = match crate::soul::monologue::surface_thoughts(db) {
        Ok(thoughts) => match thoughts.first() {
            Some(t) => format!(
                "你昨晚等 ta 的时候心里想过：「{}」。招呼里可以自然地带一点点这个念头，像真的惦记过 ta 一样，但别生硬、别像在复述。",
                t.content
            ),
            None => String::new(),
        },
        Err(e) => {
            log::warn!("[welcome_back] surface_thoughts failed: {}", e);
            String::new()
        }
    };
    messages.push(ChatMessage::user(format!(
        "（对方离开了 {absence_phrase}，刚刚回来。你注意到 ta 回来了，想自然地打个招呼。{anchor_clause}{thought_clause}简短自然，1-2 句，像个真的在等 ta 回来的人。称呼对方用「你」，不要用「用户」。按规则回复。）"
    )));

    log::info!(
        "[welcome_back] away_secs={} has_anchor={} has_thought={} tone={} facts={} episodes={} msgs={}",
        away_secs,
        has_anchor,
        !thought_clause.is_empty(),
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
        crate::db::relationship::record_interaction(conn, "welcome_back", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = grounding_guard(reply, &retrieval, &messages, llm).await;
    match reply {
        Some(reply) => Ok(Some(BubbleOutcome {
            reply,
            anchor: memory_anchor,
        })),
        None => Ok(None),
    }
}

/// Generates a loneliness-driven "想你了" bubble. When homeostasis has let
/// loneliness climb (the user has been idle at the desk, not talking) and the
/// relationship is established, she occasionally reaches out — a gentle nudge,
/// not a demand. Unlike `generate` (voices a *recalled* memory) and
/// `generate_welcome_back` (voices a *return*), this voices quiet *longing*:
/// "you're right there but we haven't talked, I just wanted to say hi".
///
/// Like `generate_welcome_back`: the memory anchor is *optional* (no anchor →
/// still speak, a lonely nudge is always worth a soft word) and never
/// fabricated (anchor comes only from retrieval, Principle 3). The loop_runner
/// gates the *emission* (loneliness threshold, closeness, presence, cooldown);
/// this function only voices it once the frontend asks. Principle 1 (Rust picks
/// anchor + assembles prompt; LLM only voices), Principle 8 (one LLM call).
pub async fn generate_lonely_bubble(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    wm_context: &[ChatMessage],
) -> Result<Option<BubbleOutcome>, String> {
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = EmotionState {
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
    // A proactive message grounded in retrieved memory is genuine recall.
    crate::mind::retrieval::reinforce_top(db, &retrieval.episodes);

    // Optional anchor: a durable fact, else a recent episode. Weighted sampling
    // (2026-08-13) instead of confidence-order argmax — see generate(). Empty
    // if neither — a lonely nudge with no anchor is still a valid "just
    // thinking of you".
    let (memory_anchor, has_anchor): (String, bool) = {
        let mut rng = rand::thread_rng();
        if let Some(f) = sample_anchorable_fact(&retrieval.facts, &mut rng) {
            (format!("{}: {}", f.key, f.value), true)
        } else if let Some(i) = crate::mind::retrieval::sample_surface_anchor(
            &retrieval.episodes,
            &Utc::now(),
            &mut rng,
        ) {
            (retrieval.episodes[i].episode.summary.clone(), true)
        } else {
            (String::new(), false)
        }
    };

    // Tone tracks mood: a high-mood pet nudges playfully, otherwise gentle.
    let tone: &str = if emotion.mood >= 0.65 { "playful" } else { "gentle" };

    let intent = crate::mind::planner::Intent {
        goal: "accompany".to_string(),
        memory_anchor: memory_anchor.clone(),
        tone: tone.to_string(),
        proactive: true,
        action: "lonely_nudge".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };

    let mut messages =
        crate::mind::budget::allocate_and_compress(&retrieval, wm_context, &emotion, &intent);

    let anchor_clause = if has_anchor {
        format!("你刚好想起 ta 之前跟你提过的事：{memory_anchor}。可以顺便轻轻带一句，像真的惦记着这件事，但只能围绕它原意，别换成别的话题、别编出没提过的细节。")
    } else {
        String::new()
    };

    messages.push(ChatMessage::user(format!(
        "（你一个人待了一会儿，有点想 ta。ta 就在旁边但没说话，你想轻轻戳一下 ta——不是催 ta 回复，也不是有事要说，就是想让 ta 知道你在。{anchor_clause}只说 1 句，简短、自然、别黏人、别问问题逼 ta 答。按规则回复，尤其规则 8 严禁编造。）"
    )));

    log::info!(
        "[lonely_nudge] loneliness={:.2} has_anchor={} tone={} facts={} episodes={} msgs={}",
        emotion.loneliness,
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
        crate::db::relationship::record_interaction(conn, "lonely_nudge", &now)
    });

    let reply = chat_result.content.trim().to_string();
    let reply = grounding_guard(reply, &retrieval, &messages, llm).await;
    match reply {
        Some(reply) => Ok(Some(BubbleOutcome {
            reply,
            anchor: memory_anchor,
        })),
        None => Ok(None),
    }
}

/// Weighted-random pick among anchorable facts: weight = 1/(1+mention_count).
/// A fact the pet has already voiced many times is de-prioritized, so proactive
/// bubbles explore newer facts instead of always taking the single highest-
/// confidence one (user feedback 2026-08-13: 浮现按置信度排序太死板).
pub fn sample_anchorable_fact<'a>(facts: &'a [Fact], rng: &mut impl rand::Rng) -> Option<&'a Fact> {
    let candidates: Vec<&Fact> = facts.iter().filter(|f| is_anchorable_fact(f)).collect();
    if candidates.is_empty() {
        return None;
    }
    let weights: Vec<f64> = candidates
        .iter()
        .map(|f| 1.0 / (1.0 + f.mention_count as f64))
        .collect();
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return Some(candidates[0]);
    }
    let mut roll = rng.gen::<f64>() * total;
    for (i, f) in candidates.iter().enumerate() {
        roll -= weights[i];
        if roll <= 0.0 {
            return Some(f);
        }
    }
    Some(*candidates.last().unwrap())
}

/// Whether a fact is worth proactively bringing up. Excludes pseudo-facts
/// (questions the user asked, phrased as facts by an over-eager extractor) and
/// requires reasonable confidence. Durable preferences/relationships/goals pass.
fn is_anchorable_fact(f: &Fact) -> bool {
    if f.confidence < 0.7 {
        return false;
    }
    let bad_key_prefixes = ["knowledge_", "belief_", "chemistry_", "geography_"];
    if bad_key_prefixes.iter().any(|p| f.key.starts_with(p)) {
        return false;
    }
    let v = f.value.to_lowercase();
    let bad_value_markers = [
        "user asked",
        "user is asking",
        "curious about user",
        "asking about",
        "does not know",
        "user doesn't know",
        "user is busy",
    ];
    if bad_value_markers.iter().any(|m| v.contains(m)) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pending::PendingEvent;

    fn pending_event(id: &str, title: &str) -> PendingEvent {
        PendingEvent {
            id: id.to_string(),
            title: title.to_string(),
            event_date: "2026-07-15".to_string(),
            remind_date: Some("2026-07-14T08:00:00".to_string()),
            source_episode: None,
            status: "pending".to_string(),
            importance: 0.8,
            followup_count: 0,
            created_at: "2026-07-14T10:00:00".to_string(),
            triggered_at: None,
            resolved_at: None,
        }
    }

    fn calm_emotion() -> EmotionState {
        EmotionState::default()
    }

    fn lonely_emotion() -> EmotionState {
        EmotionState {
            mood: 0.4,
            physical_energy: 0.5,
            social_battery: 0.4,
            stress: 0.3,
            loneliness: 0.75,
            rest_need: 0.2,
        }
    }

    fn close_perception() -> PerceptionState {
        PerceptionState {
            is_deep_focus: false,
            closeness: 35.0,
        }
    }

    #[test]
    fn test_deep_focus_no_bubble() {
        let perception = PerceptionState {
            is_deep_focus: true,
            closeness: 50.0,
        };
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &perception, &last, 1800);
        assert!(result.is_none());
    }

    #[test]
    fn test_too_soon_no_bubble() {
        let last = Utc::now() - chrono::Duration::minutes(10);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &close_perception(), &last, 1800);
        assert!(result.is_none());
    }

    #[test]
    fn test_low_closeness_no_bubble() {
        let perception = PerceptionState {
            is_deep_focus: false,
            closeness: 10.0,
        };
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview")], &calm_emotion(), &perception, &last, 1800);
        assert!(result.is_none());
    }

    #[test]
    fn test_due_event_followup() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[pending_event("pe_1", "interview tomorrow")], &calm_emotion(), &close_perception(), &last, 1800);
        assert!(result.is_some());
        let action = result.unwrap();
        assert_eq!(action.action_type, "followup");
        assert_eq!(action.message_hint, "interview tomorrow");
    }

    #[test]
    fn test_loneliness_random_chat() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[], &lonely_emotion(), &close_perception(), &last, 1800);
        assert!(result.is_some());
        let action = result.unwrap();
        assert_eq!(action.action_type, "random_chat");
    }

    #[test]
    fn test_no_event_no_loneliness_none() {
        let last = Utc::now() - chrono::Duration::hours(1);
        let result = trigger_proactive(&[], &calm_emotion(), &close_perception(), &last, 1800);
        assert!(result.is_none());
    }

    #[test]
    fn test_lively_tone_tracks_emotion() {
        let playful = EmotionState {
            mood: 0.8,
            physical_energy: 0.6,
            social_battery: 0.6,
            stress: 0.2,
            loneliness: 0.3,
            rest_need: 0.2,
        };
        assert_eq!(lively_tone(&playful), "playful");

        let gentle = EmotionState {
            mood: 0.4,
            physical_energy: 0.5,
            social_battery: 0.4,
            stress: 0.3,
            loneliness: 0.75,
            rest_need: 0.2,
        };
        assert_eq!(lively_tone(&gentle), "gentle");

        let curious = EmotionState {
            mood: 0.5,
            physical_energy: 0.5,
            social_battery: 0.5,
            stress: 0.3,
            loneliness: 0.4,
            rest_need: 0.2,
        };
        assert_eq!(lively_tone(&curious), "curious");
    }

    #[test]
    fn test_lively_prompt_time_of_day() {
        let e = calm_emotion();
        assert!(lively_prompt(&e, 9).contains("清晨"));
        assert!(lively_prompt(&e, 12).contains("中午"));
        assert!(lively_prompt(&e, 15).contains("下午"));
        assert!(lively_prompt(&e, 19).contains("傍晚"));
        assert!(lively_prompt(&e, 22).contains("晚上"));
        assert!(lively_prompt(&e, 1).contains("深夜"));
        // Anti-fabrication directive must always be present (Principle 3).
        assert!(lively_prompt(&e, 9).contains("严禁编造"));
    }

    #[test]
    fn test_sample_anchorable_fact_explores_unmentioned() {
        // Weight 1/(1+mention_count): a fact voiced 100 times must almost never
        // win over a never-voiced one, regardless of confidence order.
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let make = |id: &str, key: &str, confidence: f64, mentions: i64| Fact {
            id: id.to_string(),
            category: "preference".to_string(),
            key: key.to_string(),
            value: "value".to_string(),
            confidence,
            valid_from: Some("2026-07-01T00:00:00+00:00".to_string()),
            valid_to: None,
            source_episode: None,
            mention_count: mentions,
            created_at: "2026-07-01T00:00:00+00:00".to_string(),
            updated_at: "2026-07-01T00:00:00+00:00".to_string(),
        };
        // Old faithful: highest confidence, mentioned 100 times.
        let stale = make("f1", "movie", 0.98, 100);
        // Newer memory: slightly lower confidence, never mentioned.
        let fresh = make("f2", "hobby", 0.8, 0);
        let facts = vec![stale.clone(), fresh.clone()];

        let mut fresh_wins = 0usize;
        for seed in 0..100u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(f) = sample_anchorable_fact(&facts, &mut rng) {
                if f.id == "f2" {
                    fresh_wins += 1;
                }
            }
        }
        assert!(
            fresh_wins >= 95,
            "fresh fact should dominate the draw (got {}/100)",
            fresh_wins
        );
    }

    #[test]
    fn test_sample_anchorable_fact_filters_non_anchorable() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let low_conf = Fact {
            id: "f1".to_string(),
            category: "trivia".to_string(),
            key: "knowledge_sun".to_string(),
            value: "the sun rises".to_string(),
            confidence: 0.5,
            valid_from: Some("2026-07-01T00:00:00+00:00".to_string()),
            valid_to: None,
            source_episode: None,
            mention_count: 0,
            created_at: "2026-07-01T00:00:00+00:00".to_string(),
            updated_at: "2026-07-01T00:00:00+00:00".to_string(),
        };
        let mut rng = StdRng::seed_from_u64(3);
        assert!(sample_anchorable_fact(&[low_conf], &mut rng).is_none());
        assert!(sample_anchorable_fact(&[], &mut rng).is_none());
    }

    #[test]
    fn test_min_interval_configurable() {
        // min_interval_secs is a real parameter, not a hardcoded constant: with
        // a 5-min threshold, a 10-min-old last bubble passes the frequency gate.
        let last = Utc::now() - chrono::Duration::minutes(10);
        let result = trigger_proactive(
            &[pending_event("pe_1", "interview")],
            &calm_emotion(),
            &close_perception(),
            &last,
            300,
        );
        assert!(result.is_some());
    }
}
