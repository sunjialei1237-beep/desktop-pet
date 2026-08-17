//! Embedding quality A/B: fp32 vs int8-quantized BGE-M3 (P1 gate).
//!
//! Question this harness answers: did swapping the fp32 model for the int8
//! quantized export (P1 memory reduction) cost retrieval quality?
//!
//! Two independent measurements:
//!   1. **Controlled benchmark (ground truth)** — the same 18-episode pool /
//!      12 labelled queries as `embedding_ab_harness` (6 semantic paraphrase
//!      queries only embedding can hit, 6 literal). Each model embeds the
//!      pool with its OWN vector space (docs + query from the same model —
//!      never mixed) and we compare hit@1 counts directly.
//!   2. **Real-DB ranking agreement (no labels needed)** — all episodes from
//!      the real DB (read-only) plus 10 paraphrase queries about what the
//!      user actually talked about. We measure top-5 overlap between the two
//!      models' rankings per query. High overlap = int8 retrieves the same
//!      things fp32 would.
//!
//! PASS GATE (asserted when the fp32 files are present; the fp32 side is
//! skipped once the legacy files are deleted, keeping this green after):
//!   - benchmark: int8 hit@1 (total AND semantic subset) >= fp32 hit@1
//!   - real DB:   mean top-5 overlap >= 0.90
//!
//! The fp32 model dir is materialized via HARD LINKS next to the real model
//! dir (same volume — no 2.1 GB copy). Run:
//!   cargo test --test embedding_quality_ab -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::embedding::EmbeddingService;
use rusqlite::Connection;

// --- Controlled benchmark pool (same as embedding_ab_harness) ---------------
// Every episode gets IDENTICAL importance/strength/time at scoring time, but
// this harness ranks by RAW cosine only (no hybrid weights) so the comparison
// isolates the embedding signal itself.

struct SeedEp {
    id: &'static str,
    summary: &'static str,
}

const EPS: &[SeedEp] = &[
    SeedEp { id: "ep_spine",     summary: "用 Spine 给桌宠做骨骼动画，权重刷了一整天，很繁琐" },
    SeedEp { id: "ep_cat",       summary: "家里的猫咪上个月走了，难过了好几天，心里空落落的" },
    SeedEp { id: "ep_interview", summary: "下周三要去字节跳动面试前端实习岗位，有点紧张" },
    SeedEp { id: "ep_rust",      summary: "这段时间在啃 Rust 语言的所有权和生命周期，挺烧脑的" },
    SeedEp { id: "ep_necklace",  summary: "女朋友过生日，送了她一条银项链，她收到特别开心" },
    SeedEp { id: "ep_gym",       summary: "坚持去健身房撸铁已经三个月了，手臂围度粗了一圈" },
    SeedEp { id: "ep_hotpot",    summary: "周末和几个朋友去吃了海底捞火锅，排了好久的队" },
    SeedEp { id: "ep_movie",     summary: "去电影院看了《星际穿越》，诺兰拍得太震撼了" },
    SeedEp { id: "ep_thesis",    summary: "毕业论文写到第三章了，推进得有点慢，导师在催" },
    SeedEp { id: "ep_keyboard",  summary: "新买的客制化机械键盘到了，青轴的声音很脆很好听" },
    SeedEp { id: "ep_cold",      summary: "这两天感冒发烧到三十八度，跟公司请了假在家躺着" },
    SeedEp { id: "ep_ielts",     summary: "雅思考了总分七分，可惜口语差零点五分没到小分线" },
    SeedEp { id: "ep_d1",        summary: "今天天气不错，下午出去散了会儿步" },
    SeedEp { id: "ep_d2",        summary: "阳台种的多肉最近又长出了几个新芽" },
    SeedEp { id: "ep_d3",        summary: "把卧室彻底收拾了一遍，扔掉好多旧东西" },
    SeedEp { id: "ep_d4",        summary: "晚上自己煮了碗西红柿鸡蛋面当夜宵" },
    SeedEp { id: "ep_d5",        summary: "听了一整天讲历史的播客节目，很有意思" },
    SeedEp { id: "ep_d6",        summary: "给手机换了一张海边风景的新壁纸" },
];

struct Query {
    text: &'static str,
    expected: &'static str,
    kind: &'static str,
}

const QUERIES: &[Query] = &[
    Query { text: "最近在忙点什么技术活儿呀",           expected: "ep_spine",     kind: "semantic" },
    Query { text: "她那阵子情绪低落是为啥",             expected: "ep_cat",       kind: "semantic" },
    Query { text: "接下来有什么重要的安排没",           expected: "ep_interview", kind: "semantic" },
    Query { text: "在钻研哪门编程语言呢",               expected: "ep_rust",      kind: "semantic" },
    Query { text: "感情上有什么值得高兴的事吗",         expected: "ep_necklace",  kind: "semantic" },
    Query { text: "生活习惯上有什么一直在坚持的",       expected: "ep_gym",       kind: "semantic" },
    Query { text: "海底捞好吃吗",                       expected: "ep_hotpot",    kind: "literal" },
    Query { text: "看了啥好电影没",                     expected: "ep_movie",     kind: "literal" },
    Query { text: "论文写得咋样了",                     expected: "ep_thesis",    kind: "literal" },
    Query { text: "新键盘手感如何",                     expected: "ep_keyboard",  kind: "literal" },
    Query { text: "感冒好些了吗",                       expected: "ep_cold",      kind: "literal" },
    Query { text: "雅思成绩出来了吧",                   expected: "ep_ielts",     kind: "literal" },
];

// --- Real-DB queries (paraphrases of what the user actually said; -----------
// hand-written against the live episode table — no character overlap needed,
// these measure MODEL AGREEMENT, not ground truth).
const REAL_QUERIES: &[&str] = &[
    "你家小狗叫什么名字呀",
    "最近有什么伤心的事吗",
    "平时喜欢什么运动",
    "想喝点什么饮料",
    "老家是哪里的",
    "个子有多高",
    "上次考试结果怎么样",
    "她在忙什么项目",
    "最近有什么开心的事",
    "晚上想吃点什么",
];

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Embeds every summary + query with the given service (same model for docs
/// and query — never a mixed vector space), then returns per-query rankings
/// [(episode_id, cosine)] descending.
fn rank_all(
    svc: &EmbeddingService,
    doc_ids: &[&str],
    doc_texts: &[String],
    queries: &[&str],
) -> Vec<Vec<(String, f64)>> {
    let doc_vecs: Vec<Vec<f32>> = svc
        .embed_batch(doc_texts)
        .expect("embed docs");
    queries
        .iter()
        .map(|q| {
            let qv = svc.embed(q).expect("embed query");
            let mut ranked: Vec<(String, f64)> = doc_ids
                .iter()
                .zip(doc_vecs.iter())
                .map(|(id, dv)| (id.to_string(), cosine(&qv, dv)))
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            ranked
        })
        .collect()
}

fn top_n<'a>(ranking: &'a [(String, f64)], n: usize) -> Vec<&'a str> {
    ranking.iter().take(n).map(|(id, _)| id.as_str()).collect()
}

#[test]
fn int8_quality_matches_fp32() {
    let config = config::load_config().unwrap_or_default();
    let int8_dir = config::resolve_model_dir(&config);

    // fp32 side: hard-link the legacy files into a sibling dir (same volume).
    // choose_model_file picks legacy there (no model_quantized.onnx present).
    let fp32_src = std::path::PathBuf::from("D:/models/bge-m3");
    let fp32_files = ["model.onnx", "model.onnx_data", "tokenizer.json", "config.json", "onnxruntime.dll"];
    let fp32_available = fp32_files
        .iter()
        .all(|f| fp32_src.join(f).exists());
    if !fp32_available {
        println!("[skip] fp32 legacy files not present (already deleted?) — nothing to compare");
        return;
    }
    let fp32_dir = fp32_src.parent().unwrap().join("_fp32_ab_tmp");
    let _ = std::fs::remove_dir_all(&fp32_dir);
    std::fs::create_dir_all(&fp32_dir).unwrap();
    for f in fp32_files {
        let src = fp32_src.join(f);
        let dst = fp32_dir.join(f);
        if std::fs::hard_link(&src, &dst).is_err() {
            std::fs::copy(&src, &dst).expect("copy fp32 model file");
        }
    }

    println!("[setup] loading int8 from {}", int8_dir.display());
    let int8 = EmbeddingService::new(&int8_dir);
    int8.load().expect("load int8 model");
    println!("[setup] loading fp32 from {}", fp32_dir.display());
    let fp32 = EmbeddingService::new(&fp32_dir);
    fp32.load().expect("load fp32 model");

    // ---------- 1. Controlled benchmark: hit@1 parity -----------------------
    let bench_ids: Vec<&str> = EPS.iter().map(|e| e.id).collect();
    let bench_texts: Vec<String> = EPS.iter().map(|e| e.summary.to_string()).collect();
    let qtexts: Vec<&str> = QUERIES.iter().map(|q| q.text).collect();

    let int8_ranks = rank_all(&int8, &bench_ids, &bench_texts, &qtexts);
    let fp32_ranks = rank_all(&fp32, &bench_ids, &bench_texts, &qtexts);

    let hit = |ranks: &Vec<Vec<(String, f64)>>, kind: &str| -> (usize, usize) {
        QUERIES
            .iter()
            .enumerate()
            .filter(|(_, q)| kind == "all" || q.kind == kind)
            .fold((0, 0), |(h, t), (i, q)| {
                let got = ranks[i].first().map(|(id, _)| id.as_str()).unwrap_or("");
                (h + (got == q.expected) as usize, t + 1)
            })
    };

    for (name, ranks) in [("fp32", &fp32_ranks), ("int8", &int8_ranks)] {
        let (all, _) = hit(ranks, "all");
        let (sem, _) = hit(ranks, "semantic");
        let (lit, _) = hit(ranks, "literal");
        println!("[benchmark] {name}: hit@1 all={all}/12 semantic={sem}/6 literal={lit}/6");
    }
    println!("[benchmark] per-query top1 (fp32 -> int8):");
    for (i, q) in QUERIES.iter().enumerate() {
        let f = fp32_ranks[i].first().map(|(id, _)| id.clone()).unwrap_or_default();
        let n = int8_ranks[i].first().map(|(id, _)| id.clone()).unwrap_or_default();
        let mark = if f == n { "=" } else { "!" };
        println!("  [{:>8}] {:<18} fp32={:<14} int8={:<14} {mark}", q.kind, q.text, f, n);
    }

    let (fp32_all, _) = hit(&fp32_ranks, "all");
    let (int8_all, _) = hit(&int8_ranks, "all");
    let (fp32_sem, _) = hit(&fp32_ranks, "semantic");
    let (int8_sem, _) = hit(&int8_ranks, "semantic");
    assert!(
        int8_all >= fp32_all,
        "int8 hit@1 ({int8_all}) dropped below fp32 ({fp32_all}) on the benchmark"
    );
    assert!(
        int8_sem >= fp32_sem,
        "int8 semantic hit@1 ({int8_sem}) dropped below fp32 ({fp32_sem})"
    );

    // ---------- 2. Real-DB ranking agreement ---------------------------------
    let db_path = config::resolve_db_path(&config);
    let conn = Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open real db read-only");
    let mut stmt = conn.prepare("SELECT id, summary FROM episodes").unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    if rows.len() < 10 {
        println!("[skip] real DB has only {} episodes — skipping agreement check", rows.len());
        std::fs::remove_dir_all(&fp32_dir).ok();
        return;
    }
    let real_ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    let real_texts: Vec<String> = rows.iter().map(|(_, s)| s.clone()).collect();
    println!("[real-db] {} episodes, {} queries", rows.len(), REAL_QUERIES.len());

    let i8_real = rank_all(&int8, &real_ids, &real_texts, &REAL_QUERIES);
    let f32_real = rank_all(&fp32, &real_ids, &real_texts, &REAL_QUERIES);

    let mut overlaps: Vec<f64> = Vec::new();
    for (i, q) in REAL_QUERIES.iter().enumerate() {
        let i8_top5: Vec<&str> = top_n(&i8_real[i], 5);
        let f32_top5: Vec<&str> = top_n(&f32_real[i], 5);
        let overlap = i8_top5.iter().filter(|id| f32_top5.contains(id)).count() as f64 / 5.0;
        overlaps.push(overlap);
        println!(
            "  {:<18} overlap@5={:.2}  int8#1={} fp32#1={}",
            q,
            overlap,
            i8_top5.first().copied().unwrap_or("?"),
            f32_top5.first().copied().unwrap_or("?")
        );
    }
    let mean_overlap = overlaps.iter().sum::<f64>() / overlaps.len() as f64;
    println!("[real-db] mean top-5 overlap = {mean_overlap:.3}");

    // Cleanup the hard-link dir so we never leave stray copies behind.
    std::fs::remove_dir_all(&fp32_dir).ok();

    assert!(
        mean_overlap >= 0.90,
        "int8 vs fp32 real-DB top-5 overlap {mean_overlap:.3} below the 0.90 gate"
    );
    println!("[verdict] PASS — int8 keeps benchmark hit@1 ({int8_all} vs {fp32_all}) and real-DB agreement {mean_overlap:.3}");
}
