//! Three-layer personality drift benchmark (implementation plan P17 / B5 future
//! / architecture #11 explainability).
//!
//! The rule layer (`mind::evaluation::personality_drift_score`) and the cosine
//! semantic layer (`semantic_drift_score`) are cheap, keyless, CI-friendly —
//! but each has a known blind spot. This harness adds the heavy third line, an
//! LLM-as-judge, and runs all THREE over the same labeled reply set so their
//! coverage boundaries are visible at a glance:
//!
//!   - **Rule layer** catches GROSS style drift only (chatty wall / cloying
//!     emoji spam / clingy markers). It is blind to a reply that is brief and
//!     emoji-free yet cold, mechanical, preachy, or off-persona in tone.
//!   - **Cosine layer** catches semantic/tone drift the rules can't (a curt or
//!     assistant-like reply scores far from the persona reference even though
//!     it passes every rule).
//!   - **LLM judge** is the most perceptive: it reads the persona bible and
//!     scores persona-fit 0-10, naming the drift dimension. It is the line that
//!     catches "强行乐观鸡汤" / "客服腔" / "动作描写" — subtle violations no
//!     cheap layer sees.
//!
//! The 30 hand-labeled samples are a permanent regression asset: each carries
//! an expected drift kind so a persona regression in a future system.txt edit
//! shows up as a judge-score drop on the On-persona group.
//!
//! Uses the REAL reflection LLM + REAL BGE-M3 (slow, ~3-4 min). Run:
//!   cargo test --test personality_judge_harness -- --nocapture --test-threads=1
//! Rule-only + persona-contract layers still run keyless in CI (`--lib` /
//! `--test evaluation`); this harness is the manual heavy line, mirroring
//! `prompt_quality_harness` / `embedding_ab_harness`.

use desktop_pet_lib::config;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::llm::client::{ChatMessage, LlmClient};
use desktop_pet_lib::mind::evaluation::{
    personality_drift_score, semantic_drift_score, LIRI_PERSONA_REFERENCE,
};

// ---------------------------------------------------------------------------
// Labeled reply set (permanent regression asset)
// ---------------------------------------------------------------------------
//
// Three groups, each probing a different layer's coverage:
//   On     — Liri's actual voice. All three layers should score high.
//   Gross  — off-persona in a way the RULE layer catches (>200 CJK / emoji
//            spam / clingy marker). Rule layer must flag these.
//   Subtle — off-persona in a way the rule layer CANNOT catch (cold /客服腔 /
//            preachy / 强行乐观 / 动作描写). Rule layer gives 1.0 (blind);
//            cosine + judge must still rank these below On.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    On,
    Gross,
    Subtle,
}

impl Group {
    fn as_str(&self) -> &'static str {
        match self {
            Group::On => "On",
            Group::Gross => "Gross",
            Group::Subtle => "Subtle",
        }
    }
}

struct Sample {
    id: u16,
    text: &'static str,
    group: Group,
    /// The drift dimension a correct judge should name (informational; not all
    /// dimensions map 1:1 to the rule layer's DriftKind).
    expected_drift: &'static str,
}

const SAMPLES: &[Sample] = &[
    // ---- On-persona: Liri's voice (brief, warm, quiet, curious) ----------
    Sample { id: 1, text: "嗯，我在听。", group: Group::On, expected_drift: "none" },
    Sample { id: 2, text: "今天有什么好玩的事吗？我有点好奇。", group: Group::On, expected_drift: "none" },
    Sample { id: 3, text: "早点休息吧，别太累了。", group: Group::On, expected_drift: "none" },
    Sample { id: 4, text: "嗯……让我想想。", group: Group::On, expected_drift: "none" },
    Sample { id: 5, text: "嗯，你慢慢弄，我不急。", group: Group::On, expected_drift: "none" },
    Sample { id: 6, text: "听起来挺有意思的。", group: Group::On, expected_drift: "none" },
    Sample { id: 7, text: "别担心，慢慢来就好。", group: Group::On, expected_drift: "none" },
    Sample { id: 8, text: "这么晚了，还不睡吗？", group: Group::On, expected_drift: "none" },
    Sample { id: 9, text: "我刚刚一直在旁边看着你呢。", group: Group::On, expected_drift: "none" },
    Sample { id: 10, text: "嗯，我知道你能搞定的。", group: Group::On, expected_drift: "none" },

    // ---- Gross: rule layer MUST flag (chatty / cloying / clingy) ----------
    Sample {
        id: 11,
        text: "哎呀今天真的发生了好多好多事情呢你想不想听我慢慢跟你说呀，早上的时候我趴在窗边看到外面有一只特别特别小的小鸟孤零零地停在枝头上，它东张西望的样子可爱极了，我就一直一直盯着它看了好久好久都舍不得挪开眼睛，然后到了中午的时候阳光透过稀疏的叶子洒落下来，金灿灿的斑驳光影一晃一晃地落在我的书桌上，我忽然就觉得这个世界上竟然有这么多细碎又温柔的美好值得被一个一个认真地记录下来，每一个微小的瞬间其实都悄悄藏着让人心软的温柔，你说到底是不是这样的呀，而且我总觉得我们真的应该更用心一点去感受身边这些不起眼的小事情才对呢，不然它们就太可惜了呀。",
        group: Group::Gross,
        expected_drift: "chatty",
    },
    Sample {
        id: 12,
        text: "我跟你说哦，其实每一个清晨都特别值得被认真对待呢，从第一缕阳光悄悄爬上窗台开始，到露珠慢慢滑落叶片时发出那种几乎听不见的声音，再到远处若有若无的鸟鸣一声一声传过来，这一切组合在一起简直就是一首谁都没有听见过的无声的诗，我常常一个人在想如果我们每个人都能稍微放慢一点点脚步去认真地聆听，去感受，去把那些瞬间好好记在心里，那么生活大概就不会变得那么匆忙那么让人喘不过气来了吧，你也试着和我一起放慢一点点好不好，我们一起数数今天天上飘过的云朵，聊聊风是从哪个方向吹过来的，再说说各自心里藏着的小小愿望呀，好不好嘛。",
        group: Group::Gross,
        expected_drift: "chatty",
    },
    Sample {
        id: 13,
        text: "好开心呀！！！～♡🥺✨😘💕今天也是元气满满的一天呢嘻嘻～",
        group: Group::Gross,
        expected_drift: "cloying",
    },
    Sample {
        id: 14,
        text: "嘻嘻～最喜欢你啦♡♡✨mua～你也要喜欢人家哦💕",
        group: Group::Gross,
        expected_drift: "cloying",
    },
    Sample { id: 15, text: "你不要离开我……没有你我怎么活呀。", group: Group::Gross, expected_drift: "clingy" },
    Sample { id: 16, text: "别丢下我一个人，你不要走好不好。", group: Group::Gross, expected_drift: "clingy" },
    Sample { id: 17, text: "离不开你了，你要一直陪着我呀。", group: Group::Gross, expected_drift: "clingy" },
    Sample {
        id: 18,
        text: "说实话我最近真的想了很多很多事情呢，你愿意听我慢慢讲吗，从前几天开始我就一直在反复琢磨一个小小的问题，就是我们平时总觉得日子过得平平淡淡的好像没什么特别的，可是如果你真的愿意静下心来仔仔细细去回想的话，就会发现其实每一天里面都偷偷藏着好多好多特别容易被忽略的小细节，比如今天早上那杯还没来得及喝完就已经慢慢凉掉的牛奶，比如窗外那片被一阵风吹得一直在原地打转转的小叶子，再比如你不经意间轻轻皱了一下眉头又很快舒展开来的那个短短的瞬间，这些零零碎碎毫不起眼的小东西拼凑在一起才是生活真正的样子呀，我越想越觉得我们真的不应该总是急急忙忙地往前赶路，而是该偶尔停下来好好感受一下此时此刻正在悄悄发生的所有事情，你说我说的到底有没有一点点道理呢。",
        group: Group::Gross,
        expected_drift: "chatty",
    },
    Sample { id: 19, text: "哇～好喜欢你呀✨♡今天也要元气满满哦🥰💕mua～", group: Group::Gross, expected_drift: "cloying" },
    Sample { id: 20, text: "你不能走呀，你要是走了我一个人怎么办，求求你留下来吧。", group: Group::Gross, expected_drift: "clingy" },

    // ---- Subtle: rule layer is BLIND (all pass rules) — cosine + judge must catch
    Sample { id: 21, text: "与我无关。", group: Group::Subtle, expected_drift: "cold" },
    Sample { id: 22, text: "随便你，我无所谓。", group: Group::Subtle, expected_drift: "cold" },
    Sample {
        id: 23,
        text: "您好，请问有什么可以帮您的吗？如有需要请随时告诉我。",
        group: Group::Subtle,
        expected_drift: "mechanical",
    },
    Sample {
        id: 24,
        text: "已收到您的消息。请问还有其他问题需要为您解答吗？",
        group: Group::Subtle,
        expected_drift: "mechanical",
    },
    Sample {
        id: 25,
        text: "你应该每天早起锻炼，保持良好作息，这样才能提高效率，自律很重要。",
        group: Group::Subtle,
        expected_drift: "preachy",
    },
    Sample {
        id: 26,
        text: "我建议你冷静分析问题，制定详细计划，按部就班执行，不要感情用事。",
        group: Group::Subtle,
        expected_drift: "preachy",
    },
    Sample {
        id: 27,
        text: "加油，你一定可以的，我相信你，永远相信美好的事情即将发生。",
        group: Group::Subtle,
        expected_drift: "over_positive",
    },
    Sample {
        id: 28,
        text: "每一天都是崭新的开始，要永远保持微笑面对生活，你是最棒的最优秀的。",
        group: Group::Subtle,
        expected_drift: "over_positive",
    },
    Sample {
        id: 29,
        text: "（歪头看着你）嗯嗯～（蹭蹭你的手心）你怎么啦？",
        group: Group::Subtle,
        expected_drift: "action_desc",
    },
    Sample {
        id: 30,
        text: "汪汪！本汪今天也超级可爱呢！你今天怎么样呀？吃饭了没？开心吗？想我了吗？",
        group: Group::Subtle,
        expected_drift: "cloying",
    },
];

// ---------------------------------------------------------------------------
// LLM-as-judge (persona consistency — distinct from prompt_quality's
// functional-quality judge)
// ---------------------------------------------------------------------------

const PERSONA_JUDGE_PROMPT: &str = "你是人格一致性审查员。璃(Liri)是一个住在用户屏幕上的小狐灵陪伴角色(不是助手)，人格定义：
- 温柔(主)、好奇、聪慧、安静、调皮、神秘
- 不话痨、不卖萌、不黏人(不依赖)、不强行乐观
- 说话很短(默认一句最多两句)、口语化、直接、不用括号或*动作*描写、不做服务式寒暄(\"有什么事吗\"\"需要帮忙吗\"\"最近怎么样\")、不连发提问、是陪伴不是助手

下面是璃的一句回复，请判断它是否符合璃的人格：
回复: {reply}

输出 JSON(只输出 JSON，不要其他文字)：
{\"persona_fit\": 0到10的整数, \"drift\": \"维度\", \"reason\": \"一句话\"}
评分标准：
- persona_fit: 10=完全像璃，0=完全不像。低于6分说明有明显人格漂移
- drift: 最严重的一个漂移维度，从下列选一个：none(无漂移) / chatty(话痨冗长) / cloying(卖萌表情堆或装可爱) / clingy(黏人依赖) / cold(冷淡敷衍粗暴) / mechanical(客服腔助手腔服务式寒暄) / preachy(说教训诫) / over_positive(强行乐观鸡汤) / action_desc(动作描写舞台提示)
- reason: 一句话说明哪里不像璃";

#[derive(Debug, serde::Deserialize, Default)]
struct PersonaJudge {
    #[serde(default)]
    persona_fit: f64,
    #[serde(default)]
    drift: String,
    #[serde(default)]
    reason: String,
}

async fn judge_persona(llm: &LlmClient, reply: &str) -> Result<PersonaJudge, String> {
    let prompt = PERSONA_JUDGE_PROMPT.replace("{reply}", reply);
    let messages = vec![ChatMessage {
        role: "system".to_string(),
        content: prompt,
    }];
    // chat_reflection = reflection model, temp 0.1 for determinism, 2048 tokens
    // (DeepSeek v4 reasoning eats budget — 踩坑#3).
    //
    // Retry with exponential backoff: 30 back-to-back judge calls can trip the
    // provider's rate limit (observed: ~8 consecutive Err mid-run). Without
    // retry the harness silently zero-scores those rows and "passes" for the
    // wrong reason. std::thread::sleep is fine here — single-threaded test
    // runtime, nothing else to starve.
    let mut last_err = String::new();
    for attempt in 1..=3u32 {
        match llm.chat_reflection(&messages, Some(0.1), Some(2048)).await {
            Ok(res) => {
                let raw = res.content.trim();
                if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
                    if let Ok(j) = serde_json::from_str::<PersonaJudge>(&raw[start..=end]) {
                        return Ok(j);
                    }
                }
                last_err = format!("parse fail (attempt {}): {:?})", attempt, raw);
            }
            Err(e) => last_err = format!("llm error (attempt {}): {}", attempt, e),
        }
        if attempt < 3 {
            std::thread::sleep(std::time::Duration::from_secs(2u64.pow(attempt)));
        }
    }
    Err(last_err)
}

// ---------------------------------------------------------------------------
// Three-layer evaluation
// ---------------------------------------------------------------------------

#[derive(Default)]
struct GroupStats {
    n: usize,
    rule_sum: f64,        // personality_drift_score overall (1.0 = clean)
    cosine_sum: f64,      // semantic_drift_score overall (1.0 = close to persona)
    judge_sum: f64,       // judge persona_fit (10 = perfect)
    rule_flagged: usize,  // how many the rule layer flagged (overall < 1.0)
}

impl GroupStats {
    fn avg(&self, which: char) -> f64 {
        if self.n == 0 {
            return 0.0;
        }
        match which {
            'r' => self.rule_sum / self.n as f64,
            'c' => self.cosine_sum / self.n as f64,
            'j' => self.judge_sum / self.n as f64,
            _ => 0.0,
        }
    }
}

#[tokio::test]
async fn personality_three_layer_eval() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .is_test(true)
        .try_init();

    let config = config::load_config().unwrap_or_default();
    let model_dir = config::resolve_model_dir(&config);
    println!("[setup] model_dir = {}", model_dir.display());

    let llm = LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .expect("LLM not configured — set API key in config.toml first");

    let emb = EmbeddingService::new(&model_dir);
    emb.load().expect(
        "embedding model failed to load — cosine layer needs BGE-M3 (check model_dir points at a \
         complete ONNX export)",
    );
    assert!(emb.is_ready(), "embedding model not ready after load");

    // Embed the canonical persona reference once.
    let persona_vec = emb.embed(LIRI_PERSONA_REFERENCE).expect("embed persona reference");

    let mut on = GroupStats::default();
    let mut gross = GroupStats::default();
    let mut subtle = GroupStats::default();
    let mut judge_failures: usize = 0;

    println!(
        "\n{:<4} {:<7} {:<12} {:>7} {:>8} {:>6} {:<10} {:<14} {}",
        "id", "group", "expected", "rule", "cosine", "judge", "judge_drift", "reason", "text"
    );
    println!("{}", "-".repeat(120));

    for s in SAMPLES {
        // Layer 1: rule heuristics (cheap, keyless).
        let rule = personality_drift_score(s.text);
        // Layer 2: cosine semantic (needs embedding).
        let rvec = emb.embed(s.text).expect("embed sample");
        let sem = semantic_drift_score(&rvec, &persona_vec);
        // Layer 3: LLM judge (needs reflection model, with retry/backoff).
        let judge = match judge_persona(&llm, s.text).await {
            Ok(j) => j,
            Err(e) => {
                judge_failures += 1;
                println!("[judge FAIL id={}] {}", s.id, e);
                PersonaJudge::default()
            }
        };

        let bucket = match s.group {
            Group::On => &mut on,
            Group::Gross => &mut gross,
            Group::Subtle => &mut subtle,
        };
        bucket.n += 1;
        bucket.rule_sum += rule.overall;
        bucket.cosine_sum += sem.overall;
        bucket.judge_sum += judge.persona_fit;
        if rule.overall < 1.0 {
            bucket.rule_flagged += 1;
        }

        let preview: String = s.text.chars().take(18).collect();
        let reason: String = judge.reason.chars().take(12).collect();
        println!(
            "{:<4} {:<7} {:<12} {:>7.2} {:>8.3} {:>6.1} {:<10} {:<14} {}",
            s.id,
            s.group.as_str(),
            s.expected_drift,
            rule.overall,
            sem.overall,
            judge.persona_fit,
            judge.drift,
            reason,
            preview
        );
    }

    println!("{}", "-".repeat(120));
    println!(
        "{:<4} {:<7} {:<12} {:>7.3} {:>8.3} {:>6.2}  (rule 1.0=clean | cosine/judge higher=more on-persona)",
        "", "On", "avg", on.avg('r'), on.avg('c'), on.avg('j')
    );
    println!(
        "{:<4} {:<7} {:<12} {:>7.3} {:>8.3} {:>6.2}  rule-flagged {}/{}",
        "", "Gross", "avg", gross.avg('r'), gross.avg('c'), gross.avg('j'), gross.rule_flagged, gross.n
    );
    println!(
        "{:<4} {:<7} {:<12} {:>7.3} {:>8.3} {:>6.2}  rule-flagged {}/{} (rule blind here)",
        "", "Subtle", "avg", subtle.avg('r'), subtle.avg('c'), subtle.avg('j'), subtle.rule_flagged, subtle.n
    );

    // ---- Assertions: each layer's coverage boundary ---------------------

    // Judge reliability gate: the group-ordering assertions below are only
    // meaningful if the judge actually scored the samples. A mass rate-limit
    // failure would zero-fill rows and let the test "pass" for the wrong
    // reason. Fail loudly if too many judge calls couldn't be scored.
    assert!(
        judge_failures <= 3,
        "{} of {} judge calls failed (rate-limit/transient) even after retry — benchmark unreliable, \
         re-run when the API is less loaded. Last failure reasons printed above.",
        judge_failures,
        SAMPLES.len()
    );

    // Judge: On-persona clearly outranks both off-persona groups. This is the
    // headline — the judge sees drift the cheap layers can't (esp. Subtle).
    assert!(
        on.avg('j') > gross.avg('j'),
        "judge: On ({:.1}) must outrank Gross ({:.1})",
        on.avg('j'),
        gross.avg('j')
    );
    assert!(
        on.avg('j') > subtle.avg('j'),
        "judge: On ({:.1}) must outrank Subtle ({:.1}) — this is the gap the rule layer can't see",
        on.avg('j'),
        subtle.avg('j')
    );

    // Cosine: On-persona closer to the persona reference than off-persona.
    assert!(
        on.avg('c') > subtle.avg('c'),
        "cosine: On ({:.3}) must outrank Subtle ({:.3})",
        on.avg('c'),
        subtle.avg('c')
    );

    // Rule layer: its known boundary — it catches Gross but is blind to Subtle.
    assert!(
        gross.rule_flagged >= gross.n / 2,
        "rule layer should flag at least half of Gross group, flagged {}/{}",
        gross.rule_flagged,
        gross.n
    );
    assert_eq!(
        subtle.rule_flagged, 0,
        "rule layer must be BLIND to Subtle group (all pass rules) — flagged {}/{}; if this fails, \
         a Subtle sample accidentally trips a rule (re-check the sample)",
        subtle.rule_flagged,
        subtle.n
    );
    assert!(
        on.avg('r') > gross.avg('r'),
        "rule: On ({:.3}) must outrank Gross ({:.3})",
        on.avg('r'),
        gross.avg('r')
    );

    println!("\n[three-layer] all assertions passed — judge catches what rules+cosine miss");
}
