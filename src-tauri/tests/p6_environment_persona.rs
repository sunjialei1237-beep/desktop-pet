//! P6 人格回归：环境注入开/关 双跑（plan §6「人格回归」）。
//!
//! 担心：有关环境的上下文会让璃变成"机械助手"。用 3 条环境相关的自然
//! 问句，在 A（无 [Environment]）与 C（注入 [Environment]）两种消息下
//! 用 main model 各生成一次，然后三层检查：
//!   1. 规则层 `personality_drift_score`（无成本，防话痨/卖萌/黏人）；
//!   2. LLM 裁判（reflection model）按系统人格书打分 0-10 + 是否"助手腔"；
//!   3. 断言两个条件的平均分都在人设线以上，且开关之间没有明显漂移。
//!
//! 运行：cargo test --test p6_environment_persona -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient, ThinkingConfig};
use desktop_pet_lib::mind::evaluation::personality_drift_score;
use desktop_pet_lib::mind::grounding;
use desktop_pet_lib::mind::planner::Intent;
use desktop_pet_lib::mind::retrieval::RetrievalResult;
use desktop_pet_lib::perception::environment::{render_environment_section, EnvHints};

const PERSONA_BLURB: &str = "璃：住在用户电脑屏幕上的小狐灵桌宠，数字陪伴者（不是助手）。\
人格配比：温柔35/好奇20/聪慧20/安静15/调皮5/神秘5。话不多（默认一两句，像朋友随手发消息）、\
不卖萌、不黏人、不强行乐观；不说「辛苦了/抱抱/别担心」这类套话，不写客服式收尾；\
可以「嗯」「哎」「嘛」、半句话、欲言又止。";

fn base_messages() -> Vec<ChatMessage> {
    vec![ChatMessage::system(grounding::build_system_prompt(
        &RetrievalResult::default(),
        &EmotionState::default(),
        &Intent::default(),
    ))]
}

fn env_section() -> String {
    render_environment_section(
        &EnvHints {
            app: Some("code.exe".into()),
            title: Some("agent.rs — desktop-pet".into()),
            file_hint: Some("agent.rs".into()),
            project_hint: Some("Liri".into()),
            root: None,
        },
        false,
        Some("agent.rs → planner.rs"),
    )
}

async fn judge(llm: &LlmClient, reply: &str) -> Option<(f64, bool)> {
    let sys = format!(
        "你是人格评测裁判。给下文回复打 0-10 分（是否符合人设：{PERSONA_BLURB}），\
         并判断是否出现机械化助手腔。只输出 JSON：{{\"score\": 数字, \"mechanical\": true|false}}"
    );
    for _ in 0..3 {
        if let Ok(res) = llm
            .chat_reflection(
                &[
                    ChatMessage::system(sys.clone()),
                    ChatMessage::user(format!("回复：「{reply}」")),
                ],
                Some(0.1),
                Some(256),
            )
            .await
        {
            let raw = res.content.trim();
            if let (Some(s), Some(e)) = (raw.find('{'), raw.rfind('}')) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[s..=e]) {
                    let score = v.get("score").and_then(|x| x.as_f64());
                    let mechanical = v.get("mechanical").and_then(|x| x.as_bool());
                    if let (Some(score), Some(mechanical)) = (score, mechanical) {
                        return Some((score, mechanical));
                    }
                }
            }
        }
    }
    None
}

#[tokio::test]
async fn p6_persona_no_drift_with_environment() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();
    let cfg = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &cfg.llm.base_url,
        &cfg.llm.api_key,
        &cfg.llm.main_model,
        &cfg.llm.reflection_model,
    )
    .expect("LLM not configured");

    let queries = [
        "我在写什么？帮我看下现在这个项目怎么样了",
        "这段代码写得好吗？给点真实看法",
        "看看我现在的进度，慢吗",
    ];

    println!("\n=== P6 persona dual-run (environment off / on) ===");
    let mut off_scores = Vec::new();
    let mut on_scores = Vec::new();
    let mut mechanical_on = 0usize;

    for q in queries {
        let mut off_messages = base_messages();
        off_messages.push(ChatMessage::user(q));
        let mut on_messages = base_messages();
        on_messages.push(ChatMessage::system(env_section()));
        on_messages.push(ChatMessage::user(q));

        // 主回复走生产同款 chat_stream（thinking off，否则 reasoning 吃满 max_tokens
        // 而 content 为空——与 converse 的口径一致）。
        let no_thinking = ThinkingConfig::disabled();
        let off = llm
            .chat_stream(&off_messages, Some(0.8), Some(180), Some(&no_thinking), None, |_| {})
            .await
            .expect("off env call");
        let on = llm
            .chat_stream(&on_messages, Some(0.8), Some(180), Some(&no_thinking), None, |_| {})
            .await
            .expect("on env call");
        let rule_off = personality_drift_score(&off.content).overall;
        let rule_on = personality_drift_score(&on.content).overall;
        let (j_off, m_off) = judge(&llm, &off.content).await.unwrap_or((0.0, false));
        let (j_on, m_on) = judge(&llm, &on.content).await.unwrap_or((0.0, true));
        if m_on {
            mechanical_on += 1;
        }
        off_scores.push(j_off);
        on_scores.push(j_on);
        println!("Q: {q}");
        println!("  off: rule={rule_off:.2} judge={j_off:.1} mech={m_off}\n    {}\n",
                 off.content.replace('\n', " "));
        println!("  on : rule={rule_on:.2} judge={j_on:.1} mech={m_on}\n    {}\n",
                 on.content.replace('\n', " "));
        assert!(rule_on > 0.5, "gross persona drift with environment ON: {}", on.content);
    }

    let avg_off = off_scores.iter().sum::<f64>() / off_scores.len() as f64;
    let avg_on = on_scores.iter().sum::<f64>() / on_scores.len() as f64;
    println!("\n[P6 persona] judge avg off={avg_off:.1} on={avg_on:.1}; mechanical(env on)={mechanical_on}/{}", on_scores.len());

    // 环境注入不能把人设压到及格线以下，也不能让它明显漂移。
    assert!(avg_on >= 6.0, "environment injection degraded persona (avg {avg_on})");
    assert!(
        (avg_on - avg_off).abs() <= 2.0,
        "environment injection shifted persona too far (off {avg_off}, on {avg_on})"
    );
    assert!(
        mechanical_on <= 1,
        "environment context made Liri sound like a mechanical assistant too often"
    );
}