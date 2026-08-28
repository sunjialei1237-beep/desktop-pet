//! Thought stream (念头流) — her persistent psychological-state pool plus the
//! three-tier decision pipeline (thought-stream plan v3, 2026-08-27).
//!
//! Replaces "6 occasions × hand-written templates" with:
//!
//!   ingest (Rust, zero LLM)  →  hard gate (Rust)  →  motivation score (Rust)
//!   →  silent evaluation (flash)  →  unified renderer (main model, thinking off)
//!
//! Seeds are structured states {stimulus, emotion_tone, relation_hint} — never
//! pre-baked lines; the line is born only at render time, anchored to the REAL
//! current moment (the "今天晚上的你安静得过分" class of bug becomes
//! structurally impossible).
//!
//! Principles: #1 (Rust owns the pool, scores and gates; the LLM only judges
//! context and voices), #8 (at most one flash + one main call per voiced
//! bubble; hard-gated windows cost nothing), #11 (every decision — including
//! "decided not to speak" — is observable in logs), #12 (silence is a result).
//!
//! P1 scope: the poll-driven `proactive_bubble` path. welcome-back / lonely /
//! ritual emitters stay on the legacy generators until the stream proves out
//! (they fold in as high-salience origins in P2).

use crate::config::ProactiveConfig;
use crate::db::thoughts::{self, ThoughtSeed, STATE_PENDING};
use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::pending::proactive::{grounding_guard, log_bubble, BubbleOutcome};
use chrono::{DateTime, Datelike, Local, Timelike, Utc};

/// Hard silence floor: motivation below this never reaches the flash tier
/// (省一次调用；"忍住不说"的硬保证来自系统而非模型随机 — v3 §二层之一).
const SILENCE_FLOOR: f64 = 0.3;

/// Hard quiet hours (Nomi 22:00-08:00 的本地化：晚安说过后到午夜 + 清晨 6 点前).
/// 早安仪式不受此门影响（它走 legacy ritual 路径，不经本模块）。
fn quiet_hours(hour: u32, goodnight_done: bool) -> bool {
    hour < 6 || (hour >= 22 && goodnight_done)
}

/// Backoff: each trailing unacknowledged bubble doubles the effective interval
/// (Kindroid 实证机制；任意用户回应即清零). Capped at 8× so an off day never
/// becomes permanent silence.
fn backoff_multiplier(unacked: usize) -> i64 {
    1i64 << unacked.min(3)
}

/// Ack window: a bubble counts as "responded to" if any conversation turn
/// lands within 30 minutes after it.
const ACK_WINDOW_MINS: i64 = 30;

/// Skip-recent window: no evaluation while the user interacted this recently
/// (LettaBot skipRecentFraction 同思想 — 用户刚说话，她不该"主动"插话).
const SKIP_RECENT_SECS: i64 = 30 * 60;

/// Stale pending seeds expire after this (natural forgetting, pool hygiene).
const SEED_TTL_HOURS: i64 = 48;

/// Cap on the pending pool — ingest stops adding beyond this.
const POOL_CAP: i64 = 12;

// ---------------------------------------------------------------------------
// Ingest (zero LLM)
// ---------------------------------------------------------------------------

/// Synthesizes seeds from what is observable NOW. Pure over its `env_summary`
/// argument (the process-global environment ring is read by the caller) so the
/// logic is unit-testable. Dedup: identical pending stimuli never pile up.
pub fn ingest(db: &DbState, env_summary: Option<&str>, now: &DateTime<Utc>) {
    // 1. Natural forgetting.
    let horizon = (*now - chrono::Duration::hours(SEED_TTL_HOURS)).to_rfc3339();
    if let Ok(n) = db.with_conn(|conn| thoughts::expire_stale(conn, &horizon)) {
        if n > 0 {
            log::info!("[stream] expired {} stale seeds", n);
        }
    }

    let (mood_label, loneliness) = db
        .with_conn(|conn| {
            let e = crate::db::emotion::get(conn)?;
            Ok::<_, String>((e.mood_label, e.loneliness))
        })
        .unwrap_or_else(|_| ("平静".to_string(), 0.0));

    let try_insert = |stimulus: String, origin: &str, salience: f64, relation: Option<&str>| {
        if let Ok(n) = db.with_conn(thoughts::count_pending) {
            if n >= POOL_CAP {
                return;
            }
        }
        let exists = db
            .with_conn(|conn| thoughts::pending_stimulus_exists(conn, &stimulus))
            .unwrap_or(false);
        if exists {
            return;
        }
        let seed = ThoughtSeed {
            id: format!("ts_{}", uuid::Uuid::new_v4()),
            stimulus,
            emotion_tone: Some(mood_label.clone()),
            relation_hint: relation.map(|s| s.to_string()),
            origin: origin.to_string(),
            salience,
            created_at: now.to_rfc3339(),
            state: STATE_PENDING.to_string(),
            unspoken_reason: None,
            voiced_at: None,
            evolved_from: None,
        };
        if let Err(e) = db.with_conn(|conn| thoughts::insert(conn, &seed)) {
            log::warn!("[stream] seed insert failed: {}", e);
        }
    };

    // 2. Environment noticing — she has eyes; the ring summary is already
    //    sanitized by the environment module (untrusted-data pipeline).
    if let Some(summary) = env_summary {
        if !summary.trim().is_empty() {
            try_insert(summary.to_string(), "environment", 0.5, None);
        }
    }

    // 3. Time-of-day boundary — stored per tod so each crossing seeds once.
    let tod = match crate::perception::time::current_time_of_day() {
        crate::perception::time::TimeOfDay::Morning => "上午",
        crate::perception::time::TimeOfDay::Afternoon => "下午",
        crate::perception::time::TimeOfDay::Evening => "傍晚",
        crate::perception::time::TimeOfDay::LateNight => "深夜",
        crate::perception::time::TimeOfDay::DeepNight => "凌晨",
    };
    let tod_changed = db
        .with_conn(|conn| {
            let prev = crate::db::onboarding::get(conn, "stream_last_tod")?;
            crate::db::onboarding::save(conn, "stream_last_tod", tod)?;
            Ok::<_, String>(prev)
        })
        .ok()
        .flatten()
        .map(|p| p != tod)
        .unwrap_or(false);
    if tod_changed {
        try_insert(
            format!("时间从上一个时段走到了{}", tod),
            "self_state",
            0.3,
            None,
        );
    }

    // 4. Long silence while the user is present — "很久没说话" itself is a state.
    let last_interaction = db
        .with_conn(|conn| Ok(crate::db::relationship::get(conn)?.last_interaction_at))
        .ok()
        .flatten();
    if crate::perception::presence::current_presence() == crate::perception::presence::PresenceState::Active
    {
        if let Some(last) = last_interaction.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
            let silent_hours = now.signed_duration_since(last.with_timezone(&Utc)).num_hours();
            if silent_hours >= 2 {
                try_insert(
                    format!("和 ta 已经 {} 个小时没说话了", silent_hours),
                    "self_state",
                    0.55,
                    Some("想被陪着，但不催促"),
                );
            }
        }
    }

    // 5. Loneliness crossing — the old lonely-nudge emotion, now a seed like
    //    any other (it competes in the same scoring instead of owning a lane).
    if loneliness > 0.6 {
        try_insert(
            "心里有点空，想找个人搭句话".to_string(),
            "body",
            0.7,
            Some("想被陪着"),
        );
    }

    // 6. Time trigger MVP (v3 P2, Zep-lite): a due pending event ("你答应过
    //    的日子到了") becomes a high-salience seed instead of owning a lane.
    let now_str = now.to_rfc3339();
    let due: Vec<String> = db
        .with_conn(|conn| {
            crate::db::pending::get_due(conn, &now_str)
                .map(|evs| evs.into_iter().map(|ev| ev.title).collect())
        })
        .unwrap_or_default();
    for title in due {
        try_insert(
            format!("到时间的事：{}", title),
            "pending",
            0.9,
            Some("ta 之前提过的事，到日子了"),
        );
    }

    // 7. Unspoken reinforcement (v3 P2): overnight unspoken seeds return to
    //    pending with a salience bump — "昨天忍住没说的事今天轻轻带出".
    //    Occasion seeds (welcome/lonely/ritual) are one-shot: reviving
    //    "ta 刚回来" a day later would be exactly the time-mismatch class
    //    this whole redesign exists to kill (review catch ①).
    let overnight = (*now - chrono::Duration::hours(6)).to_rfc3339();
    let _ = db.with_conn(|conn| {
        conn.execute(
            "UPDATE thought_stream SET state = 'pending',
                    salience = MIN(salience + 0.15, 0.9)
             WHERE state = 'unspoken' AND created_at < ?1
               AND origin NOT IN ('welcome', 'lonely', 'goodmorning', 'goodnight')",
            rusqlite::params![overnight],
        )
        .map_err(|e| e.to_string())
    });
}

/// Observability snapshot: recent seeds of any state, newest first (the
/// monitoring surface for "她此刻在心里过了些什么" — Debug Panel + harness).
pub fn snapshot(db: &DbState, limit: usize) -> Vec<ThoughtSeed> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, stimulus, emotion_tone, relation_hint, origin, salience, created_at, state, unspoken_reason, voiced_at, evolved_from
                 FROM thought_stream ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| format!("prepare snapshot: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(ThoughtSeed {
                    id: row.get(0)?,
                    stimulus: row.get(1)?,
                    emotion_tone: row.get(2)?,
                    relation_hint: row.get(3)?,
                    origin: row.get(4)?,
                    salience: row.get(5)?,
                    created_at: row.get(6)?,
                    state: row.get(7)?,
                    unspoken_reason: row.get(8)?,
                    voiced_at: row.get(9)?,
                    evolved_from: row.get(10)?,
                })
            })
            .map_err(|e| format!("query snapshot: {}", e))?;
        Ok::<_, String>(rows.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tier 1 — hard gate (Rust, zero LLM)
// ---------------------------------------------------------------------------

/// Why the gate stayed closed this window (logged, #11).
#[derive(Debug)]
pub enum GateVerdict {
    Proceed,
    Silent(&'static str),
}

/// Trailing bubbles with no conversation turn within the ack window — the
/// backoff input (Kindroid: "back-to-back messages decrease in frequency if
/// unacknowledged").
fn unacked_bubbles(db: &DbState) -> usize {
    let bubbles = db
        .with_conn(|conn| crate::db::bubble_log::get_recent(conn, 5))
        .unwrap_or_default();
    let mut unacked = 0usize;
    for b in &bubbles {
        let Ok(bt) = DateTime::parse_from_rfc3339(&b.time) else { continue };
        let until = (bt + chrono::Duration::minutes(ACK_WINDOW_MINS)).to_rfc3339();
        let acked: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM conversations WHERE created_at > ?1 AND created_at <= ?2",
                    rusqlite::params![b.time, until],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())
            })
            .unwrap_or(0);
        if acked > 0 {
            break; // the trailing unacked run ends at the first responded bubble
        }
        unacked += 1;
    }
    unacked
}

/// The hard gate. Cheap non-consuming checks first; the budget (the only
/// state-writing check) runs LAST so an impossible window never burns it.
pub fn gate(db: &DbState, cfg: &ProactiveConfig, now: &DateTime<Utc>) -> GateVerdict {
    use crate::perception::presence::PresenceState;

    // Quiet hours.
    let hour = Local::now().hour();
    let goodnight_done = db
        .with_conn(|conn| Ok(crate::soul::ritual::goodnight_done_today(conn)))
        .unwrap_or(false);
    if quiet_hours(hour, goodnight_done) {
        return GateVerdict::Silent("quiet_hours");
    }

    // Not at the desk.
    if crate::perception::presence::current_presence() != PresenceState::Active {
        return GateVerdict::Silent("not_present");
    }

    // Deep focus.
    if crate::perception::focus::is_deep_focus() {
        return GateVerdict::Silent("deep_focus");
    }

    // skipRecent: the user just interacted — proactive would be an interruption.
    let last_interaction = db
        .with_conn(|conn| Ok(crate::db::relationship::get(conn)?.last_interaction_at))
        .ok()
        .flatten();
    if let Some(last) = last_interaction.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
        if now.signed_duration_since(last.with_timezone(&Utc)).num_seconds() < SKIP_RECENT_SECS {
            return GateVerdict::Silent("skip_recent");
        }
    }

    // Budget with backoff-scaled interval × rhythm jitter (去节拍器：0.75×-1.5×
    // 均匀抖动，让到达时刻不可预测 — v3 P2).
    let jitter = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        0.75 + rng.gen::<f64>() * 0.75
    };
    let effective = (cfg.min_interval_secs as f64
        * backoff_multiplier(unacked_bubbles(db)) as f64
        * jitter) as i64;
    if !crate::pending::budget::try_occupy_budget(db, effective, *now) {
        return GateVerdict::Silent("budget");
    }

    GateVerdict::Proceed
}

// ---------------------------------------------------------------------------
// Tier 1.5 — motivation score (Rust, pure)
// ---------------------------------------------------------------------------

/// Named-component motivation (v3 §二层之一). Every input is Rust-computable;
/// the breakdown is logged so the Debug Panel can show WHY this seed, why now.
pub struct ScoreCtx {
    pub loneliness: f64,
    pub closeness: f64,
    /// Hours since her last bubble (silence grows the urge, Inner Thoughts).
    pub silence_hours: f64,
    /// Hours since this seed was born (freshness decay λ=0.95/h on interest).
    pub seed_age_hours: f64,
    pub deep_focus: bool,
    /// Seconds since the user last interacted (annoyance when small).
    pub since_interaction_secs: i64,
    /// Max cosine similarity vs her recent bubbles (semantic repetition). 0.0
    /// when embeddings are unavailable.
    pub max_sim: f64,
}

pub fn motivation(seed_salience: f64, has_relation: bool, ctx: &ScoreCtx) -> f64 {
    let interest = seed_salience * 0.95f64.powf(ctx.seed_age_hours);
    let emotional_need = ctx.loneliness.clamp(0.0, 1.0);
    let relationship_value = (ctx.closeness / 100.0).clamp(0.0, 1.0) * if has_relation { 1.0 } else { 0.6 };
    let timing_bonus = (ctx.silence_hours / 4.0).min(1.0) * 0.6 + 0.1;
    let annoyance_cost = if ctx.deep_focus {
        0.5
    } else if ctx.since_interaction_secs < 300 {
        0.3
    } else {
        0.0
    };
    let recent_contact_penalty = ctx.max_sim * 0.8;
    (interest * 0.35 + emotional_need * 0.25 + relationship_value * 0.15 + timing_bonus * 0.25
        - annoyance_cost
        - recent_contact_penalty)
        .clamp(0.0, 1.0)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Max similarity of a stimulus against her recent bubbles (semantic dedup).
/// Zero-cost skip when the embedding service is unavailable.
fn max_similarity(
    embedding: Option<&EmbeddingService>,
    stimulus: &str,
    recent_texts: &[String],
) -> f64 {
    let Some(embedding) = embedding else { return 0.0 };
    let Ok(v) = embedding.embed(stimulus) else { return 0.0 };
    recent_texts
        .iter()
        .filter_map(|t| embedding.embed(t).ok())
        .map(|w| cosine(&v, &w) as f64)
        .fold(0.0_f64, f64::max)
}

// ---------------------------------------------------------------------------
// Tier 2 — flash silent evaluation
// ---------------------------------------------------------------------------

const EXPRESSION_TYPES: [&str; 5] = ["自言自语", "观察", "回应环境", "关心", "提问"];

#[derive(Debug, Clone)]
pub struct Evaluation {
    pub speak: bool,
    pub pick: String,
    pub expression_type: String,
    pub intent: String,
    pub hook: String,
    pub reason: String,
}

fn now_local_display() -> String {
    let local = Local::now();
    let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [local.weekday().num_days_from_monday() as usize];
    format!(
        "{}（{}）{}",
        local.format("%Y-%m-%d"),
        weekday,
        local.format("%H:%M")
    )
}

fn evaluate_messages(
    candidates: &[&ThoughtSeed],
    recent: &[String],
    now_local: &str,
    occasion_clause: &str,
) -> Vec<ChatMessage> {
    let mut user = format!("现在是 {now_local}。\n\n［她最近主动说过的话（新→旧）］\n");
    if recent.is_empty() {
        user.push_str("（还没有。）\n");
    } else {
        for t in recent {
            user.push_str(&format!("- 「{t}」\n"));
        }
    }
    user.push_str("\n［她心里的候选念头］\n");
    for (i, s) in candidates.iter().enumerate() {
        let relation = s.relation_hint.as_deref().unwrap_or("无");
        let tone = s.emotion_tone.as_deref().unwrap_or("平常");
        user.push_str(&format!(
            "- [S{}] 来源 {}｜状态：{}（感觉：{}；对她的意义：{}）\n",
            i + 1,
            s.origin,
            s.stimulus,
            tone,
            relation
        ));
    }
    user.push_str(&format!("\n{occasion_clause}\n只输出一个 JSON 对象，格式："));
    let contract = r#"{"speak": <true/false>, "pick": "<S1 这样的 id，不选则 S1>", "expression_type": "<自言自语|观察|回应环境|关心|提问>", "intent": "<一句话：她想达到什么>", "hook": "<一句话：从什么切口说起>", "reason": "<一句话：为什么说/为什么忍住>"}"#;
    vec![
        ChatMessage::system(format!(
            "你是「璃」的开口评估器。她安静、不黏人；她在考虑要不要把心里的一个念头说出来。你来判断：此刻说出来，ta 听到会觉得自己被轻轻陪到了，还是被打扰了？\n判断标准：\n- 刚聊过没多久、ta 在专注、念头和她最近说过的话太像 → 不说。speak=false 是正常判断，不是失败；她大部分时候选择不说。
- ［她最近主动说过的话］是硬边界：这条开口不得重复它们的内容、不得沿用它们的开头（比如她刚说过「你回来了」，这次就绝不能再以「你回来了」起头）；做不到避开就 speak=false。\n- 值得说的：此刻真的在她心里的事——ta 正在做的、惦记着的、时间到了的。\n- expression_type 从这五类里选：自言自语（说给自己听的）、观察（注意到的一个具体事实，不带凝视感）、回应环境（对身边此刻的轻反应）、关心（指向 ta 具体事的一句温度）、提问（真的好奇才用）。\n只输出 JSON，不要任何其他文字。格式：{contract}"
        )),
        ChatMessage::user(user),
    ]
}

fn parse_evaluation(raw: &str, n_candidates: usize) -> Option<Evaluation> {
    #[derive(serde::Deserialize)]
    struct Raw {
        speak: Option<bool>,
        pick: Option<String>,
        expression_type: Option<String>,
        intent: Option<String>,
        hook: Option<String>,
        reason: Option<String>,
    }
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    let parsed: Raw = serde_json::from_str(&trimmed[start..=end]).ok()?;
    let speak = parsed.speak.unwrap_or(false);
    let pick = parsed
        .pick
        .and_then(|p| {
            let digits: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse::<usize>().ok().filter(|n| (1..=n_candidates).contains(n))
        })
        .unwrap_or(1);
    let expression_type = parsed
        .expression_type
        .filter(|t| EXPRESSION_TYPES.contains(&t.as_str()))
        .unwrap_or_else(|| "自言自语".to_string());
    let reason = parsed.reason.unwrap_or_default().trim().to_string();
    if !speak && reason.is_empty() {
        return None; // declining without a reason is not a decision
    }
    Some(Evaluation {
        speak,
        pick: format!("S{}", pick),
        expression_type,
        intent: parsed.intent.unwrap_or_default().trim().to_string(),
        hook: parsed.hook.unwrap_or_default().trim().to_string(),
        reason,
    })
}

fn occasion_label(occasion: &str) -> Option<&'static str> {
    match occasion {
        "welcome" => Some("ta 离开了一会儿，刚刚回来"),
        "lonely" => Some("一个人待了一会儿，有点想 ta"),
        "goodmorning" => Some("今天第一次见到 ta"),
        "goodnight" => Some("这一天要结束了"),
        _ => None,
    }
}

/// The flash pass. Err (call failure / unparseable after retries) → the window
/// stays silent (budget already consumed — 宁少勿突兀, matching legacy
/// semantics; a malformed judge must never force-speak). `occasion` marks the
/// emitters that already passed their own gates (welcome/lonely/ritual): the
/// line is happening, evaluation steers HOW not WHETHER.
async fn evaluate(
    llm: &LlmClient,
    candidates: &[&ThoughtSeed],
    recent: &[String],
    occasion: Option<&str>,
) -> Result<Evaluation, String> {
    let occasion_clause = occasion
        .and_then(occasion_label)
        .map(|l| format!("【场合】{l}——这一句几乎一定要说，你评估的重点是\"怎么说才不生硬、不像在完成任务\"，speak 一般为 true。"))
        .unwrap_or_default();
    let messages = evaluate_messages(candidates, recent, &now_local_display(), &occasion_clause);
    for attempt in 1..=2 {
        let result = llm
            .chat_gate(&messages, Some(0.2), Some(2048))
            .await
            .map_err(|e| format!("stream evaluate LLM failed: {:?}", e))?;
        if let Some(ev) = parse_evaluation(&result.content, candidates.len()) {
            return Ok(ev);
        }
        log::warn!("[stream] unparseable evaluation (attempt {}): {:?}", attempt, result.content);
    }
    Err("stream evaluation produced no valid JSON after 2 attempts".to_string())
}

// ---------------------------------------------------------------------------
// Tier 3 — unified renderer (main model, thinking off)
// ---------------------------------------------------------------------------

fn expression_guidance(t: &str) -> &'static str {
    match t {
        "观察" => "说出你注意到的一个具体事实，只复述事实本身，不渲染你一直在看着的感觉",
        "回应环境" => "对身边此刻正在发生的事一个轻轻的反应，像顺手接了一句",
        "关心" => "一句带温度的话，指向那件具体的事，不追问、不展开",
        "提问" => "真的好奇才问，就一个问句，问完就停",
        _ => "自言自语——不对任何人说的话，说给自己听的那种",
    }
}

async fn voice(
    db: &DbState,
    llm: &LlmClient,
    seed: &ThoughtSeed,
    ev: &Evaluation,
    occasion: Option<&str>,
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
        action: "thought_stream".to_string(),
        capability: crate::tools::CapabilityMode::None,
    };
    let mut messages = crate::mind::budget::allocate_and_compress(&retrieval, &[], &emotion, &intent);

    let recent: Vec<String> = db
        .with_conn(|conn| crate::db::bubble_log::get_recent(conn, 2))
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.text.chars().take(40).collect())
        .collect();
    let anti_repeat = if recent.is_empty() {
        String::new()
    } else {
        format!(
            "你自己最近主动说过：{}——这句不得重复它们的内容，也不得以其中任何一句的开头起头（换个完全不同的说法）。",
            recent.join("；")
        )
    };
    let tone = seed.emotion_tone.as_deref().unwrap_or("平常");
    let relation = seed.relation_hint.as_deref().unwrap_or("没什么特别");
    let now = Utc::now();
    let seed_age_hours = now
        .signed_duration_since(
            DateTime::parse_from_rfc3339(&seed.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or(now),
        )
        .num_hours();
    let age_clause = if seed_age_hours >= 1 {
        format!("（这个念头是 {} 小时前冒出来的，此刻还在你心里。）", seed_age_hours)
    } else {
        String::new()
    };

    let occasion_clause = occasion
        .and_then(occasion_label)
        .map(|l| format!("这是{l}的时刻。"))
        .unwrap_or_default();
    messages.push(ChatMessage::user(format!(
        "（现在是{}。{occasion_clause}你心里有个念头正要冒出来——它的由来：{}；你当时的感觉：{}；它对你的意义：{}。如果念头或场合里的时间描述与上面的真实时间矛盾，一律以真实时间为准，绝不顺着旧描述说错时间。{age_clause}你想说的方式——{}；切入：{}；你想达到：{}。只说 1 句，口语、自然、像随手发的一条消息，不要报时间出处。{anti_repeat}规则 8 严禁编造：只围绕这个念头此刻的事实，绝不虚构 ta 跟你说过的具体事。）",
        now_local_display(),
        seed.stimulus,
        tone,
        relation,
        expression_guidance(&ev.expression_type),
        if ev.hook.is_empty() { "自然起头" } else { &ev.hook },
        if ev.intent.is_empty() { "随口陪一句" } else { &ev.intent },
    )));

    let chat_result = llm
        .chat(&messages, Some(0.8), Some(4096), None)
        .await
        .map_err(|e| format!("stream voice LLM error: {:?}", e))?;
    let reply = chat_result.content.trim().to_string();
    Ok(grounding_guard(reply, &retrieval, &messages, llm).await)
}

// ---------------------------------------------------------------------------
// Occasion path (v3 P1b) — the legacy emitters folded into the stream
// ---------------------------------------------------------------------------

/// Welcome-back / lonely-nudge / 早安晚安 ritual emitters fold into the
/// stream as high-salience occasion seeds. The emitter has ALREADY passed its
/// own gates and consumed the shared budget (loop_runner), so this skips the
/// hard gate and goes straight to evaluate → voice with an occasion label.
/// Decline → Ok(None); the caller falls back to its canned line (Principle 8).
pub async fn occasion_bubble(
    db: &DbState,
    llm: &LlmClient,
    occasion: &str,
    stimulus: &str,
) -> Result<Option<BubbleOutcome>, String> {
    let now = Utc::now();
    let mood_label = db
        .with_conn(|conn| Ok(crate::db::emotion::get(conn)?.mood_label))
        .unwrap_or_else(|_| "平静".to_string());
    let relation = match occasion {
        "welcome" => Some("等过 ta，ta 回来了"),
        "lonely" => Some("想被陪着，但不催促"),
        "goodmorning" | "goodnight" => Some("惦记 ta 的节奏"),
        _ => None,
    };
    let seed = ThoughtSeed {
        id: format!("ts_{}", uuid::Uuid::new_v4()),
        stimulus: stimulus.to_string(),
        emotion_tone: Some(mood_label),
        relation_hint: relation.map(|s| s.to_string()),
        origin: occasion.to_string(),
        salience: 0.9,
        created_at: now.to_rfc3339(),
        state: STATE_PENDING.to_string(),
        unspoken_reason: None,
        voiced_at: None,
        evolved_from: None,
    };
    db.with_conn(|conn| thoughts::insert(conn, &seed))?;

    let recent: Vec<String> = db
        .with_conn(|conn| crate::db::bubble_log::get_recent(conn, 3))
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.text.chars().take(40).collect())
        .collect();
    let candidates = vec![&seed];
    let ev = match evaluate(llm, &candidates, &recent, Some(occasion)).await {
        Ok(ev) => ev,
        Err(e) => {
            log::warn!("[stream:{}] evaluate failed ({})", occasion, e);
            return Ok(None);
        }
    };
    if !ev.speak {
        log::info!("[stream:{}] evaluated decline: {}", occasion, ev.reason);
        db.with_conn(|conn| thoughts::mark_unspoken(conn, &seed.id, &ev.reason))?;
        return Ok(None);
    }
    let reply = match voice(db, llm, &seed, &ev, Some(occasion)).await? {
        Some(r) => r,
        None => {
            log::info!("[stream:{}] voice suppressed by grounding guard", occasion);
            db.with_conn(|conn| thoughts::mark_unspoken(conn, &seed.id, "渲染未过grounding"))?;
            return Ok(None);
        }
    };
    db.with_conn(|conn| thoughts::mark_voiced(conn, &seed.id, &now.to_rfc3339()))?;
    log_bubble(db, "thought_stream", &reply, stimulus, Some(&ev.reason));
    Ok(Some(BubbleOutcome {
        reply,
        anchor: stimulus.to_string(),
        anchor_reason: Some(ev.reason),
    }))
}

// ---------------------------------------------------------------------------
// The tick — one pass of the whole pipeline
// ---------------------------------------------------------------------------

/// One pipeline pass, backend of `proactive_bubble` when
/// `[proactive] engine = "stream"`. Returns None whenever any tier chooses
/// silence (gate / score floor / evaluation decline) — silence is a result.
pub async fn tick(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    cfg: &ProactiveConfig,
) -> Result<Option<BubbleOutcome>, String> {
    let now = Utc::now();
    let env_summary = crate::perception::environment::recent_summary();
    ingest(db, env_summary.as_deref(), &now);

    // Pool check BEFORE the gate: an empty pool must not burn the budget
    // window (review catch ② — gate's budget check is the only state-writing
    // tier; skipping it on emptiness keeps the next real seed on schedule).
    let candidates = db
        .with_conn(|conn| thoughts::get_pending(conn, 5))
        .unwrap_or_default();
    if candidates.is_empty() {
        log::info!("[stream] no pending seeds — silent");
        return Ok(None);
    }

    match gate(db, cfg, &now) {
        GateVerdict::Silent(reason) => {
            log::info!("[stream] gate silent: {}", reason);
            return Ok(None);
        }
        GateVerdict::Proceed => {}
    }

    let (loneliness, closeness, last_interaction) = db
        .with_conn(|conn| {
            let e = crate::db::emotion::get(conn)?;
            let closeness = crate::db::relationship::get(conn)
                .map(|r| r.closeness)
                .unwrap_or(0.0);
            let last = crate::db::relationship::get(conn)?.last_interaction_at;
            Ok::<_, String>((e.loneliness, closeness, last))
        })
        .unwrap_or((0.0, 0.0, None));
    let last_bubble = crate::pending::budget::read_last_bubble(db);
    let silence_hours = last_bubble
        .map(|t| now.signed_duration_since(t).num_hours().max(0) as f64)
        .unwrap_or(24.0);
    let since_interaction_secs = last_interaction
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| now.signed_duration_since(d.with_timezone(&Utc)).num_seconds().max(0))
        .unwrap_or(i64::MAX);
    let recent_texts: Vec<String> = db
        .with_conn(|conn| crate::db::bubble_log::get_recent(conn, 5))
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.text)
        .collect();

    let mut scored: Vec<(f64, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ctx = ScoreCtx {
                loneliness,
                closeness,
                silence_hours,
                seed_age_hours: now
                    .signed_duration_since(
                        DateTime::parse_from_rfc3339(&s.created_at)
                            .map(|d| d.with_timezone(&Utc))
                            .unwrap_or(now),
                    )
                    .num_hours()
                    .max(0) as f64,
                deep_focus: crate::perception::focus::is_deep_focus(),
                since_interaction_secs,
                max_sim: max_similarity(embedding, &s.stimulus, &recent_texts),
            };
            (motivation(s.salience, s.relation_hint.is_some(), &ctx), i)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let (best_score, best_i) = scored[0];
    log::info!(
        "[stream] best seed score={:.2} stimulus={:?}",
        best_score,
        candidates[best_i].stimulus.chars().take(30).collect::<String>()
    );
    if best_score < SILENCE_FLOOR {
        log::info!("[stream] below silence floor — she has nothing urgent to say");
        return Ok(None);
    }

    // Top-3 to the flash judge.
    let top: Vec<&ThoughtSeed> = scored.iter().take(3).map(|(_, i)| &candidates[*i]).collect();
    let ev = match evaluate(llm, &top, &recent_texts, None).await {
        Ok(ev) => ev,
        Err(e) => {
            log::warn!("[stream] evaluate failed ({}); window silent", e);
            return Ok(None);
        }
    };
    let picked_index = ev
        .pick
        .trim_start_matches('S')
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .filter(|i| *i < top.len())
        .unwrap_or(0);
    let seed = top[picked_index];

    if !ev.speak {
        log::info!("[stream] evaluated decline: {}", ev.reason);
        db.with_conn(|conn| thoughts::mark_unspoken(conn, &seed.id, &ev.reason))?;
        return Ok(None);
    }

    let reply = match voice(db, llm, seed, &ev, None).await? {
        Some(r) => r,
        None => {
            log::info!("[stream] voice suppressed by grounding guard");
            db.with_conn(|conn| thoughts::mark_unspoken(conn, &seed.id, "渲染未过grounding"))?;
            return Ok(None);
        }
    };
    db.with_conn(|conn| thoughts::mark_voiced(conn, &seed.id, &Utc::now().to_rfc3339()))?;
    log_bubble(db, "thought_stream", &reply, &seed.stimulus, Some(&ev.reason));
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "proactive", &Utc::now().to_rfc3339())
    });
    Ok(Some(BubbleOutcome {
        reply,
        anchor: seed.stimulus.clone(),
        anchor_reason: Some(ev.reason),
    }))
}

// ---------------------------------------------------------------------------
// Tests (pure layers)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ScoreCtx {
        ScoreCtx {
            loneliness: 0.3,
            closeness: 60.0,
            silence_hours: 2.0,
            seed_age_hours: 0.5,
            deep_focus: false,
            since_interaction_secs: 3600,
            max_sim: 0.0,
        }
    }

    #[test]
    fn quiet_hours_match_nomi_window() {
        assert!(quiet_hours(3, false), "清晨 6 点前硬静默");
        assert!(quiet_hours(23, true), "晚安说过后夜里静默");
        assert!(!quiet_hours(23, false), "没说过晚安的 23 点不由此门拦");
        assert!(!quiet_hours(14, false));
        assert!(!quiet_hours(9, true));
    }

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff_multiplier(0), 1);
        assert_eq!(backoff_multiplier(1), 2);
        assert_eq!(backoff_multiplier(2), 4);
        assert_eq!(backoff_multiplier(3), 8);
        assert_eq!(backoff_multiplier(9), 8, "封顶 8×，不无限退");
    }

    #[test]
    fn motivation_grows_with_silence_and_tanks_on_repetition() {
        let base = motivation(0.7, true, &ctx());
        let mut quiet = ctx();
        quiet.silence_hours = 6.0;
        assert!(motivation(0.7, true, &quiet) > base, "越沉默动机越强");

        let mut repeat = ctx();
        repeat.max_sim = 0.9;
        assert!(motivation(0.7, true, &repeat) < SILENCE_FLOOR, "语义重复压到沉默线以下");

        let mut focus = ctx();
        focus.deep_focus = true;
        assert!(motivation(0.7, true, &focus) < SILENCE_FLOOR, "深专注重罚");

        let mut stale = ctx();
        stale.seed_age_hours = 24.0;
        assert!(motivation(0.7, true, &stale) < base, "念头随时间衰减");
    }

    #[test]
    fn parse_evaluation_tolerant_and_validating() {
        let ok = parse_evaluation(
            r#"{"speak": true, "pick": "S2", "expression_type": "观察", "intent": "陪一下", "hook": "从他在做的事说起", "reason": "刚专注完"}"#,
            3,
        )
        .unwrap();
        assert!(ok.speak);
        assert_eq!(ok.pick, "S2");
        assert_eq!(ok.expression_type, "观察");

        // 界外 id → 回落 S1；界外类型 → 回落自言自语。
        let fallback = parse_evaluation(
            r#"{"speak": true, "pick": "S9", "expression_type": "演讲", "reason": "r"}"#,
            2,
        )
        .unwrap();
        assert_eq!(fallback.pick, "S1");
        assert_eq!(fallback.expression_type, "自言自语");

        // 拒说但没给理由 → 不是决定。
        assert!(parse_evaluation(r#"{"speak": false}"#, 2).is_none());
        // 带 markdown 围栏也要能解析。
        assert!(parse_evaluation("```json\n{\"speak\": false, \"reason\": \"刚聊过\"}\n```", 2).is_some());
    }

    #[test]
    fn ingest_dedups_and_respects_cap() {
        use crate::db::test_utils::test_db;
        let db = test_db();
        let now = Utc::now();
        ingest(&db, Some("正在编辑：main.rs（desktop-pet 项目）"), &now);
        ingest(&db, Some("正在编辑：main.rs（desktop-pet 项目）"), &now);
        let n = db.with_conn(thoughts::count_pending).unwrap();
        assert_eq!(n, 1, "相同 stimulus 去重");
    }
}
