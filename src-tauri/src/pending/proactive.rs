//! Proactive behavior: decides when the pet should initiate a conversation.
//! Design doc 9.2: bubbles at most every 30 minutes; silent during deep focus.

use crate::db::facts::Fact;
use crate::db::pending::PendingEvent;
use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::emotion::state::EmotionState;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::retrieval::ScoredEpisode;
use crate::pending::selector::{self, AnchorCandidate, SelectorContext, SelectorTask};
use chrono::{DateTime, Datelike, Local, Utc};
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

/// Default share of proactive bubbles that anchor on a memory (percent 0-100);
/// the rest are lively, anchorless chatter. 15% memory / 85% lively — most
/// bubbles are 碎碎念, not recall (user feedback 2026-08-14: 不需要那么多消息带记忆).
/// Overridable via config `[proactive] memory_bubble_ratio`.
pub const DEFAULT_MEMORY_RATIO: i64 = 15;

/// Chance (percent) that welcome-back / lonely-nudge / ritual bubbles attach a
/// memory anchor at all. They used to anchor almost always; now most are pure
/// emotional greetings, and only occasional ones gently reference a memory
/// (user feedback 2026-08-14: 不用带那么多记忆).
pub const ANCHOR_PROB_PERCENT: u32 = 25;

/// Hard repeat-exclusion window for facts: a fact surfaced within this many
/// days can never be picked again as an anchor (round-robin over the rest).
pub const FACT_REPEAT_WINDOW_DAYS: i64 = 7;

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
    /// Why this memory surfaced NOW (recall_reason) — Rust-computed from the
    /// anchor's recall history ("从没主动提起过的旧事" / "对你们很重要的时刻"
    /// / "你答应过的事到时间了"). Injected into the prompt so she voices the
    /// reason naturally; kept on the outcome for Debug Panel observability
    /// (Architecture #11). None for anchorless (lively) bubbles.
    pub anchor_reason: Option<String>,
}

/// A resolved anchor pick shared by the selector and mechanical paths.
/// `fact`/`episode` refs let the caller record the surfacing ledger entry.
struct AnchorPick<'a> {
    anchor: String,
    goal: &'static str,
    tone: &'static str,
    reason: String,
    fact: Option<&'a Fact>,
    episode: Option<&'a crate::db::episodes::Episode>,
}

/// Outcome of the LLM selector pass. `Declined` (pool non-empty, nothing worth
/// saying) is a judgment to stay silent; `Empty` is no pool at all (callers
/// keep their existing fallbacks: lively bubble / anchorless greeting).
enum SelectorOutcome<'a> {
    Declined,
    Empty,
    Picked(AnchorPick<'a>),
}

/// Chinese label for a time-of-day (selector context display).
fn tod_label(tod: crate::perception::time::TimeOfDay) -> &'static str {
    use crate::perception::time::TimeOfDay;
    match tod {
        TimeOfDay::Morning => "上午",
        TimeOfDay::Afternoon => "下午",
        TimeOfDay::Evening => "傍晚",
        TimeOfDay::LateNight => "深夜",
        TimeOfDay::DeepNight => "凌晨",
    }
}

/// Formats her recent unprompted bubbles as context lines, newest first.
/// The cross-bubble continuity source: both the selector (judgment) and the
/// voicing prompts consume it, so she never repeats her own last words.
fn last_bubble_lines(db: &DbState, now: &DateTime<Utc>, n: usize) -> Vec<String> {
    db.with_conn(|conn| crate::db::bubble_log::get_recent(conn, n))
        .unwrap_or_default()
        .into_iter()
        .map(|e| {
            let dt = chrono::DateTime::parse_from_rfc3339(&e.time)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or(*now);
            let text: String = e.text.chars().take(40).collect();
            let anchor = if e.anchor.is_empty() {
                String::new()
            } else {
                let a: String = e.anchor.chars().take(30).collect();
                format!("（锚定：{a}）")
            };
            format!(
                "{}（{}）：「{}」{}",
                selector::relative_ago(now, &dt),
                selector::local_clock(&dt),
                text,
                anchor
            )
        })
        .collect()
}

/// The voicing-prompt injection: "here is what you last said unprompted —
/// don't repeat it". Empty when she has never bubbled.
fn last_bubbles_clause(db: &DbState, now: &DateTime<Utc>) -> String {
    let lines = last_bubble_lines(db, now, 2);
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "（你自己最近主动开口说的是：{}。别重复它们的内容和句式，也别接着上一条的话头往下说。）\n",
        lines.join("；")
    )
}

/// Appends a successful bubble outcome to the log (best-effort — logging must
/// never break bubbling).
fn log_bubble(db: &DbState, kind: &str, reply: &str, anchor: &str, anchor_reason: Option<&str>) {
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) =
        db.with_conn(|conn| crate::db::bubble_log::insert(conn, kind, reply, anchor, anchor_reason, &now))
    {
        log::warn!("[bubble_log] insert failed: {}", e);
    }
}

/// Fresh anchorable facts in round-robin order (fewest-surfaced first) — the
/// ordered pool behind both `sample_anchorable_fact` and the selector pool.
fn fresh_anchorable_facts<'a>(facts: &'a [Fact], now: &DateTime<Utc>) -> Vec<&'a Fact> {
    let mut fresh: Vec<&Fact> = facts
        .iter()
        .filter(|f| is_anchorable_fact(f))
        .filter(|f| !surfaced_recently(f, now))
        .collect();
    fresh.sort_by(|a, b| {
        a.surfaced_count
            .cmp(&b.surfaced_count)
            .then_with(|| a.last_surfaced_at.cmp(&b.last_surfaced_at))
            .then_with(|| a.mention_count.cmp(&b.mention_count))
    });
    fresh
}

/// Whether an episode is inside the surfacing cooldown window (mirror of
/// `sample_surface_anchor`'s filter, exposed for candidate-pool building).
fn episode_in_cooldown(ep: &crate::db::episodes::Episode, now: &DateTime<Utc>) -> bool {
    ep.last_recalled_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            now.signed_duration_since(dt.with_timezone(&Utc))
                < chrono::Duration::hours(crate::mind::retrieval::SURFACE_COOLDOWN_HOURS)
        })
        .unwrap_or(false)
}

/// Reference back into the retrieval results for a pool candidate.
#[derive(Clone, Copy)]
enum PoolRef<'a> {
    Fact(&'a Fact),
    Episode(&'a crate::db::episodes::Episode),
}

/// Builds the candidate pool the LLM chooses from: up to 4 fresh anchorable
/// facts (round-robin order), up to 3 fresh episodes (retrieval-score order),
/// plus — on the same 1/3 roll the mechanical path uses — one weak-relevance
/// serendipity candidate, so "意外想起" stays offerable as a choice rather
/// than a forced pick. The pool respects every hard exclusion the mechanical
/// path enforces; the selector only chooses AMONG pre-vetted candidates.
fn build_candidate_pool<'a>(
    facts: &'a [Fact],
    episodes: &'a [ScoredEpisode],
    now: &DateTime<Utc>,
) -> Vec<(AnchorCandidate, PoolRef<'a>)> {
    let mut pool: Vec<(AnchorCandidate, PoolRef)> = Vec::new();
    for f in fresh_anchorable_facts(facts, now).into_iter().take(4) {
        pool.push((
            AnchorCandidate {
                id: format!("fact:{}", f.id),
                kind: "fact",
                text: present_anchor(&format!("{}: {}", f.key, f.value), Some(&f.created_at)),
                hint: format!(
                    "事实｜{}｜用户提过 {} 次，她主动提起过 {} 次",
                    fact_surface_reason(f),
                    f.mention_count,
                    f.surfaced_count
                ),
            },
            PoolRef::Fact(f),
        ));
    }
    let mut episode_count = 0;
    for se in episodes {
        if episode_count >= 3 {
            break;
        }
        if episode_in_cooldown(&se.episode, now) {
            continue;
        }
        pool.push((
            AnchorCandidate {
                id: format!("ep:{}", se.episode.id),
                kind: "episode",
                text: with_emotion_anchor(
                    present_anchor(&se.episode.summary, Some(&se.episode.time)),
                    &se.episode,
                ),
                hint: format!(
                    "经历｜{}｜检索相关度 {:.2}",
                    episode_surface_reason(&se.episode, now),
                    se.score
                ),
            },
            PoolRef::Episode(&se.episode),
        ));
        episode_count += 1;
    }
    // Serendipity offer: one weak-band episode (1/3 roll, deduped) — the
    // selector may take the surprise or leave it.
    {
        let mut rng = rand::thread_rng();
        if rng.gen_range(0..3) == 0 {
            if let Some(i) = crate::mind::retrieval::sample_serendipity_anchor(episodes, &mut rng) {
                let ep = &episodes[i].episode;
                if !pool.iter().any(|(c, _)| c.id == format!("ep:{}", ep.id)) {
                    pool.push((
                        AnchorCandidate {
                            id: format!("ep:{}", ep.id),
                            kind: "episode",
                            text: with_emotion_anchor(
                                present_anchor(&ep.summary, Some(&ep.time)),
                                ep,
                            ),
                            hint: format!(
                                "经历｜弱相关联想（意外想起的那种）｜检索相关度 {:.2}",
                                episodes[i].score
                            ),
                        },
                        PoolRef::Episode(ep),
                    ));
                }
            }
        }
    }
    pool
}

/// Resolves a picked candidate back to an AnchorPick (with ledger entry
/// recorded, preserving the "surfaced BEFORE the voicing call" ordering).
fn pick_from_pool<'a>(
    pool: &[(AnchorCandidate, PoolRef<'a>)],
    id: &str,
    reason: String,
    db: &DbState,
    now: &DateTime<Utc>,
) -> Option<AnchorPick<'a>> {
    // Copy the PoolRef out (it borrows the retrieval results, not the local
    // pool vec) so the returned pick outlives this call.
    let r = pool.iter().find(|(c, _)| c.id == id).map(|(_, r)| *r)?;
    match r {
        PoolRef::Fact(f) => {
            record_anchor_surfaced(db, Some(f), None, &now.to_rfc3339());
            Some(AnchorPick {
                anchor: present_anchor(&format!("{}: {}", f.key, f.value), Some(&f.created_at)),
                goal: "accompany",
                tone: "playful",
                reason,
                fact: Some(f),
                episode: None,
            })
        }
        PoolRef::Episode(ep) => {
            record_anchor_surfaced(db, None, Some(ep), &now.to_rfc3339());
            Some(AnchorPick {
                anchor: with_emotion_anchor(present_anchor(&ep.summary, Some(&ep.time)), ep),
                goal: "accompany",
                tone: "gentle",
                reason,
                fact: None,
                episode: Some(ep),
            })
        }
    }
}

/// The LLM selector pass: presents the pre-vetted candidate pool + moment
/// context (time, mood, her last bubbles) and lets the flash model choose or
/// decline. Never fabricates — it can only pick ids from the pool.
async fn selector_pick<'a>(
    llm: &LlmClient,
    db: &DbState,
    facts: &'a [Fact],
    episodes: &'a [ScoredEpisode],
    emotion: &EmotionState,
    task: SelectorTask,
) -> Result<SelectorOutcome<'a>, String> {
    let now_utc = Utc::now();
    let pool = build_candidate_pool(facts, episodes, &now_utc);
    if pool.is_empty() {
        return Ok(SelectorOutcome::Empty);
    }
    let local = Local::now();
    let weekday = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
        [local.weekday().num_days_from_monday() as usize];
    let ctx = SelectorContext {
        task,
        now_local: format!("{}（{}）{}", local.format("%Y-%m-%d"), weekday, local.format("%H:%M")),
        tod: tod_label(crate::perception::time::current_time_of_day()).to_string(),
        mood: emotion.mood,
        loneliness: emotion.loneliness,
        last_bubbles: last_bubble_lines(db, &now_utc, 2),
    };
    let decision = selector::run(llm, &pool.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(), &ctx).await?;
    match decision.anchor_id {
        Some(id) => Ok(SelectorOutcome::Picked(pick_from_pool(&pool, &id, decision.reason, db, &now_utc).ok_or("selector picked an id missing from the pool")?)),
        None => {
            log::info!("[selector] declined: {}", decision.reason);
            Ok(SelectorOutcome::Declined)
        }
    }
}

/// The mechanical round-robin pick (the pre-selector behavior, kept as the
/// degradation fallback when the selector is disabled or fails). Records the
/// pick as surfaced before returning, exactly as the inline code did.
fn mechanical_pick<'a>(
    db: &DbState,
    facts: &'a [Fact],
    episodes: &'a [ScoredEpisode],
    now_utc: &DateTime<Utc>,
) -> Option<AnchorPick<'a>> {
    let mut rng = rand::thread_rng();
    // Memory Serendipity (design §7.4): ~1/3 of memory bubbles anchor a
    // WEAKLY-related memory instead of the strongest match.
    let serendipity_idx = if rng.gen_range(0..3) == 0 {
        crate::mind::retrieval::sample_serendipity_anchor(episodes, &mut rng)
    } else {
        None
    };
    let fact = if serendipity_idx.is_none() {
        sample_anchorable_fact(facts, now_utc)
    } else {
        None
    };
    let episode = if let Some(i) = serendipity_idx {
        Some(&episodes[i].episode)
    } else if fact.is_none() {
        crate::mind::retrieval::sample_surface_anchor(episodes, now_utc, &mut rng)
            .map(|i| &episodes[i].episode)
    } else {
        None
    };
    match (fact, episode) {
        (Some(f), _) => {
            record_anchor_surfaced(db, Some(f), None, &now_utc.to_rfc3339());
            Some(AnchorPick {
                anchor: present_anchor(&format!("{}: {}", f.key, f.value), Some(&f.created_at)),
                goal: "accompany",
                tone: "playful",
                reason: fact_surface_reason(f),
                fact: Some(f),
                episode: None,
            })
        }
        (None, Some(ep)) => {
            record_anchor_surfaced(db, None, Some(ep), &now_utc.to_rfc3339());
            let reason = if serendipity_idx.is_some() {
                "不知道为什么突然想到这个".to_string()
            } else {
                episode_surface_reason(ep, now_utc)
            };
            Some(AnchorPick {
                anchor: with_emotion_anchor(present_anchor(&ep.summary, Some(&ep.time)), ep),
                goal: "accompany",
                tone: "gentle",
                reason,
                fact: None,
                episode: Some(ep),
            })
        }
        (None, None) => None,
    }
}

/// The pre-selector optional-anchor dice for welcome-back / lonely nudges:
/// ANCHOR_PROB_PERCENT chance to attach a mechanical pick, else anchorless.
/// Fallback path when the selector is disabled or fails. (Unification note:
/// the mechanical pick includes the serendipity 1/3 roll, which the old
/// inline welcome/lonely code did not — acceptable widening of a 25%×1/3
/// corner, one picker instead of two near-duplicates.)
fn anchor_dice(
    retrieval: &crate::mind::retrieval::RetrievalResult,
    db: &DbState,
    now_utc: &DateTime<Utc>,
) -> (String, bool, Option<String>) {
    let mut rng = rand::thread_rng();
    if rng.gen_range(0..100) >= ANCHOR_PROB_PERCENT {
        return (String::new(), false, None);
    }
    match mechanical_pick(db, &retrieval.facts, &retrieval.episodes, now_utc) {
        Some(p) => (p.anchor, true, Some(p.reason)),
        None => (String::new(), false, None),
    }
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
/// Principle 1 (LLM expresses, Rust maintains state): Rust vets the candidate
/// pool and maintains every ledger/timestamp; the LLM selects or declines, and
/// voices. Principle 8 (Cost): one flash selector call + one main voice call.
///
/// `memory_ratio` (0-100, percent) is the share of *anchorless-pending* bubbles
/// that take the memory-anchored branch; the rest go lively. Config-driven
/// (`[proactive] memory_bubble_ratio`, default 15) — Architecture #6.
///
/// `selector_enabled` (`[proactive] enable_llm_selector`, default true) routes
/// the memory branch through the LLM selector: it may decline everything (the
/// window stays silent — Architecture #12), pick one candidate with a reason,
/// or — on failure — degrade to the mechanical round-robin pick.
pub async fn generate(
    db: &DbState,
    llm: &LlmClient,
    embedding: Option<&EmbeddingService>,
    wm_context: &[ChatMessage],
    memory_ratio: i64,
    selector_enabled: bool,
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

    // `memory_ratio`% memory-anchored (loop-2 recall) vs the rest lively
    // (anchorless, moment-driven: self-talk / 撒娇 / a passing thought).
    // Weighted so bubbles don't default to the single dominant memory topic —
    // lively types voice *this moment*, not a recalled fact (user feedback
    // 2026-08-09: 要像真人突然找你聊天; 2026-08-14: 频率太高、带记忆太多).
    // Pick bubble type + retrieval query up front so the non-Send ThreadRng is
    // dropped before any .await (tauri commands require the future to be Send).
    let (is_lively, query): (bool, &'static str) = {
        let mut rng = rand::thread_rng();
        // A due pending (user-set reminder) is time-sensitive — it must NOT be
        // skipped by a random lively bubble. Roll the lively dice only when
        // nothing is due; when a reminder is due, force the memory branch so
        // pending_due.first() anchors the bubble. The lively/memory split is
        // preserved for the no-pending case (diversity untouched). Surfaced
        // by closed-loop-2 harness 2026-08-09.
        let is_lively = pending_due.is_empty() && rng.gen_range(0..100) >= memory_ratio;
        let query = MEMORY_QUERIES[rng.gen_range(0..MEMORY_QUERIES.len())];
        (is_lively, query)
    };
    if is_lively {
        return generate_lively(db, llm, wm_context, &emotion).await;
    }

    // Memory-anchored: the rotated query surfaces different memories across
    // calls instead of always the dominant topic.
    let retrieval = crate::mind::retrieval::retrieve(query, &emotion, embedding, db, 8)?;

    let due_is_pet_promise = pending_due.first().map(|ev| ev.origin == "pet").unwrap_or(false);
    let (memory_anchor, goal, tone, anchor_reason): (String, &'static str, &'static str, String) =
        if let Some(ev) = pending_due.first() {
            // Reminders keep their date reference (event date) so a "明天面试"
            // reminder still lands on the right day after the title's deictic
            // words are stripped ("面试（这是 ta 7月26日 提到的事）").
            (
                present_anchor(&ev.title, Some(&ev.event_date)),
                "care",
                "gentle",
                pending_surface_reason(due_is_pet_promise).to_string(),
            )
        } else {
            // No due pending. The LLM selector (when enabled) judges whether
            // ANY candidate is worth spontaneously surfacing — trivial memories
            // like "喝雪碧" get declined instead of force-voiced (2026-08-16
            // 续⁴¹: "你喝雪碧的时候我都在看着"). The mechanical round-robin
            // pick stays as the disabled/failure fallback.
            let now_utc = Utc::now();
            let pick: Option<AnchorPick> = if selector_enabled {
                match selector_pick(
                    llm,
                    db,
                    &retrieval.facts,
                    &retrieval.episodes,
                    &emotion,
                    SelectorTask::Spontaneous,
                )
                .await
                {
                    Ok(SelectorOutcome::Picked(p)) => Some(p),
                    Ok(SelectorOutcome::Declined) => {
                        log::info!("[proactive] selector declined — she stays silent this window");
                        return Ok(None);
                    }
                    Ok(SelectorOutcome::Empty) => None,
                    Err(e) => {
                        log::warn!("[proactive] selector failed ({}); mechanical pick", e);
                        mechanical_pick(db, &retrieval.facts, &retrieval.episodes, &now_utc)
                    }
                }
            } else {
                mechanical_pick(db, &retrieval.facts, &retrieval.episodes, &now_utc)
            };
            match pick {
                Some(p) => (p.anchor, p.goal, p.tone, p.reason),
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
    // Cross-bubble continuity: she sees what she last said unprompted.
    let last_clause = last_bubbles_clause(db, &Utc::now());
    messages.push(ChatMessage::user(format!(
        "{last_clause}{}",
        due_bubble_prompt(&memory_anchor, due_is_pet_promise, &anchor_reason)
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
        Some(reply) => {
            // Log the outcome so the next window knows what she last said.
            let kind = if pending_due.is_empty() {
                "proactive_memory"
            } else {
                "due_pending"
            };
            log_bubble(db, kind, &reply, &memory_anchor, Some(&anchor_reason));
            Ok(Some(BubbleOutcome {
                reply,
                anchor: memory_anchor,
                anchor_reason: Some(anchor_reason),
            }))
        }
        None => Ok(None),
    }
}

/// Appends the moment's atmosphere ("当时的氛围") to a surfaced episode
/// anchor so she recalls it with warmth — "我想起你当时…" instead of
/// reciting a file (memory-trigger "context" idea, adapted). No-op when the
/// episode has no anchor. Appended AFTER present_anchor so the deictic
/// neutralization never touches the atmosphere text.
fn with_emotion_anchor(anchor: String, ep: &crate::db::episodes::Episode) -> String {
    match ep.emotion_anchor.as_deref() {
        Some(a) if !a.trim().is_empty() => format!("{}（当时的氛围：{}）", anchor, a),
        _ => anchor,
    }
}

/// How long ago an episode stops counting as "recently recalled".
const RECENT_RECALL_DAYS: i64 = 30;

/// Why a FACT surfaced now (recall_reason). Rust-computed from the surfacing
/// history the governance layer already tracks — the LLM only voices it
/// (Architecture #1). Companion to the fewest-surfaced rotation: the rotation
/// decides WHAT surfaces, this explains WHY it feels right to mention.
fn fact_surface_reason(f: &Fact) -> String {
    if f.surfaced_count == 0 {
        "一直没找到合适时机提起的事".to_string()
    } else {
        "你们常聊的话题".to_string()
    }
}

/// Why an EPISODE surfaced now. Priority: atmosphere > landmark > never
/// surfaced > long untouched > recently on her mind.
fn episode_surface_reason(ep: &crate::db::episodes::Episode, now: &DateTime<Utc>) -> String {
    if ep.emotion_anchor.as_deref().map(|a| !a.trim().is_empty()).unwrap_or(false) {
        return "想起来还带着当时的氛围".to_string();
    }
    if ep.is_landmark {
        return "对你们很重要的时刻".to_string();
    }
    if ep.recall_count == 0 {
        return "从没主动提起过的旧事".to_string();
    }
    if let Some(last) = ep.last_recalled_at.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()) {
        if now.signed_duration_since(last).num_days() > RECENT_RECALL_DAYS {
            return "很久没提起的旧事".to_string();
        }
    }
    "最近心里的事".to_string()
}

/// Why a DUE PENDING surfaced now — her promise vs the user's event.
fn pending_surface_reason(is_pet_promise: bool) -> &'static str {
    if is_pet_promise {
        "你答应过的事，到时间了"
    } else {
        "ta 之前提过的事，到日子了"
    }
}

/// User prompt for the due-pending / surfaced-memory bubble. Two voices:
/// - user event (origin="user"): she suddenly remembers something THE USER
///   told her about and gently brings it up.
/// - pet promise (origin="pet"): time is up on something SHE said she would
///   do — she shows up to keep her word. Forgetting her own promise is the
///   most trust-damaging failure a companion can make, so the voice is
///   "我说过要…" not an alarm-style reminder.
fn due_bubble_prompt(anchor: &str, is_pet_promise: bool, anchor_reason: &str) -> String {
    // The shared "可不问" tail: most memory bubbles should be a warm statement,
    // not a question (user feedback 2026-08-14: 多为问句).
    let no_question = "这条不一定要问问题——大多数时候就是一句带着温度的陈述；真的好奇最多一个问句，别追问。";
    // recall_reason: why THIS memory surfaced now (Rust-computed; the LLM only
    // voices it — never invents a reason). Lets her open with "我突然想起你
    // 之前说…" instead of reciting the anchor cold.
    let reason_clause = format!("你想起它的由头：{anchor_reason}——自然带出这个感觉，但别照搬这句话。");
    if is_pet_promise {
        format!(
            "（现在是兑现你自己承诺的时刻：{}。这是你亲口答应 ta 的事，时间到了。以「我说过要…」的口吻自然地兑现或提起，像一个说到做到的人，不是闹钟式提醒，也不要道歉式检讨。只能围绕它原意来聊，绝不能换成别的项目、事件或名字，更不能编出记忆里没有的具体事；实在没什么好接的，就说句简单的招呼。{reason_clause}{no_question}按规则回复，尤其规则 8。）",
            anchor
        )
    } else {
        format!(
            "（你刚刚突然想起了这件事，想主动跟用户说。你想起来的只有这一件：{}。只能围绕它原意来聊，它是什么就说什么，绝不能换成别的项目、事件或名字，更不能编出记忆里没有的具体事；实在没什么好接的，就说句简单的招呼。{reason_clause}{no_question}按规则回复，尤其规则 8。）",
            anchor
        )
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
/// Pub for the soul-style harness (M6 bubble-persona blind test).
pub async fn generate_lively(
    db: &DbState,
    llm: &LlmClient,
    wm_context: &[ChatMessage],
    emotion: &EmotionState,
) -> Result<Option<BubbleOutcome>, String> {
    // Identity-only retrieval (Soul v2 plan L2b): she knows who she is, who
    // she's talking to, and how old the relationship is — but carries no
    // episodic/factual memories, so grounding_guard still blocks any invented
    // claim about the user's past. Previously RetrievalResult::default() left
    // 85% of bubbles persona-less (format_persona fell back to a generic
    // English companion line).
    let retrieval = crate::mind::retrieval::load_identity(db);
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
    // Cross-bubble continuity: she sees what she last said unprompted (the
    // Soul v2 observation item "lively 别和上一条像" finally has its data
    // source — a static cliché blacklist can't know what she actually said).
    let last_clause = last_bubbles_clause(db, &Utc::now());
    messages.push(ChatMessage::user(format!(
        "{last_clause}{}",
        lively_prompt(emotion, hour)
    )));

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
    if let Some(reply) = &reply {
        log_bubble(db, "lively", reply, "", None);
    }
    Ok(reply.map(|reply| BubbleOutcome {
        reply,
        anchor: String::new(),
        anchor_reason: None,
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
    // Soul v2 L2c：v2 声部改写——短句、配料 + 正面行为描述；原三行负面
    // 黑名单说教压缩成一条手术刀（防套路语义逐条保留，框成"说出来就不像
    // 你"的身份理由，不再是禁令腔）。禁编造约束原样保留。
    format!(
        "（此刻大概是{time_hint}，你{mood_hint}。没有特别的事，也不一定要 ta 回答——就是这一刻脑子里飘过一句话，随口说说。\n\n从一个小切口起头就好：一个小动作（伸懒腰、打哈欠、拨弄手边的东西）、刚注意到的一个细节（窗外的声音、屏幕的光、空气的温度）、一个身体感觉（犯困、饿、暖洋洋、肩膀酸）、一个荒唐的小念头、一句没头没尾的自言自语。像真人脑子里突然飘过的那一句，不是打招呼，也不是表达关心。\n\n开场让它自己长出来——这些现成句式说出来就不像你了：{time_avoid}、「忽然/突然」+「想你/想到你」、「阳光正好/太阳正暖」、「在吗/在干嘛/有事吗」。大部分时候就是一句陈述；真的好奇才问一个，ta 不回也完全没关系，别追问。\n\n只说 1 句，简短自然。规则 8 严禁编造：只谈你此刻的自己——感受、身边、身体，绝不假装记得用户跟你说过的具体事情或喜好。）"
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
/// Like `generate`: the memory anchor is *optional* (no anchor → still speak,
/// a welcome is always worth saying) and never fabricated (anchor comes only
/// from retrieval, Principle #3). With `selector_enabled`, the anchor choice
/// goes through the LLM selector in Garnish mode — it may attach one
/// candidate with a reason or return none (pure emotional greeting) instead
/// of the 25% dice.
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
    selector_enabled: bool,
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
    // Optional anchor: the selector (Garnish) decides whether a memory rides
    // along; decline / empty pool / dice-miss → anchorless pure greeting.
    // Never fabricated (anchor comes only from retrieval, Principle #3).
    let now_utc = Utc::now();
    let (memory_anchor, has_anchor, anchor_reason): (String, bool, Option<String>) =
        if selector_enabled {
            match selector_pick(
                llm,
                db,
                &retrieval.facts,
                &retrieval.episodes,
                &emotion,
                SelectorTask::Garnish,
            )
            .await
            {
                Ok(SelectorOutcome::Picked(p)) => (p.anchor, true, Some(p.reason)),
                Ok(_) => (String::new(), false, None),
                Err(e) => {
                    log::warn!("[welcome_back] selector failed ({}); anchor dice", e);
                    anchor_dice(&retrieval, db, &now_utc)
                }
            }
        } else {
            anchor_dice(&retrieval, db, &now_utc)
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
        let reason_clause = anchor_reason
            .as_deref()
            .map(|r| format!("你想起它的由头：{r}——自然带出这个感觉，但别照搬这句话。"))
            .unwrap_or_default();
        format!("你想起 ta 之前跟你提过的事：{memory_anchor}。{reason_clause}可以顺便轻轻关心一句，但只能围绕这件事的原意，别把它换成别的话题、别编出没提过的项目或细节，别像在完成任务。")
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
    let last_clause = last_bubbles_clause(db, &Utc::now());
    messages.push(ChatMessage::user(format!(
        "{last_clause}（对方离开了 {absence_phrase}，刚刚回来。你注意到 ta 回来了，想自然地打个招呼。{anchor_clause}{thought_clause}这条不一定要问问题——大多数时候就是一句带着温度的陈述，真的好奇最多一个问句。简短自然，1-2 句，像个真的在等 ta 回来的人。称呼对方用「你」，不要用「用户」。按规则回复。）"
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
        Some(reply) => {
            log_bubble(db, "welcome_back", &reply, &memory_anchor, anchor_reason.as_deref());
            Ok(Some(BubbleOutcome {
                reply,
                anchor: memory_anchor,
                anchor_reason,
            }))
        }
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
    selector_enabled: bool,
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
    // Optional anchor: the selector (Garnish) decides whether a memory rides
    // along; decline / empty pool / dice-miss → pure "just thinking of you".
    // No anchor → still a valid nudge. Never fabricated (retrieval-only).
    let now_utc = Utc::now();
    let (memory_anchor, has_anchor, anchor_reason): (String, bool, Option<String>) =
        if selector_enabled {
            match selector_pick(
                llm,
                db,
                &retrieval.facts,
                &retrieval.episodes,
                &emotion,
                SelectorTask::Garnish,
            )
            .await
            {
                Ok(SelectorOutcome::Picked(p)) => (p.anchor, true, Some(p.reason)),
                Ok(_) => (String::new(), false, None),
                Err(e) => {
                    log::warn!("[lonely_nudge] selector failed ({}); anchor dice", e);
                    anchor_dice(&retrieval, db, &now_utc)
                }
            }
        } else {
            anchor_dice(&retrieval, db, &now_utc)
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
        let reason_clause = anchor_reason
            .as_deref()
            .map(|r| format!("你想起它的由头：{r}——自然带出这个感觉，但别照搬这句话。"))
            .unwrap_or_default();
        format!("你刚好想起 ta 之前跟你提过的事：{memory_anchor}。{reason_clause}可以顺便轻轻带一句，像真的惦记着这件事，但只能围绕它原意，别换成别的话题、别编出没提过的细节。")
    } else {
        String::new()
    };

    let last_clause = last_bubbles_clause(db, &Utc::now());
    messages.push(ChatMessage::user(format!(
        "{last_clause}（你一个人待了一会儿，有点想 ta。ta 就在旁边但没说话，你想轻轻戳一下 ta——不是催 ta 回复，也不是有事要说，就是想让 ta 知道你在。{anchor_clause}只说 1 句，简短、自然、别黏人、这句最好连问句都别带，一句带温度的陈述就好。按规则回复，尤其规则 8 严禁编造。）"
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
        Some(reply) => {
            log_bubble(db, "lonely_nudge", &reply, &memory_anchor, anchor_reason.as_deref());
            Ok(Some(BubbleOutcome {
                reply,
                anchor: memory_anchor,
                anchor_reason,
            }))
        }
        None => Ok(None),
    }
}

/// Round-robin pick among anchorable facts (repeat-window hardening 2026-08-14;
/// replaces the weighted-random draw).
///
/// Deterministic rotation, not a probability: among anchorable facts NOT
/// surfaced within `FACT_REPEAT_WINDOW_DAYS`, pick the one with the fewest
/// surfacings, then the oldest `last_surfaced_at` (never-surfaced first), then
/// the least-mentioned. A fact voiced today can NEVER be picked again within
/// the window — the "同一条记忆绝对不能多次浮现" guarantee for facts. When the
/// whole pool is inside the window, returns None (caller falls back to
/// episodes → lively rather than repeat a memory).
pub fn sample_anchorable_fact<'a>(facts: &'a [Fact], now: &DateTime<Utc>) -> Option<&'a Fact> {
    fresh_anchorable_facts(facts, now).into_iter().next()
}

/// Whether a fact was surfaced within the hard repeat window (7 days).
fn surfaced_recently(f: &Fact, now: &DateTime<Utc>) -> bool {
    match &f.last_surfaced_at {
        Some(s) => match DateTime::parse_from_rfc3339(s) {
            Ok(d) => (*now - d.with_timezone(&Utc)).num_days() < FACT_REPEAT_WINDOW_DAYS,
            Err(_) => false,
        },
        None => false,
    }
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

/// Presents a memory anchor to the LLM: relative time words stripped
/// ("今天在找实习" → "在找实习") plus the memory's date as a bracketed
/// reference ("（这是 ta 8月13日 提到的事）") — she gets a correct sense of when
/// without echoing stale deictic words (fix: "你说今天在找实习").
pub fn present_anchor(anchor: &str, date: Option<&str>) -> String {
    let mut s = crate::mind::deictic::neutralize_deictic(anchor);
    if let Some(d) = date {
        if let Some(fmt) = crate::mind::deictic::format_memory_date(d) {
            s.push_str(&format!("（这是 ta {} 提到的事）", fmt));
        }
    }
    s
}

/// Records that the picked anchor was actually surfaced — the ONLY episode
/// reinforcement in the proactive path (fix: `reinforce_top` used to mark all
/// top-8 episodes, inflating recall_count and breaking the cooldown window).
/// Facts get `surfaced_count + 1 / last_surfaced_at = now` so the round-robin
/// rotates past them; episodes reuse `last_recalled_at` via `episodes::reinforce`.
/// Called the moment the anchor is picked, before generation (conservative:
/// 宁少勿突兀 — a failed generation still consumes the pick, no instant re-pick).
pub fn record_anchor_surfaced(db: &DbState, fact: Option<&Fact>, episode: Option<&crate::db::episodes::Episode>, now: &str) {
    if let Some(f) = fact {
        if let Err(e) = db.with_conn(|conn| crate::db::facts::bump_surfaced(conn, &f.id, now)) {
            log::warn!("Failed to bump fact surfaced {}: {}", f.id, e);
        }
    }
    if let Some(ep) = episode {
        if let Err(e) = db.with_conn(|conn| crate::db::episodes::reinforce(conn, &ep.id, now)) {
            log::warn!("Failed to reinforce episode {}: {}", ep.id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pending::PendingEvent;

    #[test]
    fn test_due_bubble_prompt_user_event_voice() {
        let p = due_bubble_prompt("明天有个实习面试", false, "ta 之前提过的事，到日子了");
        assert!(p.contains("突然想起了这件事"), "user events surface as recall");
        assert!(!p.contains("兑现"), "user events are not promises");
        assert!(p.contains("明天有个实习面试"));
        assert!(p.contains("由头"), "recall_reason is injected");
    }

    #[test]
    fn test_due_bubble_prompt_pet_promise_voice() {
        let p = due_bubble_prompt("明早叫 ta 起床", true, "你答应过的事，到时间了");
        assert!(p.contains("兑现你自己承诺"), "pet promise frames as keeping her word");
        assert!(p.contains("我说过要"), "promise voice, not alarm reminder");
        assert!(p.contains("明早叫 ta 起床"));
        assert!(!p.contains("突然想起了这件事"));
    }

    #[test]
    fn test_surface_reasons() {
        // Fact: never surfaced vs. rotated before.
        let mut f = Fact {
            id: "f1".into(), category: "preference".into(), key: "drink".into(),
            value: "三分糖奶茶".into(), confidence: 0.9,
            valid_from: None, valid_to: None, source_episode: None,
            mention_count: 3, created_at: "2026-08-01T00:00:00+00:00".into(),
            updated_at: "2026-08-01T00:00:00+00:00".into(),
            surfaced_count: 0, last_surfaced_at: None,
        };
        assert_eq!(fact_surface_reason(&f), "一直没找到合适时机提起的事");
        f.surfaced_count = 4;
        assert_eq!(fact_surface_reason(&f), "你们常聊的话题");

        // Episode: priority atmosphere > landmark > never recalled > stale > default.
        let now = Utc::now();
        let mut ep = crate::db::episodes::Episode {
            id: "ep_1".into(), time: "2026-07-01T00:00:00+00:00".into(),
            summary: "和糯米去看猫".into(), emotion: Some("开心".into()),
            importance: 0.7, is_landmark: false, subject: "user".into(),
            participants: None, topics: None, source_type: "conversation".into(),
            source_conversation_id: None, source_turn: None,
            memory_strength: 0.7, recall_count: 2,
            last_recalled_at: Some(now.to_rfc3339()),
            consolidated: false, created_at: "2026-07-01T00:00:00+00:00".into(),
            emotion_anchor: None,
        };
        assert_eq!(episode_surface_reason(&ep, &now), "最近心里的事");
        ep.recall_count = 0;
        assert_eq!(episode_surface_reason(&ep, &now), "从没主动提起过的旧事");
        ep.recall_count = 2;
        let stale = (now - chrono::Duration::days(60)).to_rfc3339();
        ep.last_recalled_at = Some(stale);
        assert_eq!(episode_surface_reason(&ep, &now), "很久没提起的旧事");
        ep.is_landmark = true;
        assert_eq!(episode_surface_reason(&ep, &now), "对你们很重要的时刻");
        ep.emotion_anchor = Some("在猫咖，眼睛亮亮的".into());
        assert_eq!(episode_surface_reason(&ep, &now), "想起来还带着当时的氛围");

        assert_eq!(pending_surface_reason(true), "你答应过的事，到时间了");
        assert_eq!(pending_surface_reason(false), "ta 之前提过的事，到日子了");
    }

    #[test]
    fn test_with_emotion_anchor_appends_atmosphere() {
        let mut ep = crate::db::episodes::Episode {
            id: "ep_1".into(), time: "2026-08-14T10:00:00+00:00".into(),
            summary: "和糯米去看猫".into(), emotion: Some("开心".into()),
            importance: 0.7, is_landmark: false, subject: "user".into(),
            participants: None, topics: None, source_type: "conversation".into(),
            source_conversation_id: None, source_turn: None,
            memory_strength: 0.7, recall_count: 0, last_recalled_at: None,
            consolidated: false, created_at: "2026-08-14T10:00:00+00:00".into(),
            emotion_anchor: Some("在猫咖，眼睛亮亮的".into()),
        };
        let out = with_emotion_anchor("和糯米去看猫（ta 8月14日 提到的事）".into(), &ep);
        assert!(out.contains("（当时的氛围：在猫咖，眼睛亮亮的）"));
        assert!(out.starts_with("和糯米去看猫"), "anchor body comes first");

        ep.emotion_anchor = Some("   ".into());
        assert_eq!(with_emotion_anchor("anchor".into(), &ep), "anchor", "blank anchor is a no-op");

        ep.emotion_anchor = None;
        assert_eq!(with_emotion_anchor("anchor".into(), &ep), "anchor", "missing anchor is a no-op");
    }

    fn pending_event(id: &str, title: &str) -> PendingEvent {
        PendingEvent {
            origin: "user".to_string(),
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
    fn test_sample_anchorable_fact_round_robin_never_repeats() {
        // Round-robin (2026-08-14): never-surfaced facts win deterministically;
        // a fact surfaced within the 7-day repeat window is HARD-excluded —
        // the "同一条记忆绝对不能多次浮现" guarantee.
        let now = Utc::now();
        let make = |id: &str, key: &str, confidence: f64, mentions: i64, surfaced: i64, last: Option<&str>| Fact {
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
            surfaced_count: surfaced,
            last_surfaced_at: last.map(|s| s.to_string()),
        };
        // Old faithful: highest confidence but surfaced 100 times.
        let stale = make("f1", "movie", 0.98, 100, 100, Some("2026-08-01T00:00:00+00:00"));
        // Never surfaced, never mentioned → must win regardless of confidence.
        let fresh = make("f2", "hobby", 0.8, 0, 0, None);
        let facts = vec![stale.clone(), fresh.clone()];
        let picked = sample_anchorable_fact(&facts, &now).unwrap();
        assert_eq!(picked.id, "f2", "never-surfaced fact wins over heavily-surfaced");

        // Hard exclusion: everything surfaced within the window → None (caller
        // falls back to episodes/lively rather than repeat a memory). Both
        // facts were surfaced recently (relative to `now`) — inside 7 days.
        let stale_in_window = make(
            "f1",
            "movie",
            0.98,
            100,
            100,
            Some(&(now - chrono::Duration::days(1)).to_rfc3339()),
        );
        let just_now = (now - chrono::Duration::hours(1)).to_rfc3339();
        let repeated = make("f3", "drink", 0.9, 0, 1, Some(&just_now));
        assert!(
            sample_anchorable_fact(&[stale_in_window, repeated], &now).is_none(),
            "all facts inside the repeat window → None, never a repeat"
        );
    }

    #[test]
    fn test_sample_anchorable_fact_filters_non_anchorable() {
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
            surfaced_count: 0,
            last_surfaced_at: None,
        };
        let now = Utc::now();
        assert!(sample_anchorable_fact(&[low_conf], &now).is_none());
        assert!(sample_anchorable_fact(&[], &now).is_none());
    }

    #[test]
    fn test_present_anchor_strips_deictic_and_adds_date() {
        // "今天在找实习" recorded 2026-07-26 must never be echoed as "今天" later
        // (user feedback 2026-08-14: 时间完全对不上).
        let s = present_anchor("今天在找实习", Some("2026-07-26T08:00:00+00:00"));
        assert!(!s.contains("今天"), "deictic word must be stripped: {}", s);
        assert!(s.contains("在找实习"));
        assert!(s.contains("7月26日"), "date reference injected: {}", s);

        // No date available → just stripped.
        assert_eq!(present_anchor("明天去面试", None), "去面试");

        // No deictic → content untouched (reference still appended when known).
        let s3 = present_anchor("在准备找实习", Some("2026-07-26T08:00:00+00:00"));
        assert!(s3.starts_with("在准备找实习"), "no-deictic content preserved: {}", s3);
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

    fn make_fact(id: &str, key: &str, confidence: f64, surfaced: i64, last: Option<&str>) -> Fact {
        Fact {
            id: id.to_string(),
            category: "preference".to_string(),
            key: key.to_string(),
            value: "value".to_string(),
            confidence,
            valid_from: None,
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: "2026-07-01T00:00:00+00:00".to_string(),
            updated_at: "2026-07-01T00:00:00+00:00".to_string(),
            surfaced_count: surfaced,
            last_surfaced_at: last.map(|s| s.to_string()),
        }
    }

    fn make_scored_episode(id: &str, summary: &str, score: f64, last_recalled: Option<&str>) -> ScoredEpisode {
        ScoredEpisode {
            episode: crate::db::episodes::Episode {
                id: id.to_string(),
                time: "2026-08-01T00:00:00+00:00".to_string(),
                summary: summary.to_string(),
                emotion: Some("开心".to_string()),
                importance: 0.7,
                is_landmark: false,
                subject: "user".to_string(),
                participants: None,
                topics: None,
                source_type: "conversation".to_string(),
                source_conversation_id: None,
                source_turn: None,
                memory_strength: 0.7,
                recall_count: 0,
                last_recalled_at: last_recalled.map(|s| s.to_string()),
                consolidated: false,
                created_at: "2026-08-01T00:00:00+00:00".to_string(),
                emotion_anchor: None,
            },
            score,
            score_breakdown: Default::default(),
        }
    }

    #[test]
    fn test_build_candidate_pool_filters_caps_and_ids() {
        let now = Utc::now();
        let in_window = (now - chrono::Duration::days(1)).to_rfc3339();
        // 6 fresh anchorable facts → capped at 4; plus one windowed and one
        // low-confidence fact → both hard-excluded.
        let mut facts: Vec<Fact> = (0..6)
            .map(|i| make_fact(&format!("f{i}"), &format!("k{i}"), 0.9, 0, None))
            .collect();
        facts.push(make_fact("windowed", "kw", 0.9, 1, Some(&in_window)));
        facts.push(make_fact("lowconf", "kl", 0.5, 0, None));
        // One fresh episode, one in-cooldown episode. Scores stay above the
        // serendipity band so the 1/3 roll can never add a surprise candidate
        // (determinism).
        let episodes = vec![
            make_scored_episode("ep_fresh", "在准备找实习", 0.81, None),
            make_scored_episode("ep_cool", "上周已提过", 0.7, Some(&in_window)),
        ];
        let pool = build_candidate_pool(&facts, &episodes, &now);
        let fact_ids: Vec<&str> = pool.iter().map(|(c, _)| c.id.as_str()).collect();
        assert_eq!(fact_ids.iter().filter(|i| i.starts_with("fact:")).count(), 4, "facts capped at 4");
        assert!(fact_ids.contains(&"fact:f0"), "fresh fact in pool: {:?}", fact_ids);
        assert!(!fact_ids.iter().any(|i| i.contains("windowed")), "repeat-window fact excluded");
        assert!(!fact_ids.iter().any(|i| i.contains("lowconf")), "low-confidence fact excluded");
        assert!(fact_ids.contains(&"ep:ep_fresh"), "fresh episode in pool: {:?}", fact_ids);
        assert!(!fact_ids.iter().any(|i| i.contains("ep_cool")), "cooldown episode excluded");
        // Candidate text carries the deictic-neutralized anchor + date ref.
        let (fresh_fact_candidate, _) = pool.iter().find(|(c, _)| c.id == "fact:f0").unwrap();
        assert!(fresh_fact_candidate.text.contains("k0: value"));
        assert!(fresh_fact_candidate.text.contains("7月1日"), "date reference attached");
    }

    #[test]
    fn test_pick_from_pool_resolves_and_records() {
        let db = crate::db::test_utils::test_db();
        let now = Utc::now();
        // Seed a real fact row so the surfacing ledger bump lands.
        let fact = make_fact("f_real", "goal", 0.9, 0, None);
        db.with_conn(|conn| crate::db::facts::dedup_insert(conn, &fact)).unwrap();
        let facts = vec![fact];
        let episodes = vec![make_scored_episode("ep_real", "和糯米去猫咖", 0.7, None)];
        let pool = build_candidate_pool(&facts, &episodes, &now);

        let pick = pick_from_pool(&pool, "fact:f_real", "她一直惦记".to_string(), &db, &now).unwrap();
        assert_eq!(pick.goal, "accompany");
        assert_eq!(pick.tone, "playful");
        assert_eq!(pick.reason, "她一直惦记");
        assert!(pick.fact.is_some());

        // The ledger was written BEFORE the voicing call would run.
        let after: Vec<Fact> = db.with_conn(|conn| crate::db::facts::get_all(conn)).unwrap();
        assert_eq!(after[0].surfaced_count, 1, "surfaced_count bumped on pick");

        // Episode resolution takes the gentle voice.
        let pick_ep = pick_from_pool(&pool, "ep:ep_real", "带着当时的氛围".to_string(), &db, &now).unwrap();
        assert_eq!(pick_ep.tone, "gentle");
        assert!(pick_ep.episode.is_some());

        // Unknown id → None (never guess).
        assert!(pick_from_pool(&pool, "fact:zz", String::new(), &db, &now).is_none());
    }

    #[test]
    fn test_last_bubbles_clause_empty_then_formatted() {
        let db = crate::db::test_utils::test_db();
        let now = Utc::now();
        assert_eq!(last_bubbles_clause(&db, &now), "", "no bubbles yet → no clause");
        let earlier = (now - chrono::Duration::hours(3)).to_rfc3339();
        db.with_conn(|conn| {
            crate::db::bubble_log::insert(conn, "lively", "窗外好像下雨了", "", None, &earlier)?;
            crate::db::bubble_log::insert(
                conn,
                "proactive_memory",
                "面试加油呀，我一直惦记着",
                "在准备找实习",
                Some("她一直惦记"),
                &(now - chrono::Duration::hours(1)).to_rfc3339(),
            )?;
            Ok(())
        })
        .unwrap();
        let clause = last_bubbles_clause(&db, &now);
        assert!(clause.contains("面试加油呀"), "most recent bubble text present: {clause}");
        assert!(clause.contains("窗外好像下雨了"), "second-most-recent present: {clause}");
        assert!(clause.contains("1 小时前"), "relative time present: {clause}");
        assert!(clause.contains("锚定"), "anchor label present: {clause}");
        assert!(clause.contains("别重复"), "anti-repeat instruction present");
    }
}
