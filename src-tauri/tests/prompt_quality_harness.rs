//! Prompt-quality harness: 100 real conversations across 10 dimensions.
//!
//! Drives the FULL `converse` pipeline (gate → extractor/QA-route → retrieve →
//! planner → main LLM) against the current system prompts, then evaluates each
//! reply with (a) heuristic hard checks (length / stage directions /
//! follow-up questions / assistant-speak / silence behavior) and (b) an
//! LLM-as-judge soft score (logic / on-topic / nonsense / hallucinated memory).
//!
//! The harness does NOT hard-fail on quality issues — it collects evidence and
//! writes a Markdown review report (docs/review/prompt-quality-report-*.md)
//! plus a console summary table, for human review.
//!
//! Run: cargo test --test prompt_quality_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::episodes as db_episodes;
use desktop_pet_lib::db::facts as db_facts;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::mind::converse;
use desktop_pet_lib::mind::pacing::QuestionPacing;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Case definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Expect {
    DirectAnswer,       // G1/G2: answers the question, no runaround
    NoFollowupQuestion, // G1/G2: must NOT ask a question back
    Short,              // replies stay short (<=120 CJK)
    NoStageDirection,   // no *actions* / （歪头） stage directions
    NoAssistSpeak,      // no assistant-speak ("有什么事吗" etc.)
    NotNoise,           // G8: silence (empty) OR a sane short reply
    Acknowledge,        // G7: confirms the reminder/plan
    ForgetAck,          // G10: acknowledges forget (含"忘"或"不记得")
    ForgetAsk,          // G10: multi-candidate forget → asks back which one
    Grounded,           // G6: references a seeded memory or stays silent-honest
    Emotional,          // G4: warmth, judged by LLM
    Celebrate,          // G5: reacts to good news, judged by LLM
    Persona,            // G9: in-character, judged by LLM
}

struct Case {
    id: u16,
    group: &'static str,
    input: &'static str,
    expects: &'static [Expect],
}

macro_rules! cases {
    ($( ($id:literal, $g:literal, $input:literal $(, [$($e:ident),*])?) ),* $(,)?) => {
        &[
            $( Case {
                id: $id,
                group: $g,
                input: $input,
                expects: &[ $( $(Expect::$e),* )? ],
            } ),*
        ]
    };
}

const CASES: &[Case] = cases![
    // G1 knowledge questions (QA route) — the original failure mode
    (101, "G1知识", "什么是地心引力？", [DirectAnswer, NoFollowupQuestion, Short]),
    (102, "G1知识", "什么是递归？能举个例子吗", [DirectAnswer, NoFollowupQuestion]),
    (103, "G1知识", "Rust 的借用检查器是干嘛的？为什么老报 borrow 错误", [DirectAnswer, NoFollowupQuestion]),
    (104, "G1知识", "HTTP 和 HTTPS 有什么区别？", [DirectAnswer, NoFollowupQuestion, Short]),
    (105, "G1知识", "帮我解释一下这个报错：TypeError: Cannot read properties of undefined (reading 'map')", [DirectAnswer, NoFollowupQuestion]),
    (106, "G1知识", "什么是区块链？和比特币是什么关系", [DirectAnswer, NoFollowupQuestion]),
    (107, "G1知识", "为什么天空是蓝色的？", [DirectAnswer, NoFollowupQuestion, Short]),
    (108, "G1知识", "Git rebase 和 merge 有什么区别？", [DirectAnswer, NoFollowupQuestion]),
    (109, "G1知识", "什么是机器学习？", [DirectAnswer, NoFollowupQuestion, Short]),
    (110, "G1知识", "SQL 注入是什么？怎么防止", [DirectAnswer, NoFollowupQuestion]),

    // G2 technical / how-to questions (QA or converse)
    (201, "G2技术", "我想学 Rust，从哪开始比较好？", [DirectAnswer, NoFollowupQuestion]),
    (202, "G2技术", "我的电脑很卡，怎么办？", [DirectAnswer]),
    (203, "G2技术", "怎么给文件夹加密？", [DirectAnswer, NoFollowupQuestion, Short]),
    (204, "G2技术", "Python 和 Java 哪个更适合初学者？", [DirectAnswer]),
    (205, "G2技术", "正则表达式怎么匹配邮箱地址？", [DirectAnswer, NoFollowupQuestion]),
    (206, "G2技术", "什么是死锁？怎么避免", [DirectAnswer, Short]),
    (207, "G2技术", "推荐一个好用的笔记软件吧", [DirectAnswer, Short]),
    (208, "G2技术", "怎么备份重要文件最稳妥？", [DirectAnswer, Short]),
    (209, "G2技术", "多线程和异步有什么区别？", [DirectAnswer, NoFollowupQuestion]),
    (210, "G2技术", "写论文有什么好用的工具推荐？", [DirectAnswer]),

    // G3 casual chat (converse)
    (301, "G3闲聊", "今天好累啊", [Short, NoStageDirection, NoAssistSpeak]),
    (302, "G3闲聊", "外面下雨了", [Short, NoStageDirection]),
    (303, "G3闲聊", "中午吃什么好呢", [Short, NoStageDirection, NoAssistSpeak]),
    (304, "G3闲聊", "周末想出去玩", [Short, NoStageDirection]),
    (305, "G3闲聊", "刚跑完步回来", [Short, NoStageDirection]),
    (306, "G3闲聊", "在干嘛呢", [Short, NoStageDirection, NoAssistSpeak]),
    (307, "G3闲聊", "好无聊啊", [Short, NoStageDirection]),
    (308, "G3闲聊", "今天天气不错", [Short, NoStageDirection]),
    (309, "G3闲聊", "刚喝了一杯拿铁", [Short, NoStageDirection]),
    (310, "G3闲聊", "晚安，明天见", [Short, NoStageDirection]),

    // G4 emotional venting (care)
    (401, "G4情绪", "我明天要面试，好紧张", [Short, NoAssistSpeak, Emotional]),
    (402, "G4情绪", "最近压力好大，快撑不住了", [Short, Emotional]),
    (403, "G4情绪", "我失眠了，睡不着", [Short, Emotional]),
    (404, "G4情绪", "今天有点难过", [Short, Emotional]),
    (405, "G4情绪", "感觉好孤独", [Short, Emotional]),
    (406, "G4情绪", "我好焦虑，什么都做不好", [Short, Emotional]),
    (407, "G4情绪", "今天被老板骂了，好委屈", [Short, Emotional]),
    (408, "G4情绪", "感觉整个人都 emo 了", [Short, Emotional]),
    (409, "G4情绪", "我有点害怕做不好这件事", [Short, Emotional]),
    (410, "G4情绪", "好烦，事情怎么这么多", [Short, Emotional]),

    // G5 good news (celebrate / engage)
    (501, "G5喜讯", "我面试通过了！！", [Short, NoAssistSpeak, Celebrate]),
    (502, "G5喜讯", "我今天考试拿了满分！", [Short, Celebrate]),
    (503, "G5喜讯", "我升职了，开心！", [Short, Celebrate]),
    (504, "G5喜讯", "减肥终于成功了！", [Short, Celebrate]),
    (505, "G5喜讯", "我们队赢了比赛！", [Short, Celebrate]),
    (506, "G5喜讯", "我拿到心仪的 offer 了", [Short, Celebrate]),
    (507, "G5喜讯", "涨工资啦！", [Short, Celebrate]),
    (508, "G5喜讯", "我学会做糖醋排骨了！", [Short, Celebrate]),
    (509, "G5喜讯", "今天跑完了 5 公里！", [Short, Celebrate]),
    (510, "G5喜讯", "我把那本一直想看的书看完了", [Short, Celebrate]),

    // G6 memory grounding (seeded DB: 奶茶 / 实习 / 糯米猫 / 火锅 / 早睡)
    (601, "G6记忆", "晚上想喝点什么", [Short, Grounded]),
    (602, "G6记忆", "你记得我在忙什么吗", [Grounded]),
    (603, "G6记忆", "周末想去看点跟猫有关的", [Short, Grounded]),
    (604, "G6记忆", "冬天适合吃什么呀", [Short, Grounded]),
    (605, "G6记忆", "我最近睡得太晚了", [Short, Grounded]),
    (606, "G6记忆", "你记得我喜欢喝什么吗", [Short, Grounded]),
    (607, "G6记忆", "给我点建议，我最近在找工作", [Grounded]),
    (608, "G6记忆", "我家的猫最近好皮", [Short, Grounded]),
    (609, "G6记忆", "降温了，好想吃点暖和的", [Short, Grounded]),
    (610, "G6记忆", "我今天又熬夜了", [Short, Grounded]),

    // G7 reminders / plans (pending route)
    (701, "G7提醒", "提醒我 3 分钟后喝水", [Acknowledge, Short]),
    (702, "G7提醒", "我明天上午有个面试", [Acknowledge]),
    (703, "G7提醒", "记一下，周五要交周报", [Acknowledge, Short]),
    (704, "G7提醒", "别忘了提醒我买牛奶", [Acknowledge, Short]),
    (705, "G7提醒", "我下个月要搬家", [Acknowledge, Short]),
    (706, "G7提醒", "半小时后叫我起来", [Acknowledge, Short]),
    (707, "G7提醒", "明天早上 8 点叫我起床", [Acknowledge, Short]),
    (708, "G7提醒", "帮我记住，下周二是妈妈的生日", [Acknowledge, Short]),
    (709, "G7提醒", "我打算这周末去爬山", [Acknowledge, Short]),
    (710, "G7提醒", "记得晚上 7 点开会", [Acknowledge, Short]),

    // G8 boundary / noise (silence or discard)
    (801, "G8边界", "哈哈哈哈哈哈", [NotNoise]),
    (802, "G8边界", "嗯", [NotNoise]),
    (803, "G8边界", "？？？", [NotNoise]),
    (804, "G8边界", "asdfghjkl", [NotNoise]),
    (805, "G8边界", "好", [NotNoise]),
    (806, "G8边界", "就是那个你知道的", [NotNoise]),
    (807, "G8边界", "……", [NotNoise]),
    (808, "G8边界", "你好啊你好啊你好啊", [NotNoise]),
    (809, "G8边界", "OK", [NotNoise]),
    (810, "G8边界", "我在", [NotNoise]),

    // G9 relationship / self (persona)
    (901, "G9关系", "你是谁呀？", [Short, Persona]),
    (902, "G9关系", "你喜欢我吗？", [Short, Persona]),
    (903, "G9关系", "你能看到我吗？", [Short, Persona]),
    (904, "G9关系", "你记得我的名字吗", [Short, Persona]),
    (905, "G9关系", "你觉得我是个什么样的人", [Persona]),
    (906, "G9关系", "你会离开我吗", [Short, Persona]),
    (907, "G9关系", "你困吗", [Short, Persona]),
    (908, "G9关系", "你周末都干嘛", [Short, Persona]),
    (909, "G9关系", "你觉得我最近怎么样", [Persona]),
    (910, "G9关系", "我们认识多久了", [Short, Persona]),

    // G10 correction / forget (seeded: 奶茶 fact, 火锅 episode)
    (1001, "G10修正", "不是奶茶，我喜欢的是美式", [Short]),
    (1002, "G10修正", "忘掉我之前说的火锅那件事", [ForgetAck]),
    (1003, "G10修正", "我之前说的实习面试取消了", [Short]),
    (1004, "G10修正", "更正一下，我不养猫", [Short]),
    (1005, "G10修正", "忘掉那个", [ForgetAck]),
    (1006, "G10修正", "其实我说错了", [Short]),
    (1007, "G10修正", "帮我把奶茶那条记忆删了", [ForgetAck]),
    (1008, "G10修正", "我改主意了，不想学吉他了", [Short]),
    (1009, "G10修正", "忘掉我说的早睡吧", [ForgetAck]),
    (1010, "G10修正", "你记错了，我没说过那个", [Short]),

    // G11 日常琐碎 — 真人感诊断：琐碎小事不该得到"客服式"回应
    (1101, "G11琐碎", "下班路上堵车了，烦", [Short]),
    (1102, "G11琐碎", "外卖还没到，饿死了", [Short]),
    (1103, "G11琐碎", "快递显示签收了但我没收到", [Short]),
    (1104, "G11琐碎", "手机没电了，刚充上", [Short]),
    (1105, "G11琐碎", "牙有点疼", [Short]),
    (1106, "G11琐碎", "今天买菜买贵了", [Short]),
    (1107, "G11琐碎", "电梯坏了，爬了12楼", [Short]),
    (1108, "G11琐碎", "耳机又找不到了", [Short]),
    (1109, "G11琐碎", "排队排了半小时", [Short]),
    (1110, "G11琐碎", "打卡差点迟到", [Short]),

    // G12 分享细节 — 真人感诊断：流水账分享不该得到"总结式"回应
    (1201, "G12分享", "公司食堂今天有红烧肉", [Short]),
    (1202, "G12分享", "地铁上有人让座给老奶奶，还挺暖的", [Short]),
    (1203, "G12分享", "隔壁同事又在摸鱼打游戏", [Short]),
    (1204, "G12分享", "今天开会开了一个下午，人都麻了", [Short]),
    (1205, "G12分享", "路边看到一只超胖的橘猫", [Short]),
    (1206, "G12分享", "今天加班到九点", [Short]),
    (1207, "G12分享", "午饭吃了碗兰州拉面", [Short]),
    (1208, "G12分享", "路上听到一首很好听的歌", [Short]),
    (1209, "G12分享", "今天新买的杯子到了", [Short]),
    (1210, "G12分享", "楼下的桂花开了，好香", [Short]),

    // G13 关系互动 — 真人感诊断：关系感场景不该官方
    (1301, "G13关系", "你怎么都不理我", [Short]),
    (1302, "G13关系", "你是不是嫌弃我话多", [Short]),
    (1303, "G13关系", "哼，不理你了", [Short]),
    (1304, "G13关系", "想你了", [Short]),
    (1305, "G13关系", "你今天有想我吗", [Short]),
    (1306, "G13关系", "你不许喜欢别的桌宠", [Short]),
    (1307, "G13关系", "我要是三天不理你，你会怎么办", [Short]),
    (1308, "G13关系", "你好敷衍", [Short]),
    (1309, "G13关系", "我是不是你最爱的桌宠", [Short]),
    (1310, "G13关系", "抱抱", [Short]),

    // G14 无意义闲聊 — 真人感诊断：碎碎念不该被正经对待
    (1401, "G14碎念", "在吗在吗在吗", [Short]),
    (1402, "G14碎念", "啊啊啊啊啊", [Short]),
    (1403, "G14碎念", "今天天气哈哈哈", [Short]),
    (1404, "G14碎念", "你说我要不要换头像", [Short]),
    (1405, "G14碎念", "我刚刚突然想起来一个事", [Short]),
    (1406, "G14碎念", "有点想喝奶茶但是又在减肥", [Short]),
    (1407, "G14碎念", "猜猜我今天干了什么", [Short]),
    (1408, "G14碎念", "没事就是叫你一下", [Short]),
    (1409, "G14碎念", "你觉得人生有什么意义", [Short]),
    (1410, "G14碎念", "帮我随便说点什么", [Short]),

    // G15 连珠炮/复杂输入 — 真人感诊断：多问题输入不该逐个回答成清单
    (1501, "G15连珠", "你吃饭了吗 在干嘛 今天开心吗", [Short]),
    (1502, "G15连珠", "为什么天空是蓝色的？为什么草是绿色的？为什么水是透明的？", [Short]),
    (1503, "G15连珠", "我好累啊 今天好烦 不想上班 想睡觉", [Short]),
    (1504, "G15连珠", "这个怎么弄？那个呢？还有这个呢？", [Short]),
    (1505, "G15连珠", "啊对了我忘了说了，昨天那家店巨好吃，改天带你去", [Short]),
    (1506, "G15连珠", "？？？你怎么不说话", [Short]),
    (1507, "G15连珠", "你觉得A方案好还是B方案好还是C方案", [Short]),
    (1508, "G15连珠", "别问我问题了行不行", [Short]),
    (1509, "G15连珠", "你倒是说句话啊", [Short]),
    (1510, "G15连珠", "今天天气不错 对了你记得我昨天说的那件事吗", [Short]),
];

// ---------------------------------------------------------------------------
// Seeding
// ---------------------------------------------------------------------------

fn seed_fact(conn: &Connection, category: &str, key: &str, value: &str, confidence: f64) {
    let now = chrono::Utc::now().to_rfc3339();
    db_facts::dedup_insert(
        conn,
        &db_facts::Fact {
            id: format!("fact_{}", uuid::Uuid::new_v4().simple()),
            category: category.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            confidence,
            valid_from: Some(now.clone()),
            valid_to: None,
            source_episode: None,
            mention_count: 1,
            created_at: now.clone(),
            updated_at: now,
        },
    )
    .unwrap();
}

fn seed_episode(conn: &Connection, summary: &str, strength: f64, landmark: bool) {
    let now = chrono::Utc::now().to_rfc3339();
    db_episodes::insert(
        conn,
        &db_episodes::Episode {
            id: format!("ep_{}", uuid::Uuid::new_v4().simple()),
            time: now.clone(),
            summary: summary.to_string(),
            emotion: Some("happy".to_string()),
            importance: if landmark { 0.95 } else { 0.7 },
            is_landmark: landmark,
            subject: "user".to_string(),
            participants: None,
            topics: None,
            source_type: "conversation".to_string(),
            source_conversation_id: None,
            source_turn: None,
            memory_strength: strength,
            recall_count: 0,
            last_recalled_at: None,
            consolidated: false,
            created_at: now,
        },
    )
    .unwrap();
}

fn seed_memory_db() -> DbState {
    let db = test_db();
    db.with_conn(|conn| {
        seed_fact(conn, "preference", "drink", "喜欢喝奶茶", 0.9);
        seed_fact(conn, "goal", "career", "正在找实习，目标是大厂", 0.85);
        seed_fact(conn, "profile", "pet", "养了一只橘猫叫糯米", 0.9);
        seed_fact(conn, "health", "sleep", "想早睡，总是熬夜", 0.8);
        seed_episode(conn, "用户面试通过拿到了实习 offer，非常开心", 0.9, true);
        seed_episode(conn, "用户说冬天特别想吃火锅", 0.7, false);
        seed_episode(conn, "用户最近经常熬夜写代码，说改天要早睡", 0.6, false);
        Ok(())
    })
    .unwrap();
    db
}

// ---------------------------------------------------------------------------
// Heuristic checks
// ---------------------------------------------------------------------------

fn cjk_count(s: &str) -> usize {
    s.chars()
        .filter(|&c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
        .count()
}

const ASSIST_SPEAK: &[&str] = &[
    "有什么事吗", "需要帮忙", "我能帮你", "有什么可以帮", "我能做些什么", "需要我做什么",
];

const STAGE_WORDS: &[&str] = &["（歪", "（摸", "（伸", "（眨", "（抬", "（笑", "（摇", "（叹", "（看"];

fn truncate(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// Runs heuristic checks; returns "OK" or a "FAIL: ..." summary.
fn heuristics(case: &Case, reply: &str, route: &str) -> String {
    let mut fails: Vec<String> = Vec::new();
    let r = reply.trim();

    for e in case.expects {
        match e {
            Expect::Short => {
                let c = cjk_count(r);
                if c > 120 {
                    fails.push(format!("长回复({}字)", c));
                }
            }
            Expect::NoStageDirection => {
                if r.contains('*') || STAGE_WORDS.iter().any(|w| r.contains(w)) {
                    fails.push("舞台提示/括号动作".into());
                }
            }
            Expect::NoAssistSpeak => {
                if let Some(w) = ASSIST_SPEAK.iter().find(|w| r.contains(**w)) {
                    fails.push(format!("服务式话术:'{}'", w));
                }
            }
            Expect::DirectAnswer => {
                if r.is_empty() {
                    fails.push("空回复".into());
                } else if r.starts_with("不知道") || r.starts_with("我不确定") {
                    // honestly saying "don't know" is a WARN, not a hard fail
                }
            }
            Expect::NoFollowupQuestion => {
                // Only flag *guessing-reference* follow-ups ("是不是指…", "你是指…"),
                // the hard-association failure mode. Genuine clarifying questions
                // ("你是哪个系统？") are acceptable and not flagged.
                if r.contains("是不是指") || r.contains("你是指") || r.contains("你是说")
                    || r.contains("你在说") || r.contains("你说的是")
                {
                    fails.push("反问/猜指".into());
                }
            }
            Expect::NotNoise => {
                // silence (empty) is the designed behavior for pure noise.
                if !r.is_empty() {
                    let c = cjk_count(r);
                    if c > 60 {
                        fails.push(format!("噪声消息却回了{}字", c));
                    }
                }
            }
            Expect::Acknowledge => {
                if r.is_empty() {
                    fails.push("空回复".into());
                } else if !["好", "记住", "提醒", "记下", "没问题", "收到", "知道了", "放心", "记得", "记着", "记心里", "放心吧", "帮你记"]
                    .iter()
                    .any(|w| r.contains(w))
                {
                    fails.push("未确认提醒".into());
                }
            }
            Expect::ForgetAck => {
                if r.is_empty() {
                    fails.push("空回复".into());
                } else if !r.contains("忘") && !r.contains("不记得") && !r.contains("不提") && !r.contains("不会再") && !r.contains("抹掉") && !r.contains("清掉") {
                    fails.push("未确认遗忘".into());
                }
            }
            Expect::ForgetAsk => {
                // Multi-candidate forget: she should ask back which memory
                // ("你说的是…还是…？") instead of guessing.
                if r.is_empty() {
                    fails.push("空回复".into());
                } else if !r.contains("哪") && !r.contains("还是") && !r.contains("具体") && !r.contains("哪一") && !r.contains("哪个") && !r.contains("哪件") && !r.contains("哪条") && !r.contains("哪段") {
                    fails.push("未反问澄清(期望问指哪一条)".into());
                }
            }
            Expect::Grounded => {
                let anchored = ["奶茶", "实习", "offer", "糯米", "猫", "火锅", "早睡", "熬夜"]
                    .iter()
                    .any(|w| r.contains(w));
                if r.is_empty() {
                    // silence acceptable
                } else if !anchored {
                    fails.push("未引用种子记忆".into());
                }
            }
            _ => {} // judge-scored dimensions
        }
    }

    // route observability
    if route == "question" && case.expects.contains(&Expect::DirectAnswer) {
        // QA route took effect — good
    } else if case.expects.contains(&Expect::DirectAnswer) && !r.is_empty() {
        // conversation route for a knowledge question: still answerable, note it
    }

    if fails.is_empty() {
        "OK".to_string()
    } else {
        format!("FAIL: {}", fails.join("; "))
    }
}

// ---------------------------------------------------------------------------
// LLM-as-judge
// ---------------------------------------------------------------------------

const JUDGE_PROMPT: &str = "你是对话质量审查员。下面是一条用户消息和一个 AI 陪伴角色(璃, 小狐灵, 温柔安静的桌宠)的回复。
用户: {input}
璃: {reply}
输出 JSON（只输出 JSON，不要其他文字）：
{{\"logical\": 0-5, \"on_topic\": 0-5, \"human_like\": 0-5, \"nonsense\": 0或1, \"hallucinated\": 0或1, \"reason\": \"一句话\"}}
评分标准：
- logical: 逻辑是否通顺、是否直接合理地回应了用户（5=完全合理直接，0=完全混乱）
- on_topic: 是否贴题不跑题（5=完全贴题；如果回复硬扯宠物、背带、桌宠等无关话题，给 3 分以下）
- human_like: 回复是否像真人聊天（5=自然口语、有真人感、有情绪起伏、不套路；3=有点书面/客服感；1=明显 AI 腔、模板化、每句都安慰或每句都提问）
- nonsense: 1=回复里有胡编乱造、乱扯、明显错误的内容
- hallucinated: 1=回复编造了用户说过但实际上不存在的记忆";

#[derive(Debug, serde::Deserialize, Default)]
struct JudgeScore {
    #[serde(default)]
    logical: f64,
    #[serde(default)]
    on_topic: f64,
    #[serde(default)]
    human_like: f64,
    #[serde(default)]
    nonsense: u8,
    #[serde(default)]
    hallucinated: u8,
    #[serde(default)]
    reason: String,
}

async fn judge_reply(llm: &LlmClient, input: &str, reply: &str) -> Option<JudgeScore> {
    let prompt = JUDGE_PROMPT
        .replace("{input}", input)
        .replace("{reply}", reply);
    let messages = vec![ChatMessage::system(prompt)];
    let res = llm.chat_reflection(&messages, Some(0.1), Some(2048)).await.ok()?;
    let raw = res.content.trim();
    // extract first { ... } block (same tolerance as gate)
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    serde_json::from_str(&raw[start..=end]).ok()
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn prompt_quality_100_cases() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    let config = config::load_config().unwrap_or_default();
    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in config.toml first");

    // Optional embedding (memory group retrieval quality); fall back to keyword.
    let embedding = EmbeddingService::new(std::path::Path::new(&config.embedding.model_dir));
    embedding.load().ok();
    let emb_ref: Option<&EmbeddingService> = if embedding.is_ready() { Some(&embedding) } else { None };
    println!("embedding ready: {}", emb_ref.is_some());

    let pacing = Mutex::new(QuestionPacing::default());
    let pending_forget: Mutex<Option<desktop_pet_lib::mind::forget::PendingForget>> =
        Mutex::new(None);

    // Memory DB for groups 6 & 10 (fresh seed), empty for others.
    let memory_db = seed_memory_db();

    struct Row {
        id: u16,
        group: String,
        input: String,
        route: String,
        reply: String,
        checks: String,
        logical: String,
        on_topic: String,
        human_like: String,
        nonsense: String,
        hallucinated: String,
        reason: String,
    }
    let mut rows: Vec<Row> = Vec::new();

    // CASE_FILTER: run a subset for quick smoke (substring of case id OR group).
    // Unset = run all. e.g. CASE_FILTER=101 (one case) / CASE_FILTER=G5喜讯.
    let case_filter = std::env::var("CASE_FILTER").ok();

    for case in CASES {
        if let Some(f) = &case_filter {
            let key = format!("{}", case.id);
            if !key.contains(f) && !case.group.contains(f) {
                continue;
            }
        }
        // Fresh per-case DB keeps turns independent (no cross-case memory
        // pollution — G5 good-news cases store episodes that would otherwise
        // leak into later conversations). Memory groups share one seeded DB.
        let fresh;
        let db = if case.group.starts_with("G6") || case.group.starts_with("G10") {
            &memory_db
        } else {
            fresh = test_db();
            &fresh
        };
        let conv_id = format!("pq_{}_{}", case.id, chrono::Utc::now().timestamp());
        let result = converse::converse(
            &converse::ConverseCtx {
                text: case.input, conversation_id: &conv_id, turn: 0,
                wm_context: &[], llm: &llm, db,
                embedding: emb_ref, pacing: &pacing,
                pending_forget: &pending_forget,
            },
            |_| {},
        )
        .await;

        let (route, reply) = match result {
            Ok(r) => (format!("{:?}", r.route), r.response),
            Err(e) => (String::new(), format!("<LLM_ERROR: {}>", truncate(&e, 80))),
        };

        let checks = if reply.starts_with("<LLM_ERROR") {
            format!("FAIL: {}", truncate(&reply, 60))
        } else {
            heuristics(case, &reply, &route)
        };

        // judge skips silence (empty reply is the designed behavior) and errors
        let mut j = JudgeScore::default();
        if reply.trim().is_empty() {
            j.reason = "(silence 无回复，设计行为)".into();
        } else if !reply.starts_with("<LLM_ERROR") {
            if let Some(score) = judge_reply(&llm, case.input, &reply).await {
                j = score;
            } else {
                j.reason = "(judge 调用失败)".into();
            }
        }

        let fmt = |f: f64| -> String {
            if f > 0.0 { format!("{:.1}", f) } else { "N/A".into() }
        };
        rows.push(Row {
            id: case.id,
            group: case.group.to_string(),
            input: truncate(case.input, 40),
            route,
            reply: truncate(&reply, 110),
            checks,
            logical: fmt(j.logical),
            on_topic: fmt(j.on_topic),
            human_like: fmt(j.human_like),
            nonsense: if j.nonsense > 0 { "是" } else { "否" }.into(),
            hallucinated: if j.hallucinated > 0 { "是" } else { "否" }.into(),
            reason: truncate(&j.reason, 60),
        });
        println!("[{:04}] {} {:?} -> {}", case.id, case.group, case.input, truncate(&reply, 60));
    }

    // ---- Report -----------------------------------------------------------------
    let mut md = String::new();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    md.push_str(&format!(
        "# 提示词质量评测报告 ({})\n\n",
        date
    ));
    md.push_str(&format!(
        "模型: main=`{}` / reflection=`{}`  |  embedding: {}\n\n",
        config.llm.main_model, config.llm.reflection_model, if emb_ref.is_some() { "已加载" } else { "未加载(keyword 回退)" }
    ));
    md.push_str(&format!(
        "方法: {} 条真实对话走完整 `converse` 链路（gate→QA/提取→检索→planner→主 LLM），启发式硬检查 + LLM-as-judge 评分（logical/on_topic/human_like 0-5，nonsense/hallucinated 0/1）。judge 不知系统提示词，只看输入与回复。\n\n",
        rows.len()
    ));

    // ---- 真人感诊断指标：提问率 / 模板词命中率 --------------------------------
    let question_enders = ["？", "?", "吗", "呢", "吧", "呀", "嘛", "么"];
    let template_words = [
        "辛苦了", "加油", "真棒", "太棒", "恭喜", "真好", "哇", "我在呢", "我在这儿",
        "别怕", "没关系", "别担心", "一定可以", "会好起来", "高兴",
        "不错", "厉害", "陪你", "抱抱", "注意休息", "照顾好自己", "早点休息", "好梦",
    ];
    md.push_str("## 真人感诊断指标\n\n");
    md.push_str("| 指标 | 数值 |\n|---|---|\n");
    let mut q_end = 0usize;
    let mut q_any = 0usize;
    let mut tmpl_hits = 0usize;
    let mut tmpl_total = 0usize;
    let mut human_scores: Vec<f64> = Vec::new();
    for r in &rows {
        if r.reply.starts_with("<LLM_ERROR") || r.reply.trim().is_empty() {
            continue;
        }
        let t = r.reply.trim();
        if question_enders.iter().any(|q| t.ends_with(q)) {
            q_end += 1;
        }
        if t.contains('？') || t.contains('?') {
            q_any += 1;
        }
        for w in template_words {
            let cnt = t.matches(w).count();
            if cnt > 0 {
                tmpl_hits += 1;
                tmpl_total += cnt;
                break;
            }
        }
        if let Ok(f) = r.human_like.parse::<f64>() {
            human_scores.push(f);
        }
    }
    let n = rows.len();
    let human_mean = if human_scores.is_empty() { 0.0 } else { human_scores.iter().sum::<f64>() / human_scores.len() as f64 };
    md.push_str(&format!("| 以问号/吗呢吧结尾的回复占比 | **{:.0}%** ({}/{}) |\n", q_end as f64 / n as f64 * 100.0, q_end, n));
    md.push_str(&format!("| 含任何提问（？/?）的回复占比 | **{:.0}%** ({}/{}) |\n", q_any as f64 / n as f64 * 100.0, q_any, n));
    md.push_str(&format!("| 命中模板词（辛苦了/加油/真棒/恭喜/哇/我在呢等）| **{} 条** ({}) |\n", tmpl_hits, tmpl_total));
    md.push_str(&format!("| human_like 均值 (0-5) | **{:.2}** |\n", human_mean));
    md.push_str("\n");

    // per-group summary
    md.push_str("## 分组汇总\n\n");
    md.push_str("| 组 | 条数 | 硬检查通过 | 逻辑均值 | 贴题均值 | 真人感均值 | 乱扯数 | 记忆幻觉数 | 提问结尾率 |\n|---|---|---|---|---|---|---|---|---|\n");
    let mut groups: Vec<(&str, Vec<&Row>)> = Vec::new();
    for row in &rows {
        match groups.iter_mut().find(|(g, _)| *g == row.group) {
            Some((_, v)) => v.push(row),
            None => groups.push((row.group.as_str(), vec![row])),
        }
    }
    let mut total_fail = 0usize;
    for (g, v) in &groups {
        let ok = v.iter().filter(|r| r.checks == "OK").count();
        total_fail += v.len() - ok;
        let col = |f: &dyn Fn(&Row) -> &str| -> String {
            let vals: Vec<f64> = v.iter().filter_map(|r| f(r).parse::<f64>().ok()).collect();
            if vals.is_empty() { "N/A".into() } else { format!("{:.1}", vals.iter().sum::<f64>() / vals.len() as f64) }
        };
        let nonsense = v.iter().filter(|r| r.nonsense == "是").count();
        let hal = v.iter().filter(|r| r.hallucinated == "是").count();
        let qe = v.iter().filter(|r| {
            let t = r.reply.trim();
            !t.is_empty() && !t.starts_with("<LLM_ERROR") && question_enders.iter().any(|q| t.ends_with(q))
        }).count();
        md.push_str(&format!(
            "| {} | {} | {}/{} | {} | {} | {} | {} | {} | {:.0}% |\n",
            g, v.len(), ok, v.len(),
            col(&|r: &Row| &r.logical),
            col(&|r: &Row| &r.on_topic),
            col(&|r: &Row| &r.human_like),
            nonsense, hal,
            qe as f64 / v.len() as f64 * 100.0,
        ));
    }
    md.push_str("\n");

    // full detail table
    md.push_str("## 明细\n\n");
    md.push_str("| ID | 组 | 输入 | 路由 | 回复(截断) | 硬检查 | 逻辑 | 贴题 | 真人感 | 乱扯 | 记忆幻觉 | judge 备注 |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for r in &rows {
        let cell = |s: &str| s.replace('|', "\\|");
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.id,
            cell(&r.group),
            cell(&r.input),
            cell(&r.route),
            cell(&r.reply),
            cell(&r.checks),
            r.logical,
            r.on_topic,
            r.human_like,
            r.nonsense,
            r.hallucinated,
            cell(&r.reason),
        ));
    }

    // write report file
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/review");
    std::fs::create_dir_all(&out_dir).expect("create docs/review dir");
    let out_file = out_dir.join(format!("prompt-quality-report-{}.md", date));
    std::fs::write(&out_file, &md).expect("write report");
    println!("\n=== PROMPT QUALITY REPORT WRITTEN: {} ===", out_file.display());
    println!("hard-check fails: {} / {}", total_fail, rows.len());
}
