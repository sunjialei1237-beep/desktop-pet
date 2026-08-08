//! Full conversation pipeline orchestration.
//! Architecture principle #4: direct call chain, no queues.

use crate::db::DbState;
use crate::embedding::EmbeddingService;
use crate::llm::client::{ChatMessage, LlmClient};
use crate::mind::gate::GateRoute;
use crate::mind::planner::Intent;
use rand::Rng;

/// Lightweight retrieval-score view for the debug panel (Architecture #11:
/// "检索了什么" — what was retrieved and why it ranked). Keeps the full
/// `ScoredEpisode`/`Episode` out of the snapshot payload.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetrievedScoreDebug {
    pub summary: String,
    pub score: f64,
    pub semantic: f64,
    pub strength: f64,
    pub recency: f64,
    pub emotion: f64,
}

/// Last turn's prompt-budget observability (#8 cost / #11 explainability):
/// how big the system prompt / total input was, against the budget. None on
/// silence turns (no LLM call, no prompt built).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptTokenDebug {
    pub system_tokens: usize,
    pub input_tokens: usize,
    pub budget: usize,
    pub conversation_turns: usize,
}

/// Result of a full conversation turn.
#[derive(Debug)]
pub struct ConversationResult {
    /// The pet's reply (empty if silence action was chosen).
    pub response: String,
    /// The Intent that directed the response.
    pub intent: Intent,
    /// The gate route assigned to the user message.
    pub route: GateRoute,
    /// Why retrieval was or was not triggered.
    pub trigger_reason: String,
    /// Potential hallucination violations detected.
    pub grounding_violations: Vec<String>,
    /// Top retrieved episodes with score breakdown (debug observability, #11).
    pub retrieved_scores: Vec<RetrievedScoreDebug>,
    /// Last turn's prompt token budget (None on silence). Debug observability.
    pub prompt_tokens: Option<PromptTokenDebug>,
}

/// Bundled inputs for one conversation turn — Architecture Principle #2
/// ("BrainState 统一快照"): one coherent handle instead of a 9-parameter
/// sprawl, so adding a turn-scoped input later touches the struct, not every
/// call site. The streaming token callback stays a separate `converse`
/// parameter: it's a generic `FnMut` side-channel (not turn state), and folding
/// it in would make the whole struct generic. Every field is borrowed for the
/// call's lifetime — call sites build it from references, no allocation.
pub struct ConverseCtx<'a> {
    pub text: &'a str,
    pub conversation_id: &'a str,
    pub turn: i32,
    pub wm_context: &'a [ChatMessage],
    pub llm: &'a LlmClient,
    pub db: &'a DbState,
    pub embedding: Option<&'a EmbeddingService>,
    pub pacing: &'a std::sync::Mutex<crate::mind::pacing::QuestionPacing>,
}

/// Full conversation pipeline:
/// Ingest -> Trigger -> Retrieve -> Plan -> Budget -> LLM -> Grounding.
pub async fn converse(
    ctx: &ConverseCtx<'_>,
    mut on_token: impl FnMut(&str),
) -> Result<ConversationResult, String> {
    // Bridge the unified snapshot to local names so the pipeline body reads
    // unchanged. Each field is a borrowed reference (Copy), so these are cheap
    // pointer copies, not moves or clones.
    let text = ctx.text;
    let conversation_id = ctx.conversation_id;
    let turn = ctx.turn;
    let wm_context = ctx.wm_context;
    let llm = ctx.llm;
    let db = ctx.db;
    let embedding = ctx.embedding;
    let pacing = ctx.pacing;
    let now = chrono::Utc::now().to_rfc3339();

    // Step 1: Ingest (Gate -> Extract -> Store).
    // Build known_facts summary so the extractor can avoid duplicates.
    let known_facts = db.with_conn(|conn| {
        let facts = crate::db::facts::get_by_category(conn, "preference")?;
        let summary: Vec<String> = facts.iter().take(20).map(|f| format!("{}: {}", f.key, f.value)).collect();
        Ok(summary.join("; "))
    })?;

    let outcome = crate::mind::ingest(text, conversation_id, turn, &known_facts, llm, db, embedding).await?;

    // Direct-answer mode for general-knowledge / technical questions: skip
    // memory retrieval and memory injection entirely, so the model answers
    // the question itself instead of hard-associating it with pet memories.
    let qa_mode = outcome.route == GateRoute::Question;

    // Step 2: Load emotion state from DB and convert to business type.
    let db_emotion = db.with_conn(crate::db::emotion::get)?;
    let emotion = crate::emotion::state::EmotionState {
        mood: db_emotion.mood,
        physical_energy: db_emotion.physical_energy,
        social_battery: db_emotion.social_battery,
        stress: db_emotion.stress,
        loneliness: db_emotion.loneliness,
        rest_need: db_emotion.rest_need,
    };

    // Step 3: Load pending events due.
    let pending_due = db.with_conn(|conn| crate::db::pending::get_due(conn, &now))?;

    // Step 4: Memory trigger.
    // Step 5: Retrieve memories. Skipped in QA mode — a direct answer needs
    // no memories, and retrieval would only pollute the prompt with
    // irrelevant pet topics.
    let (retrieval, trigger_reason) = if qa_mode {
        log::info!("QA mode: skipping memory retrieval (question route)");
        // Skip episodes/facts (the memory that derails a factual answer) but
        // KEEP identity: persona / relationship / user profile are who she is
        // and who she's talking to, not memory that sends the answer off-topic.
        // Loaded cheaply (no embedding, no episode scan) so a QA reply still
        // sounds like 璃 and can address the user by name. (Architecture #1:
        // identity is Rust-loaded, not LLM-imagined.)
        let mut r = crate::mind::retrieval::RetrievalResult::default();
        let _ = db.with_conn(|c| {
            r.persona_traits = crate::db::persona::get_all_traits(c).unwrap_or_default();
            r.relationship = crate::db::relationship::get(c).ok();
            r.user_profile = crate::db::onboarding::load(c).unwrap_or_default();
            Ok::<(), String>(())
        });
        (r, "question route (QA mode)".to_string())
    } else {
        let trigger_decision = crate::mind::trigger::should_retrieve(text, &emotion, wm_context);
        let reason = trigger_decision.reason.clone();
        if trigger_decision.should_retrieve {
            (
                crate::mind::retrieval::retrieve(text, &emotion, embedding, db, 5)?,
                reason,
            )
        } else {
            log::info!("Retrieval skipped: {}", trigger_decision.reason);
            (
                crate::mind::retrieval::retrieve(text, &emotion, embedding, db, 3)?,
                reason,
            )
        }
    };

    // Project the top retrieved episodes into a lightweight debug view (used in
    // both return paths below). Built once here while `retrieval` is in scope.
    let retrieved_scores: Vec<RetrievedScoreDebug> = retrieval
        .episodes
        .iter()
        .take(5)
        .map(|se| RetrievedScoreDebug {
            summary: se.episode.summary.clone(),
            score: se.score,
            semantic: se.score_breakdown.semantic,
            strength: se.score_breakdown.strength,
            recency: se.score_breakdown.recency,
            emotion: se.score_breakdown.emotion,
        })
        .collect();

    // Step 6: Planner — produce Intent. The per-turn context is bundled into
    // one BrainState snapshot (Architecture #2) — one coherent handle instead
    // of five loose references threaded into the decision.
    let relationship = db
        .with_conn(crate::db::relationship::get)
        .ok();
    let brain = crate::mind::brain_state::BrainState::new(
        text,
        &emotion,
        relationship.as_ref(),
        &pending_due,
        &retrieval,
    );
    let mut intent = crate::mind::planner::plan(&brain);

    // Closeness (0..100) feeds the mood label: at low closeness neutral/positive
    // moods surface as 害羞 (design §6.2 "陌生时拘谨"). Computed once, used by both
    // emotion-write sites below.
    let closeness = relationship.as_ref().map(|r| r.closeness).unwrap_or(0.0);

    // QA mode: strip planner directives that would steer the reply toward
    // memories or follow-up questions — just answer the question.
    if qa_mode {
        intent.goal = "converse".to_string();
        intent.memory_anchor.clear();
        intent.tone = "gentle".to_string();
        intent.proactive = false;
        // A question must be answered, never silenced — otherwise a rare
        // planner silence (fed the empty QA retrieval) would drop the answer.
        intent.action = "normal".to_string();
    }

    // Step 7: Check for silence action.
    if intent.action == "silence" {
        log::info!("Planner chose silence");
        // Silence = user is anxious enough to warrant quiet. Apply the turn's
        // emotion delta (silence goal adds stress+/mood-) before returning.
        let delta = crate::emotion::react::react_to_turn(text, &intent.goal);
        let new_mood = (emotion.mood + delta.mood).clamp(0.0, 1.0);
        let new_energy = (emotion.physical_energy + delta.physical_energy).clamp(0.0, 1.0);
        let new_social = (emotion.social_battery + delta.social_battery).clamp(0.0, 1.0);
        let new_stress = (emotion.stress + delta.stress).clamp(0.0, 1.0);
        let new_loneliness = (emotion.loneliness + delta.loneliness).clamp(0.0, 1.0);
        let new_state = crate::emotion::state::EmotionState {
            mood: new_mood,
            physical_energy: new_energy,
            social_battery: new_social,
            stress: new_stress,
            loneliness: new_loneliness,
            rest_need: emotion.rest_need,
        };
        let new_label = crate::emotion::state::derive_mood_label_with_closeness(&new_state, closeness);
        let _ = db.with_conn(|conn| {
            crate::db::emotion::update_fields(
                conn,
                Some(new_mood),
                Some(new_label),
                Some(new_energy),
                Some(new_social),
                Some(new_stress),
                Some(new_loneliness),
                None,
                &now,
            )
        });
        log::info!(
            "[emotion-react] (silence) mood {:.2}->{:.2} ({}) stress {:.2}->{:.2}",
            emotion.mood, new_mood, new_label, emotion.stress, new_stress,
        );
        let _ = db.with_conn(|conn| {
            crate::db::relationship::record_interaction(conn, "silence", &now)
        });
        return Ok(ConversationResult {
            response: String::new(),
            intent,
            route: outcome.route,
            trigger_reason,
            grounding_violations: vec![],
            retrieved_scores,
            prompt_tokens: None,
        });
    }

    // Step 7.5: Follow-up question frequency control. The planner stays pure
    // (architecture #8); the throttle lives in this orchestration layer. A
    // silence turn already returned above, so this only touches speaking turns.
    // QA mode skips pacing: a direct answer asks no follow-up question.
    if !qa_mode {
        let roll: f64 = rand::thread_rng().gen();
        let mut guard = pacing
            .lock()
            .map_err(|e| format!("pacing lock error: {}", e))?;
        let snapshot = guard.clone();
        let (new_goal, next) =
            crate::mind::pacing::throttle(&intent.goal, &snapshot, roll);
        log::info!(
            "[pacing] roll={:.3} credit={}->{} last={}->{} goal={}",
            roll, snapshot.credit, next.credit,
            snapshot.last_turn_was_question, next.last_turn_was_question, intent.goal
        );
        intent.goal = new_goal;
        *guard = next;
    }

    // Step 8: Budget — compress context into messages.
    let messages = if qa_mode {
        crate::mind::budget::allocate_qa(
            &retrieval,
            wm_context,
            &emotion,
            &intent,
        )
    } else {
        crate::mind::budget::allocate_and_compress(
            &retrieval,
            wm_context,
            &emotion,
            &intent,
        )
    };

    // Append the CURRENT user message as the final turn. This must come AFTER
    // budget compression so the latest question can never be truncated away,
    // and the LLM always answers *this* turn (not a stale history entry).
    let mut messages = messages;

    // If this turn just scheduled a reminder (extracted in Step 1), tell the
    // expression LLM so it naturally confirms it ("好的，3分钟后提醒你").
    // Extractor and converse are separate LLM calls; without this bridge she
    // wouldn't know she just accepted a reminder (Architecture Principle #2:
    // cross-module state flows through the shared IngestionOutcome).
    if let Some(pe) = outcome
        .extraction
        .as_ref()
        .and_then(|e| e.pending_event.as_ref())
    {
        let timing = if let Some(mins) = pe.offset_minutes {
            format!("{}分钟后", mins)
        } else if let Some(date) = pe.event_date.as_deref() {
            date.split('T').next().unwrap_or(date).to_string()
        } else {
            "稍后".to_string()
        };
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: format!(
                "（系统提示：你刚刚帮用户记下了一个提醒——「{}」，{}. 在回复里自然地确认这件事，简短温暖，比如「好的，{}提醒你」.）",
                pe.title, timing, timing
            ),
        });
    }

    // If this turn just forgot a memory (Step 1 ingest Forget route), tell the
    // expression LLM so it confirms naturally ("好，我忘了") WITHOUT repeating
    // the deleted content — repeating it means the forget failed and feels
    // creepy. If nothing was deleted (no confident match), she honestly says she
    // doesn't remember it (Architecture Principle #1: Rust decided what to
    // erase; the LLM only acknowledges).
    if let Some(forget) = outcome.forget.as_ref() {
        let content = if forget.deleted {
            "（系统提示：用户刚才让你忘掉一段记忆，你已经把它彻底忘了。简短温暖地确认你忘了，比如「好，我忘了」或「嗯，已经不记得了」。绝对不要复述或暗示那段内容——你真的忘了，就想不起来了。）".to_string()
        } else {
            "（系统提示：用户想让你忘掉某件事，但你的记忆里其实没有这段，可能是记混了。诚实又温和地说你好像不记得这件事。）".to_string()
        };
        log::info!("[converse] forget this turn: deleted={}", forget.deleted);
        messages.push(ChatMessage {
            role: "system".to_string(),
            content,
        });
    }

    // Surface any internal thought the last reflection left for "next time the
    // user shows up" (Design 7.1 / P13.2: she really thought of it, timestamp
    // proves it). Folded into THIS LLM turn — no extra call (Principle 8).
    // Consumed once: surface_thoughts marks it surfaced, so it fires at most
    // once whether voiced here or in a welcome-back bubble. Restrained: she
    // only drops it in when the turn naturally relates; otherwise she just chats.
    // Skipped in QA mode — a direct answer must not be derailed by a memory.
    let thought_clause = if qa_mode {
        None
    } else {
        match crate::soul::monologue::surface_thoughts(db) {
        Ok(thoughts) => match thoughts.first() {
            Some(t) => {
                log::info!(
                    "[converse] surfaced thought: {:?}",
                    t.content.chars().take(40).collect::<String>()
                );
                Some(format!(
                    "（系统提示：你之前独处时心里有过一个念头：「{}」。如果这轮聊天能自然关联到，可以轻轻带一点，像真的惦记过；但如果话题无关，就别强提，正常聊就好。）",
                    t.content
                ))
            }
            None => None,
        },
        Err(e) => {
            log::warn!("[converse] surface_thoughts failed: {}", e);
            None
        }
        }
    };
    if let Some(clause) = thought_clause {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: clause,
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: text.to_string(),
    });

    let system_tokens = crate::mind::budget::estimate_tokens(&messages[0].content);
    let prompt_debug = PromptTokenDebug {
        system_tokens,
        input_tokens: crate::mind::budget::estimate_messages_tokens(&messages),
        budget: if qa_mode {
            crate::mind::budget::qa_system_prompt_budget()
        } else {
            crate::mind::budget::system_prompt_budget()
        },
        conversation_turns: wm_context.len(),
    };
    log::info!(
        "[ctx] messages={} last_user={:?} system_tokens~={} history_turns={}",
        messages.len(),
        text.chars().take(40).collect::<String>(),
        system_tokens,
        wm_context.len(),
    );

    // Step 9: LLM — generate response. Streaming: each content token is
    // forwarded to `on_token` for live bubble rendering (architecture #10),
    // while the fully accumulated text is returned for grounding / emotion.
    let mut chat_result = llm
        .chat_stream(&messages, Some(0.8), Some(4096), &mut on_token)
        .await
        .map_err(|e| format!("LLM error: {:?}", e))?;
    // Retry once on empty content — the flash reasoning model occasionally eats
    // the whole budget (finish_reason=length) and returns empty (the same
    // transient failure mode as pitfall #3; extractor/gate/correction already
    // retry). Streaming: the first attempt emitted nothing, so the retry's
    // tokens flow into the same live bubble.
    if chat_result.content.trim().is_empty() {
        log::warn!("[converse] main reply empty on first attempt; retrying once");
        chat_result = llm
            .chat_stream(&messages, Some(0.8), Some(4096), &mut on_token)
            .await
            .map_err(|e| format!("LLM error on retry: {:?}", e))?;
        if chat_result.content.trim().is_empty() {
            log::warn!("[converse] main reply still empty after retry");
        }
    }
    let response = chat_result.content;

    // Step 10: Grounding check. Skipped in QA mode — retrieval has no
    // episodes/facts there, so the check could only false-positive against a
    // direct factual answer (no memories to ground claims against).
    let violations = if qa_mode {
        vec![]
    } else {
        crate::mind::grounding::check_groundedness(&response, &retrieval)
    };

    // Step 11: Record interaction.
    let _ = db.with_conn(|conn| {
        crate::db::relationship::record_interaction(conn, "chat", &now)
    });

    // Step 12: Emotion reactivity — apply rule-based deltas from this turn.
    // Pure rules only (principle #8); no LLM call. Makes the expression reflect
    // the conversation, not just the 30s homeostasis drift.
    let delta = crate::emotion::react::react_to_turn(text, &intent.goal);
    let new_mood = (emotion.mood + delta.mood).clamp(0.0, 1.0);
    let new_energy = (emotion.physical_energy + delta.physical_energy).clamp(0.0, 1.0);
    let new_social = (emotion.social_battery + delta.social_battery).clamp(0.0, 1.0);
    let new_stress = (emotion.stress + delta.stress).clamp(0.0, 1.0);
    let new_loneliness = (emotion.loneliness + delta.loneliness).clamp(0.0, 1.0);
    let new_state = crate::emotion::state::EmotionState {
        mood: new_mood,
        physical_energy: new_energy,
        social_battery: new_social,
        stress: new_stress,
        loneliness: new_loneliness,
        rest_need: emotion.rest_need,
    };
    let new_label = crate::emotion::state::derive_mood_label_with_closeness(&new_state, closeness);
    let _ = db.with_conn(|conn| {
        crate::db::emotion::update_fields(
            conn,
            Some(new_mood),
            Some(new_label),
            Some(new_energy),
            Some(new_social),
            Some(new_stress),
            Some(new_loneliness),
            None,
            &now,
        )
    });
    log::info!(
        "[emotion-react] mood {:.2}->{:.2} ({}) social {:.2}->{:.2} stress {:.2}->{:.2}",
        emotion.mood, new_mood, new_label,
        emotion.social_battery, new_social,
        emotion.stress, new_stress,
    );

    Ok(ConversationResult {
        response,
        intent,
        route: outcome.route,
        trigger_reason,
        grounding_violations: violations,
        retrieved_scores,
        prompt_tokens: Some(prompt_debug),
    })
}
