//! Embedding A/B retrieval benchmark (Architecture Principle #8 cost-is-design +
//! #11 explainability).
//!
//! Quantifies the retrieval-quality gap between the two retrieval modes the
//! `retrieve()` pipeline can run in:
//!   - **Baseline (no embedding model):** candidate selection by memory
//!     strength, semantic score from keyword Jaccard + CJK character-bigram
//!     overlap (`compute_semantic` fallback branch).
//!   - **With embedding (BGE-M3 loaded):** candidate selection by cosine
//!     similarity over `episode_vectors`, semantic score from cosine.
//!
//! This is the gap the HANDOFF flagged: `sem ≈ 0` in the live Debug Panel
//! proved the production env never loaded the model, so retrieval silently
//! fell back to keywords. This harness measures exactly how much that costs.
//!
//! Methodology (controlled experiment — isolates the semantic signal):
//!   * Every episode is seeded with IDENTICAL importance (0.5), memory_strength
//!     (0.5), time (now) and no emotion. So the strength / recency / emotion
//!     score components are constant across all episodes — ranking is decided
//!     purely by the semantic component (cosine vs keyword).
//!   * Queries are split into two groups:
//!       - `literal`: query shares characters/words with its answer (both modes
//!         should hit).
//!       - `semantic`: query is paraphrase only — NO character overlap with the
//!         answer (only embedding can hit; keyword bigram fallback should miss).
//!   * Two independent in-memory DBs are seeded (one per mode) for clean
//!     isolation. (retrieve() is now a pure read — ADR 2026-08-09 — so strength
//!     no longer cross-contaminates; the per-mode DBs stay as a defensive
//!     boundary keeping each mode's ranking measurement self-contained.)
//!
//! Uses the REAL embedding model (CPU ONNX, no LLM, no network beyond model
//! load). Run:
//!   cargo test --test embedding_ab_harness -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::db::episodes as db_episodes;
use desktop_pet_lib::db::test_utils::test_db;
use desktop_pet_lib::db::vectors as db_vectors;
use desktop_pet_lib::db::DbState;
use desktop_pet_lib::embedding::EmbeddingService;
use desktop_pet_lib::emotion::state::EmotionState;
use desktop_pet_lib::mind::retrieval::retrieve;
use rusqlite::Connection;

/// A seeded episode. All episodes share constant importance/strength/time so
/// the semantic component is the only ranking differentiator.
struct SeedEp {
    id: &'static str,
    summary: &'static str,
}

/// A benchmark query. `expected` is the episode id that SHOULD rank #1.
/// `kind` = "literal" (char overlap with answer) | "semantic" (paraphrase only).
struct Query {
    text: &'static str,
    expected: &'static str,
    kind: &'static str,
}

// --- Episode pool -----------------------------------------------------------
// Six "semantic" answers, six "literal" answers, six distractors.
const EPS: &[SeedEp] = &[
    // semantic-answer episodes
    SeedEp { id: "ep_spine",     summary: "用 Spine 给桌宠做骨骼动画，权重刷了一整天，很繁琐" },
    SeedEp { id: "ep_cat",       summary: "家里的猫咪上个月走了，难过了好几天，心里空落落的" },
    SeedEp { id: "ep_interview", summary: "下周三要去字节跳动面试前端实习岗位，有点紧张" },
    SeedEp { id: "ep_rust",      summary: "这段时间在啃 Rust 语言的所有权和生命周期，挺烧脑的" },
    SeedEp { id: "ep_necklace",  summary: "女朋友过生日，送了她一条银项链，她收到特别开心" },
    SeedEp { id: "ep_gym",       summary: "坚持去健身房撸铁已经三个月了，手臂围度粗了一圈" },
    // literal-answer episodes (answer text contains a distinctive keyword reused in the query)
    SeedEp { id: "ep_hotpot",    summary: "周末和几个朋友去吃了海底捞火锅，排了好久的队" },
    SeedEp { id: "ep_movie",     summary: "去电影院看了《星际穿越》，诺兰拍得太震撼了" },
    SeedEp { id: "ep_thesis",    summary: "毕业论文写到第三章了，推进得有点慢，导师在催" },
    SeedEp { id: "ep_keyboard",  summary: "新买的客制化机械键盘到了，青轴的声音很脆很好听" },
    SeedEp { id: "ep_cold",      summary: "这两天感冒发烧到三十八度，跟公司请了假在家躺着" },
    SeedEp { id: "ep_ielts",     summary: "雅思考了总分七分，可惜口语差零点五分没到小分线" },
    // distractors (pool fillers — should never be the answer)
    SeedEp { id: "ep_d1",        summary: "今天天气不错，下午出去散了会儿步" },
    SeedEp { id: "ep_d2",        summary: "阳台种的多肉最近又长出了几个新芽" },
    SeedEp { id: "ep_d3",        summary: "把卧室彻底收拾了一遍，扔掉好多旧东西" },
    SeedEp { id: "ep_d4",        summary: "晚上自己煮了碗西红柿鸡蛋面当夜宵" },
    SeedEp { id: "ep_d5",        summary: "听了一整天讲历史的播客节目，很有意思" },
    SeedEp { id: "ep_d6",        summary: "给手机换了一张海边风景的新壁纸" },
];

// --- Query set --------------------------------------------------------------
const QUERIES: &[Query] = &[
    // semantic — paraphrase, intentionally NO character overlap with the answer
    Query { text: "最近在忙点什么技术活儿呀",           expected: "ep_spine",     kind: "semantic" },
    Query { text: "她那阵子情绪低落是为啥",             expected: "ep_cat",       kind: "semantic" },
    Query { text: "接下来有什么重要的安排没",           expected: "ep_interview", kind: "semantic" },
    Query { text: "在钻研哪门编程语言呢",               expected: "ep_rust",      kind: "semantic" },
    Query { text: "感情上有什么值得高兴的事吗",         expected: "ep_necklace",  kind: "semantic" },
    Query { text: "生活习惯上有什么一直在坚持的",       expected: "ep_gym",       kind: "semantic" },
    // literal — reuses a distinctive keyword/character from the answer
    Query { text: "海底捞好吃吗",                       expected: "ep_hotpot",    kind: "literal" },
    Query { text: "看了啥好电影没",                     expected: "ep_movie",     kind: "literal" },
    Query { text: "论文写得咋样了",                     expected: "ep_thesis",    kind: "literal" },
    Query { text: "新键盘手感如何",                     expected: "ep_keyboard",  kind: "literal" },
    Query { text: "感冒好些了吗",                       expected: "ep_cold",      kind: "literal" },
    Query { text: "雅思成绩出来了吧",                   expected: "ep_ielts",     kind: "literal" },
];

/// Inserts one episode with the controlled constants (importance 0.5,
/// strength 0.5, no emotion). Ranking thus depends only on the semantic score.
fn insert_episode(conn: &Connection, id: &str, summary: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let ep = db_episodes::Episode {
        emotion_anchor: None,
        id: id.to_string(),
        time: now.clone(),
        summary: summary.to_string(),
        emotion: None,
        importance: 0.5,
        is_landmark: false,
        subject: "user".to_string(),
        participants: None,
        topics: None,
        source_type: "conversation".to_string(),
        source_conversation_id: None,
        source_turn: None,
        memory_strength: 0.5,
        recall_count: 0,
        last_recalled_at: None,
        consolidated: false,
        created_at: now,
    };
    db_episodes::insert(conn, &ep).expect("insert episode");
}

/// Seeds all episodes. When an embedding service is supplied and ready, also
/// embeds each summary into `episode_vectors` (this is the "backfill" the live
/// store path does at ingest time — reproduced here so the with-embedding run
/// actually has vectors to search).
fn seed(db: &DbState, emb: Option<&EmbeddingService>) {
    for ep in EPS {
        db.with_conn(|conn| {
            insert_episode(conn, ep.id, ep.summary);
            Ok(())
        })
        .unwrap();
    }
    if let Some(svc) = emb {
        if svc.is_ready() {
            for ep in EPS {
                match svc.embed(ep.summary) {
                    Ok(vec) => {
                        db.with_conn(|conn| db_vectors::insert(conn, ep.id, &vec))
                            .unwrap();
                    }
                    Err(e) => panic!("embed {} failed: {}", ep.id, e),
                }
            }
            let n = db
                .with_conn(|conn| db_vectors::count(conn))
                .unwrap();
            println!("[seed] embedded {} episodes -> {} vectors stored", EPS.len(), n);
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GroupStats {
    queries: usize,
    hit3: usize,
    mrr_sum: f64,
    sem_sum: f64, // semantic score of the expected answer, averaged over queries
}

impl GroupStats {
    fn hit_rate(&self) -> f64 {
        if self.queries == 0 { 0.0 } else { self.hit3 as f64 / self.queries as f64 }
    }
    fn mrr(&self) -> f64 {
        if self.queries == 0 { 0.0 } else { self.mrr_sum / self.queries as f64 }
    }
    fn avg_sem(&self) -> f64 {
        if self.queries == 0 { 0.0 } else { self.sem_sum / self.queries as f64 }
    }
}

/// Runs all queries through `retrieve` in the given mode and aggregates stats.
/// Returns (overall, literal_group, semantic_group).
fn eval(
    emb: Option<&EmbeddingService>,
    emotion: &EmotionState,
    label: &str,
) -> (GroupStats, GroupStats, GroupStats) {
    let mut overall = GroupStats::default();
    let mut lit = GroupStats::default();
    let mut sem = GroupStats::default();

    println!("\n========== {} ==========", label);
    println!(
        "{:<6} {:<10} {:<4} {:<4} {:>7} {:<8}",
        "kind", "query", "hit", "rank", "sem@exp", "top1_summary"
    );

    for q in QUERIES {
        // Fresh in-memory DB per query for clean isolation. (retrieve() is now a
        // pure read — ADR 2026-08-09 — so strength no longer mutates across
        // queries, but a per-query DB keeps each ranking measurement independent
        // and defends against any future retrieval side-effect.)
        let db = test_db();
        seed(&db, emb);
        let result = retrieve(q.text, emotion, emb, &db, 3).expect("retrieve");
        // episodes already sorted desc + truncated to top_k(3)
        let rank = result
            .episodes
            .iter()
            .position(|se| se.episode.id == q.expected);
        let hit = rank.is_some();
        let mrr = rank.map(|r| 1.0 / (r + 1) as f64).unwrap_or(0.0);
        let sem_exp = result
            .episodes
            .iter()
            .find(|se| se.episode.id == q.expected)
            .map(|se| se.score_breakdown.semantic)
            .unwrap_or(0.0);
        let top1 = result
            .episodes
            .first()
            .map(|se| se.episode.summary.chars().take(12).collect::<String>())
            .unwrap_or_else(|| "(none)".to_string());

        println!(
            "{:<6} {:<10} {:<4} {:<4} {:>7.3} {:<8}",
            q.kind,
            &q.text[..q.text.char_indices().take(8).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(q.text.len())],
            if hit { "Y" } else { "n" },
            rank.map(|r| format!("#{}", r + 1)).unwrap_or("-".to_string()),
            sem_exp,
            top1
        );

        let bucket = if q.kind == "semantic" { &mut sem } else { &mut lit };
        bucket.queries += 1;
        bucket.hit3 += hit as usize;
        bucket.mrr_sum += mrr;
        bucket.sem_sum += sem_exp;
        overall.queries += 1;
        overall.hit3 += hit as usize;
        overall.mrr_sum += mrr;
        overall.sem_sum += sem_exp;
    }

    println!(
        "[{}] overall   Hit@3 {:.0}%  MRR {:.3}  avg_sem@exp {:.3}",
        label,
        overall.hit_rate() * 100.0,
        overall.mrr(),
        overall.avg_sem()
    );
    println!(
        "[{}] literal   Hit@3 {:.0}%  MRR {:.3}  avg_sem@exp {:.3}",
        label,
        lit.hit_rate() * 100.0,
        lit.mrr(),
        lit.avg_sem()
    );
    println!(
        "[{}] semantic  Hit@3 {:.0}%  MRR {:.3}  avg_sem@exp {:.3}",
        label,
        sem.hit_rate() * 100.0,
        sem.mrr(),
        sem.avg_sem()
    );

    (overall, lit, sem)
}

#[test]
fn embedding_ab_comparison() {
    // Resolve model dir from the live config (validates the D-drive config end
    // to end — same path the app uses at startup).
    let config = config::load_config().unwrap_or_default();
    let model_dir = config::resolve_model_dir(&config);
    println!("[setup] model_dir = {}", model_dir.display());

    let svc = EmbeddingService::new(&model_dir);
    svc.load().expect(
        "embedding model failed to load — check model_dir points at a complete \
         BGE-M3 ONNX export (model.onnx + model.onnx_data + tokenizer.json)",
    );
    assert!(svc.is_ready(), "model reported not ready after load");

    // sanity: a vector actually comes out at the expected dimension
    let probe = svc.embed("你好世界").expect("probe embed");
    println!("[setup] probe embedding dim = {}", probe.len());

    let emotion = EmotionState::default();

    // Baseline: NO embedding passed -> keyword/strength fallback path.
    // (eval seeds a fresh in-memory DB per query internally.)
    let (base_all, base_lit, base_sem) = eval(None, &emotion, "BASELINE (keyword)");

    // With embedding: vectors seeded + cosine retrieval path.
    let (emb_all, emb_lit, emb_sem) = eval(Some(&svc), &emotion, "WITH EMBEDDING");

    // ---- final comparison ----
    println!("\n################## A/B SUMMARY ##################");
    print_row("metric", "baseline", "w/ embed", "delta");
    print_row(
        "overall Hit@3",
        &format!("{:.0}%", base_all.hit_rate() * 100.0),
        &format!("{:.0}%", emb_all.hit_rate() * 100.0),
        &format!("{:+.0} pts", (emb_all.hit_rate() - base_all.hit_rate()) * 100.0),
    );
    print_row(
        "overall MRR",
        &format!("{:.3}", base_all.mrr()),
        &format!("{:.3}", emb_all.mrr()),
        &format!("{:+.3}", emb_all.mrr() - base_all.mrr()),
    );
    print_row(
        "literal Hit@3",
        &format!("{:.0}%", base_lit.hit_rate() * 100.0),
        &format!("{:.0}%", emb_lit.hit_rate() * 100.0),
        &format!("{:+.0} pts", (emb_lit.hit_rate() - base_lit.hit_rate()) * 100.0),
    );
    print_row(
        "semantic Hit@3",
        &format!("{:.0}%", base_sem.hit_rate() * 100.0),
        &format!("{:.0}%", emb_sem.hit_rate() * 100.0),
        &format!("{:+.0} pts", (emb_sem.hit_rate() - base_sem.hit_rate()) * 100.0),
    );
    print_row(
        "semantic MRR",
        &format!("{:.3}", base_sem.mrr()),
        &format!("{:.3}", emb_sem.mrr()),
        &format!("{:+.3}", emb_sem.mrr() - base_sem.mrr()),
    );
    print_row(
        "avg sem@answer",
        &format!("{:.3}", base_all.avg_sem()),
        &format!("{:.3}", emb_all.avg_sem()),
        &format!("{:+.3}", emb_all.avg_sem() - base_all.avg_sem()),
    );

    // The headline assertion: embedding must not regress literal queries, and
    // must strictly improve the paraphrase (semantic) group — that is the entire
    // reason the model exists.
    assert!(
        emb_sem.hit_rate() >= base_sem.hit_rate(),
        "embedding regressed semantic Hit@3: {} -> {}",
        base_sem.hit_rate(),
        emb_sem.hit_rate()
    );
    assert!(
        emb_lit.hit_rate() >= base_lit.hit_rate(),
        "embedding regressed literal Hit@3: {} -> {}",
        base_lit.hit_rate(),
        emb_lit.hit_rate()
    );
    assert!(
        emb_sem.mrr() > base_sem.mrr(),
        "embedding did not improve semantic MRR ({} -> {})",
        base_sem.mrr(),
        emb_sem.mrr()
    );
}

fn print_row(a: &str, b: &str, c: &str, d: &str) {
    println!("{:<18} {:<12} {:<12} {:<12}", a, b, c, d);
}
