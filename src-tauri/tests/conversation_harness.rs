//! Live conversation harness: drives the full `converse` pipeline against the
//! real configured LLM for many turns, logging per-turn input/output so we can
//! audit whether replies track the CURRENT question (root cause we fixed: the
//! current user message was never appended to the LLM messages array).
//!
//! Run: `cargo test --test conversation_harness -- --nocapture --test-threads=1`
//! Uses the REAL config + DB (same paths the app uses), so live persona data
//! is visible. Writes conversation_harness_report.txt with the full transcript.

use desktop_pet_lib::config;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::mind::converse;
use desktop_pet_lib::mind::pacing::QuestionPacing;
use desktop_pet_lib::mind::working::WorkingMemory;
use std::sync::Mutex;

/// (question, tag). Tags let us classify alignment. Many turns are designed to
/// expose "reply-to-wrong-turn": strong topic keywords, plus recall turns that
/// only succeed if the pet actually saw the prior fact-state turn.
const SCRIPT: &[(&str, &str)] = &[
    ("你好呀", "greeting"),
    ("现在几点了？", "time"),
    ("你今天心情怎么样？", "mood"),
    ("我叫什么名字你知道吗？", "recall_name"),
    ("你最喜欢什么颜色？", "color"),
    ("1加1等于几？", "math"),
    ("现在的时间是什么时候", "time"),
    ("你会做什么呀？", "ability"),
    ("北京是哪个国家的首都？", "geo"),
    ("我有点累了", "comfort"),
    ("你记得我吗？", "recall"),
    ("讲个笑话吧", "joke"),
    ("水的化学式是什么？", "science"),
    ("你觉得猫可爱吗？", "opinion"),
    ("帮我算一下 12 乘以 8", "math"),
    ("你几岁了？", "age"),
    ("今天天气怎么样？", "weather"),
    ("我很喜欢喝奶茶", "fact_state"),
    ("我刚刚说我喜欢喝什么？", "recall_milktea"),
    ("你是谁？", "identity"),
    ("再见啦", "farewell"),
    ("等等，我回来了", "return"),
    ("你觉得我这个人怎么样？", "opinion_user"),
    ("地球上最大的洋是哪个？", "geo"),
    ("我想吃火锅", "fact_state"),
    ("一百以内最大的质数是多少？", "math"),
    ("你会唱歌吗？", "ability"),
    ("你现在开心吗？", "mood"),
    ("太阳从哪个方向升起？", "science"),
    ("你能记住我说的话吗？", "ability"),
    ("我喜欢蓝色", "fact_state"),
    ("我最喜欢的颜色是什么？", "recall_color"),
    ("嗨", "greeting"),
    ("现在几点钟？", "time"),
    ("你刚才有没有认真听我说话？", "recall"),
    ("狗和猫你更喜欢哪个？", "opinion"),
    ("我工作好忙啊", "comfort"),
    ("三天后是星期几？", "time"),
    ("你相信外星人吗？", "opinion"),
    ("再来一个笑话", "joke"),
    ("我要睡觉了", "farewell"),
    ("月亮为什么会发光？", "science"),
    ("你还记得我刚才说我喜欢什么吗？", "recall_recent"),
    ("你最讨厌什么？", "opinion"),
    ("一打是多少个？", "math"),
    ("我好开心今天", "comfort"),
    ("你的梦想是什么？", "opinion"),
    ("帮我记一下：我最爱的电影是星际穿越", "fact_state"),
    ("我最爱的电影是什么？", "recall_movie"),
    ("谢谢你陪我聊天", "farewell"),
    ("最后再问我一遍我是谁来确认你还记得", "recall_name"),
];

#[tokio::test]
async fn run_50_turn_conversation() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    let config = config::load_config().unwrap_or_default();
    let db_path = config::resolve_db_path(&config);
    let db = DbState::open(&db_path).expect("open db");
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in config.toml first");

    let wm = Mutex::new(WorkingMemory::new());
    let pacing = Mutex::new(QuestionPacing::default());
    let pending_forget: Mutex<Option<desktop_pet_lib::mind::forget::PendingForget>> =
        Mutex::new(None);

    let mut aligned = 0usize;
    let mut misaligned = 0usize;
    let mut stage_dir_violations = 0usize;
    let mut log_lines: Vec<String> = Vec::new();

    for (i, (question, tag)) in SCRIPT.iter().enumerate() {
        let conversation_id = format!("harness_{}", chrono::Utc::now().timestamp());
        let wm_ctx = wm.lock().unwrap().get_context();

        let result = match converse::converse(
            &converse::ConverseCtx {
                text: question, conversation_id: &conversation_id, turn: i as i32,
                wm_context: &wm_ctx, llm: &llm, db: &db,
                embedding: None, pacing: &pacing,
                pending_forget: &pending_forget,
            },
            |_|{},
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                let line = format!(
                    "[{:02}/{}] tag={} Q={:?} -> ERROR: {}",
                    i + 1,
                    SCRIPT.len(),
                    tag,
                    question,
                    e
                );
                println!("{}", line);
                log_lines.push(line);
                misaligned += 1;
                continue;
            }
        };

        {
            let mut w = wm.lock().unwrap();
            w.push(ChatMessage {
                role: "user".to_string(),
                content: question.to_string(),
            });
            if !result.response.is_empty() {
                w.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: result.response.clone(),
                });
            }
        }

        let has_stage_dir = result.response.contains('（') || result.response.contains('(');
        if has_stage_dir {
            stage_dir_violations += 1;
        }

        let looks_stale = !matches!(*tag, "greeting" | "farewell" | "return")
            && (result.response.contains("你好")
                || result.response.contains("打招呼")
                || result.response.contains("新游戏"));
        if looks_stale {
            misaligned += 1;
        } else {
            aligned += 1;
        }

        let line = format!(
            "[{:02}/{}] tag={} Q={:?} -> A={:?}{} intent={} violations={}",
            i + 1,
            SCRIPT.len(),
            tag,
            question,
            result.response,
            if has_stage_dir { "  <<STAGE-DIR>>" } else { "" },
            result.intent.action,
            result.grounding_violations.len(),
        );
        println!("{}", line);
        log_lines.push(line);
    }

    let summary = format!(
        "\n=== SUMMARY: {} turns, aligned~={}, misaligned={}, stage_dir_violations={} ===",
        SCRIPT.len(),
        aligned,
        misaligned,
        stage_dir_violations
    );
    println!("{}", summary);

    std::fs::write(
        "conversation_harness_report.txt",
        format!("{}\n{}", log_lines.join("\n"), summary),
    )
    .ok();

    assert!(
        aligned as f32 / SCRIPT.len() as f32 >= 0.8,
        "alignment too low: {}/{} aligned",
        aligned,
        SCRIPT.len()
    );
    assert!(
        stage_dir_violations <= 3,
        "too many stage-direction violations: {}",
        stage_dir_violations
    );
}
