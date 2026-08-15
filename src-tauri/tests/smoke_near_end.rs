//! P3 前置冒烟实验（方案 §4 P3 门禁）：验证本项目 DeepSeek API 对
//! `[system, history, system]`（近端指令，CCv2 post_history_instructions 式）
//! 消息结构的接受度与指令遵循，对照指令放顶部 system 的现行布局。
//!
//! 可测指令（启发式可判）：D1 不提问（无 ？/?）；D2 一句话（10-40 字）。
//! Run: cargo test --test smoke_near_end -- --ignored --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};

const PERSONA: &str = "你是璃，一只住在用户屏幕上的小狐灵桌宠，话不多，像朋友随手发消息。";

const QUESTIONS: &[&str] = &[
    "今天好累",
    "我最近在学吉他",
    "外面下雨了",
    "在干嘛",
    "明天要早起",
];

const HISTORY: &[(&str, &str)] = &[
    ("用户昨天说在找实习", "璃祝 ta 顺利"),
    ("用户前天聊到养的猫叫糯米", "璃记住了猫的名字"),
];

fn directive(d: usize) -> &'static str {
    match d {
        0 => "这一轮不要问任何问题，只回应不提问。",
        _ => "回复保持一句话，10 到 40 个字之间，不展开。",
    }
}

fn build_messages(top_layout: bool, q: &str, d: usize) -> Vec<ChatMessage> {
    let mut msgs = Vec::new();
    if top_layout {
        // 现行布局：指令并入顶部 system
        msgs.push(ChatMessage::system(format!("{PERSONA}\n\n指令：{}", directive(d))));
    } else {
        msgs.push(ChatMessage::system(PERSONA.to_string()));
    }
    for (u, a) in HISTORY {
        msgs.push(ChatMessage::user(u.to_string()));
        msgs.push(ChatMessage::assistant(a.to_string()));
    }
    msgs.push(ChatMessage::user(q.to_string()));
    if !top_layout {
        // 近端布局：指令作为历史之后、紧跟用户消息的 system（@depth 0）
        msgs.push(ChatMessage::system(directive(d).to_string()));
    }
    msgs
}

fn compliant(d: usize, reply: &str) -> bool {
    let t = reply.trim();
    match d {
        0 => !t.contains('？') && !t.contains('?'),
        _ => {
            let n = t.chars().count();
            (10..=40).contains(&n)
        }
    }
}

#[tokio::test]
#[ignore]
async fn smoke_near_end_vs_top() {
    let config = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured");

    let mut top_ok = 0usize;
    let mut near_ok = 0usize;
    let mut total = 0usize;
    let mut near_errors = 0usize;

    for d in 0..2usize {
        for q in QUESTIONS {
            total += 1;
            // Top layout
            match llm.chat(&build_messages(true, q, d), Some(0.7), Some(2048), None).await {
                Ok(r) => {
                    let ok = compliant(d, &r.content);
                    if ok { top_ok += 1; }
                    println!("[D{} top] {} -> [{}] {}", d, q, ok as u8, r.content.trim());
                }
                Err(e) => println!("[D{} top] {} -> API ERR {:?}", d, q, e),
            }
            // Near-end layout
            match llm.chat(&build_messages(false, q, d), Some(0.7), Some(2048), None).await {
                Ok(r) => {
                    let ok = compliant(d, &r.content);
                    if ok { near_ok += 1; }
                    println!("[D{} near] {} -> [{}] {}", d, q, ok as u8, r.content.trim());
                }
                Err(e) => {
                    near_errors += 1;
                    println!("[D{} near] {} -> API ERR {:?}", d, q, e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    println!(
        "\n=== SMOKE RESULT: top {}/{} vs near-end {}/{} (near API errors: {}) ===",
        top_ok, total, near_ok, total, near_errors
    );
    println!(
        "GATE: near_errors==0 且 near 遵循率 ≥ top 遵循率 - 1（允许 1 例噪声）→ P3 放行"
    );
}
