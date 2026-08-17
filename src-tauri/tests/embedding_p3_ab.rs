//! P3 evaluation: BGE-M3 int8 (incumbent) vs bge-small-zh-v1.5 int8 (~24 MB).
//!
//! **Verdict 2026-08-17: P3 REJECTED.** Same benchmark hit@1 (10/12), but
//! real-DB top-5 overlap was only **0.52** (gate: >= 0.90) — on the real 46
//! episodes the small model surfaced meta-conversation ("询问昨天有趣的事")
//! as a top hit for 开心/伤心/喝点什么 queries (literal collision, weak
//! discrimination) and drifted hard on 老家/项目. Quality gate "P3 must not
//! hurt retrieval" failed => BGE-M3 int8 stays. Keep this harness as a
//! soft-report re-evaluation tool (no assert; rerun after corpus grows or a
//! better small model appears).
//!
//! Run:
//!   cargo test --test embedding_p3_ab -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::embedding::EmbeddingService;
use rusqlite::Connection;

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

fn rank_all(
    svc: &EmbeddingService,
    doc_ids: &[&str],
    doc_texts: &[String],
    queries: &[&str],
) -> Vec<Vec<(String, f64)>> {
    let doc_vecs: Vec<Vec<f32>> = svc.embed_batch(doc_texts).expect("embed docs");
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

#[test]
fn small_zh_vs_m3_int8() {
    let config = config::load_config().unwrap_or_default();
    let m3_dir = config::resolve_model_dir(&config);
    let small_dir = std::path::PathBuf::from("D:/models/bge-small-zh-v1.5");
    if !EmbeddingService::new(&small_dir).files_present() {
        println!("[skip] bge-small-zh-v1.5 not present at {} — P3 not downloaded", small_dir.display());
        return;
    }

    println!("[setup] loading m3-int8 from {}", m3_dir.display());
    let m3 = EmbeddingService::new(&m3_dir);
    m3.load().expect("load m3-int8");
    println!("[setup] loading bge-small-zh from {}", small_dir.display());
    let small = EmbeddingService::new(&small_dir);
    small.load().expect("load bge-small-zh");

    // ---------- 1. Controlled benchmark --------------------------------------
    let bench_ids: Vec<&str> = EPS.iter().map(|e| e.id).collect();
    let bench_texts: Vec<String> = EPS.iter().map(|e| e.summary.to_string()).collect();
    let qtexts: Vec<&str> = QUERIES.iter().map(|q| q.text).collect();

    let m3_ranks = rank_all(&m3, &bench_ids, &bench_texts, &qtexts);
    let small_ranks = rank_all(&small, &bench_ids, &bench_texts, &qtexts);

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

    for (name, ranks) in [("m3-int8", &m3_ranks), ("small-zh", &small_ranks)] {
        let (all, _) = hit(ranks, "all");
        let (sem, _) = hit(ranks, "semantic");
        let (lit, _) = hit(ranks, "literal");
        println!("[benchmark] {name}: hit@1 all={all}/12 semantic={sem}/6 literal={lit}/6");
    }
    println!("[benchmark] per-query top1 (m3 -> small):");
    for (i, q) in QUERIES.iter().enumerate() {
        let m = m3_ranks[i].first().map(|(id, _)| id.clone()).unwrap_or_default();
        let s = small_ranks[i].first().map(|(id, _)| id.clone()).unwrap_or_default();
        let mark = if m == s { "=" } else { "!" };
        println!("  [{:>8}] {:<18} m3={:<14} small={:<14} {mark}", q.kind, q.text, m, s);
    }

    let (m3_all, _) = hit(&m3_ranks, "all");
    let (small_all, _) = hit(&small_ranks, "all");
    let (m3_sem, _) = hit(&m3_ranks, "semantic");
    let (small_sem, _) = hit(&small_ranks, "semantic");

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
    let real_ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    let real_texts: Vec<String> = rows.iter().map(|(_, s)| s.clone()).collect();
    println!("[real-db] {} episodes, {} queries", rows.len(), REAL_QUERIES.len());

    let m3_real = rank_all(&m3, &real_ids, &real_texts, &REAL_QUERIES);
    let small_real = rank_all(&small, &real_ids, &real_texts, &REAL_QUERIES);

    let mut overlaps: Vec<f64> = Vec::new();
    for (i, q) in REAL_QUERIES.iter().enumerate() {
        let m3_top5: Vec<&str> = m3_real[i].iter().take(5).map(|(id, _)| id.as_str()).collect();
        let s_top5: Vec<&str> = small_real[i].iter().take(5).map(|(id, _)| id.as_str()).collect();
        let overlap = m3_top5.iter().filter(|id| s_top5.contains(id)).count() as f64 / 5.0;
        overlaps.push(overlap);
        println!(
            "  {:<18} overlap@5={:.2}  m3#1={} small#1={}",
            q,
            overlap,
            m3_top5.first().copied().unwrap_or("?"),
            s_top5.first().copied().unwrap_or("?")
        );
    }
    let mean_overlap = overlaps.iter().sum::<f64>() / overlaps.len() as f64;
    println!("[real-db] mean top-5 overlap = {mean_overlap:.3}");

    let pass = small_all >= m3_all && small_sem >= m3_sem && mean_overlap >= 0.90;
    if pass {
        println!("[verdict] PASS — small-zh keeps hit@1 ({small_all} vs {m3_all}) and agreement {mean_overlap:.3}");
    } else {
        println!(
            "[verdict] FAIL — hit@1 {small_all} vs {m3_all} (semantic {small_sem} vs {m3_sem}), overlap {mean_overlap:.3} — P3 swap rejected (2026-08-17 decision: keep bge-m3-int8)"
        );
    }
    // Soft report only: the 2026-08-17 evaluation REJECTED the swap (real-DB
    // overlap 0.52 vs the 0.90 gate). An assert would keep the suite red
    // forever on a model we are not shipping; the harness stays as the
    // re-evaluation tool for when the corpus grows or a better small model
    // appears.
}
