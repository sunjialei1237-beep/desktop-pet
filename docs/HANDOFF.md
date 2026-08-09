# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-08-09（续¹⁰·选择性遗忘多轮消歧义）—— 补遗忘链路两个体验缺口：① 多候选不澄清（「忘掉咖啡」同时命中 fact+episode，旧逻辑直接猜删一个可能删错）② fact/pending 措辞不匹配太硬（char_overlap 字面不重叠，「忘掉早睡的事」匹配不到 fact「想早睡总是熬夜」→ 生硬"不记得"）。`forget_best_match` 改三态 `ForgetOutcome::{Deleted,Declined,Ambiguous}`——≥2 候选**不再删而是反问**；新 `PendingForget` 跨轮 slot（抄 `pacing` Mutex 范式）+ `resolve_candidate`（序数词 第N个/前者/后者/甲乙 + char_overlap≥0.4）+ 90s 超时 + 只重问一次；converse 在 ingest **前**拦第二轮（"第一个"不进 Forget gate→必须 gate 前拦截，#1）→ 解析→`execute_candidate` 删→跳 ingest（二轮不被存为新记忆）；fact/pending 加 `semantic_rerank`（char_overlap top-5 现场 embed+cosine 归一映射 0.7 门，模型不可用退回 char_overlap，#6）。踩坑#4 全程避（只加 enum/字段不改签名；3 harness ConverseCtx 构造点同步 + prompt_quality case 1009 ForgetAck→ForgetAsk + ForgetAsk 启发式）。**lib 293（+6 forget 测）/ check --tests ✅ / release 已 rebuild（17:20）**。→ 待实跑见 D15。详见 §最近一轮 (续¹⁰)。**原 续⁹·记忆卫生层 —— 结构性治理记忆三类易复发缺陷：A 抽取无校验 / B 读路径强化 / C 去重视野。新 `mind/memory_gate.rs`（category 白名单 + 噪声 key/value deny，store 写库前过滤，中文 trivia 靠 key 抓）；`retrieve()` 删 reinforce 副作用 → 纯读 + 新 `reinforce_top`（仅 converse+proactive genuine-recall 调用，零签名变更）；converse known_facts preference-only → `get_all_active(30)`。复盘纠正：原以为 strength 只升不降→**错**，`decay_strength`(0.998/天) 已每日运行，故砍掉新衰减子系统。ADR `docs/decisions/2026-08-09-memory-hygiene-layer.md`（含三次多视角复盘）。**代码 lib 287 + golden 29 passed / 17 测试二进制全编译，commit `7f4af17`**。**一次性数据治理已执行**：expire 10 噪声 facts（知识问答/自我语境/越界类，保留 current_reading+糯米副本）+ 19 非地标 episode strength snap 回 importance（解测试期 rc382/445/446 饱和，排序现按 importance：小猪去世0.8居顶/素数trivia0.1落底），DB 备份 `.bak-hygiene`。**测试全绿**：lib 287 + golden 29 + memory_gate 6 + **闭环2 真实 LLM 验证 ✅ pass**（途中发现并修**续⁸ 既存 bug**：lively 70% 概率早返回跳过到期 pending，`proactive.rs::generate` 加一行守卫 `pending_due.is_empty() &&` 掷 lively 骰 → 到期提醒现确定性触发，70/30 多样性对无 pending 场景保留）。**release 待 rebuild**。详见 §最近一轮 (续⁹)。**续⁸ 自主冒泡灵性重构仍在位**（频率30min + 记忆30/灵性70），lively 多样性"先观察"。**

## 项目一句话
见 [`CLAUDE.md`](../CLAUDE.md)。Kill List 三闭环驱动开发：活着 Body → 记住你 Memory → 懂你 Soul。

## 当前进度（以测试为准）

| 闭环 / 层 | 状态 | 锚定测试 |
|---|---|---|
| 闭环1 说→记住→跨会话召回 | ✅ | `cargo test --test memory_recall` |
| 闭环2 到期主动提起 | ✅ | harness + 实跑：3分钟后主动冒泡提醒 |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 闭环3 "她记得我"体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| 库单测 | ✅ 194 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ✅ | 实跑验收通过（autoFocus:false + ny 取反） |
| 生命感 回来主动招呼 | ✅ 实跑通过 | loop_runner presence 转换 → welcome_back_bubble |
| 生命感 情绪连续外显 | ✅ build 过 / 待实跑 | emotionDriver → Live2DCanvas 连续参数插值（P10 emotionBridge）|
| 生命感 气泡生命力(节奏+glyph) | ✅ 实跑通过 | bubblePacing 打字节奏随情绪(关键词驱动) + bubble-glyph 无文字气泡（#12）|
| 生命感 昼夜节律接入 | ✅ build 过 / 待今晚实跑 | circadian sleepiness 接入微行为权重（深夜 yawn↑/look_around↓，Tier3 #7）— PID 95248 挂着过夜 |
| 生命感 Foley 音效 | ✅ 实跑通过 | 真实 Foley 素材 10 接入 + 启动 hi + 权重静默优先 + cooldown + 亲密度分档；sleep 已接（B3，待实跑）（Tier1 #3）|
| 对话 流式回复 | ✅ 实跑确认 | ipc::Channel 逐字（emit/listen 命令体内投递延迟+listener 立即 unlisten 全丢→Channel 正解）；用户长回复实跑确认逐字 |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）

> **2026-08-09（续¹⁰）选择性遗忘：多轮消歧义 + fact/pending 语义匹配 —— ✅ 已收尾（lib 293 + check --tests + release 17:20 已 rebuild）**。08-05 episode/fact/pending 遗忘 MVP 是**单轮、零状态、最高分赢家通吃**——两个体验缺口：①「忘掉咖啡」同时命中 fact「咖啡」+ episode「和糯米喝咖啡」时直接猜删一个（可能删错，#1 不可违背）②「忘掉早睡的事」因 char_overlap 字面不重叠匹配不到 fact「想早睡总是熬夜」→ 生硬"不记得"。**模块 A 多轮消歧义**：`forget_best_match` 改三态 `ForgetOutcome::{Deleted{summary}, Declined, Ambiguous{candidates}}`（替 `ForgetResult`）——≥2 候选**不删而反问**（landmark 已被 episode 腿过滤，候选皆可删）；新 `PendingForget{query,candidates,created_at}` 跨轮 slot（抄 `ConverseCtx.pacing` 的 `&Mutex<Option<..>>` 范式）；纯函数 `resolve_candidate`（序数词表 `ordinal_index`：第N个/前者/后者/最后/1/A/甲乙 + `cjk_to_digit` → 索引；否则 char_overlap 取最高≥0.4）+ `is_off_topic`（无序数且全候选 char_overlap<0.2 → 判换话题）。**关键控制流**：第二轮"第一个"不进 Forget gate（Forget 是动词驱动）→ converse **在 ingest 之前** `resolve_pending_forget` 拦截——take-and-clear 一次锁（>90s stale drop）→ resolve 命中 `execute_candidate` 删 + 跳过 ingest（合成 Silence-route outcome，二轮不被存为新记忆）；off-topic → 正常 ingest；仍不明 → 重问一次（slot 已清，防循环）。三条路径（Resolved/Reask/Proceed）汇合到既有 chat 生成回复。**模块 B fact/pending 语义匹配**：`find_fact_candidate`/`find_pending_candidate` 加 `embedding: Option<&EmbeddingService>`——char_overlap 粗筛 top-5 → `semantic_rerank` 现场 embed_batch + `cosine_similarity`（未归一，`((cos+1)/2).clamp(0,1)` 映射匹配 retrieval::compute_semantic）→ 0.7 门；embedding 任意 hiccup 退回 char_overlap（#6）。**模块 C harness 同步（踩坑#4）**：`ForgetCandidate` 加 `#[derive(Debug,Clone)]`（ForgetOutcome/PendingForget 的 Vec 成员要求）；`IngestionOutcome.forget` 类型 `ForgetResult`→`ForgetOutcome`（字段名不变）；3 harness（conversation/memory_recall/prompt_quality）ConverseCtx 构造点加 `pending_forget: &Mutex::new(None)`；prompt_quality case 1009 经真模型验证为**单候选**（见续¹⁰「修正」）→ 保持 ForgetAck；`Expect::ForgetAsk` 启发式保留备用。**修正（9bc3dac）**：① BGE-M3 无关基线 ~0.5 raw → 映射 0.75 > 0.7 门致假阳性——`semantic_rerank` 改**只提升 char_overlap>0 的条目**（字面锚点），1002/1005/1007 误 Ambiguous 已解；② 1009 episode leg `retrieve(top_k=1)` 被 offer 地标挡住 → 早睡 episode 没被看到 → 单 fact → ForgetAck（种子假象 + 既有局限，生产无地标干扰则正常双候选）。**lib 293（forget 18 测含 6 新）/ check --tests ✅ / prompt_quality G10 全 9 例 hard-check 0/9**。→ release 待 Fix B 后重 rebuild；待实跑见 D15。**当前无进行中任务**。

> **2026-08-09（续⁹）记忆卫生层 —— ✅ 已收尾（全测试绿 + 数据治理已执行，release 待 rebuild）**。用户"1先观察 2治理，且不能只清这一次脏数据——设计更好结构防复发；设计完自复盘3次（多角度：合理否/会否引新问题/有无更优解）；先调研可复用框架别急着造；设计复盘后自主执行并测试"。**firecrawl 调研**：mem0（REJECT 闸 + ADD-only 软废弃，V3 已砍 LLM judge 翻车+成本）/ MemGPT-Letta（blocks+caps+后台 sleep-time worker）/ Zep-Graphiti（bi-temporal 知识图谱，判 overkill）。读码定位**三类结构性缺陷**：**A 抽取无校验**（store 全信 extractor + LLM 自打 confidence，"太阳东升西落"conf0.98 入库）/ **B 读路径强化**（`retrieve()` 每次读都副作用写 `reinforce()` → recall_count 飙 382/445/446、strength 饱和钉 1.0、富者愈富）/ **C 去重视区**（known_facts 只拉 preference 类，跨类糯米碎片化→重抽）。**三次复盘关键纠正**：B"无衰减"为**假**——`decay_strength`(×0.998/天) 已在 `loop_runner:309` 每日运行，故砍掉新衰减子系统。**两层确定性卫生（LLM 只提议、Rust 校验，#1）**：Part1 新 `mind/memory_gate.rs`（category 白名单 + 噪声 key/value deny，store 写库前过滤；中文 trivia 靠 key `knowledge_question` 抓，6 单测）；Part2 `retrieve()` 删 reinforce 副作用→纯读 + 新 `reinforce_top(db, episodes)`（仅 converse + proactive genuine-recall 显式调用，零签名变更，避坑#4）；Part3 converse known_facts preference-only → `get_all_active(30)`。**不做**（复盘收敛）：知识图谱 / LLM judge 二次校验 / 新衰减 / importance 地板 / gate kill-switch（均见 ADR rationale）。**一次性数据治理**：expire 10 噪声 facts + 19 非地标 episode strength snap 回 importance（保留 current_reading + 糯米 relationship/preference 副本），DB 备份 `.bak-hygiene`。**测试全绿**：lib 287 / golden 29 / memory_gate 6 / **闭环2 ✅ pass**（途中发现并修**续⁸ 既存 bug**：lively 70% 概率早返回跳过到期 pending → `proactive.rs::generate` 一行守卫 `pending_due.is_empty() &&` 掷 lively 骰，到期提醒现确定性触发，70/30 多样性对无 pending 场景保留）。ADR `docs/decisions/2026-08-09-memory-hygiene-layer.md`（含三次多视角复盘全文）；治理脚本 `scripts/migrate_memory_hygiene.py`。commit `7f4af17`（卫生层）。→ 详见 §最近一轮 (续⁹)。

> **2026-08-09 续⁸ 自主冒泡：频率修复 + 灵性重构（记忆30/灵性70）—— ✅ 已收尾（lib 280 全过 + release 重建 exit0）**。用户反馈：① 频率太高（几分钟一冒）② 内容单一（全和糯米有关，要像真人突然找你聊天，可自言自语/撒娇）。firecrawl 调研 + AskUserQuestion 定（频率=30min 可配 / 比例=记忆30:灵性70）。**频率根因（bug）**：`commands.rs:470` 硬编码 `now-31min` 绕过 trigger_proactive 的 30min 门控 → 5min 轮询每次过 → 高频。**内容根因**：`proactive.rs::generate` 固定 query + 强制 memory anchor + "只聊这件事" + 无锚点沉默 → 永远糯米。**修复**：① 频率——AppState 加 `last_proactive_bubble: Mutex<Option<DateTime>>`，check_proactive 读真实值传 trigger_proactive（新 `min_interval_secs` 参数，config `proactive.min_interval_secs` 默认 1800），过门控即占位（conservative 宁少勿突兀，生成失败也不重复触发）。② 灵性——generate 入口 `rand` 加权（≥30 走 lively）：**memory(30%)** query 轮换池 5 条 + 无锚点降级 lively 而非沉默；**新 generate_lively(70%)** 不调 retrieve（省 embedding）、空 RetrievalResult 让 grounding_guard 自然禁编造用户记忆、注入**本地时段+情绪**驱动 prompt（自言自语/撒娇/碎碎念）。两编译坑已修（ThreadRng 非 Send→rng 收敛块内 drop；chrono Timetrait→format("%H")）。**lib 280（+3 测）/ check --tests ✅ / release 重建（1m10s+2.64s 前端）**。→ 待实跑：① 冒泡≈30min ② 内容不再全糯米、出现自言自语/撒娇（Debug Panel action=lively_bubble）。详见 §最近一轮 (续⁸)。

> **2026-08-09 接手完成 续⁷ 收尾**（用户"读 handoff、用 codegraph 了解代码、继续完成昨天未完成的聊天回复问题"）。codegraph + 源码逐处复核续⁷ 三处改动确在位（非仅信旧记）：converse.rs:415 `ThinkingConfig::disabled()`+`reasoning_effort=None` / grounding.rs:290 空记忆显式标记 + :293 非空 footer + :690 测试断言 / system.txt round-2 8 样例。exe 未运行→**rebuild 成功**（10:40:44，exit0/0警告/23.3MB）。② 向用户完整诚实报告（速度/性格/幻觉根因 + G6 trade + 方差）已交付。⏳ **当前无进行中任务**。**速度：用户确认可接受（max 4s/mean 2.7s 达标）→ gate/extractor 并行优化不做，留 backlog**（已分析两轮[续⁷ option A + 2026-08-09 AIRI 调研]，结论固化：gate 与 extractor 互相独立却串行[converse.rs:99→mod.rs:48]，`tokio::join` 并行预期砍 ~0.5–1s 首字延迟，代价是 Question/Discard 路由白跑一次 flash extract，需要时直接做不必再调研；AIRI 的本地零 RTT 复制不了[API-bound]）。仍待：用户实跑验收手感 + 决定 G6 follow-up。

> **2026-08-08（续⁷）速度+性格+幻觉根因 —— ✅ 已收尾（代码 lib 277 全过 + 已提交 `13e7dc8`；release 2026-08-09 10:40 重建）**。6 轮 A/B 跑完、代码改完验证。**接手三步已全做**：① **rebuild release** ✅（`npx tauri build --no-bundle`）② **向用户完整诚实报告** ✅：速度已解决（main 关思考，FULL max4s/mean2.7s/0超5s → **option A 不做**）/ 性格回归（round-2 soul block+8样例，human 4.07）/ 空记忆幻觉**已修**（grounding 显式标记，fresh 组全 0）/ **已披露 G6 越界 6/10 = 性格同源 trade**（样例教"上次说"framing = 用户要的"连过去"性格，不可全除）/ ~8pp run-to-run 方差。③ **可选（待用户定）**：用户在意 G6 → 软化 ex2/ex3 出处 framing（**削弱性格，需权衡**）或流式 chat 路径运行时阻断（流式已流出 token 无法撤回，本质受限）。**完整 6 轮 arc + 根因 + 代码改动清单见 §最近一轮 (续⁷)**。

> **2026-08-08 自主批次推进中**（用户授权："挨个推进 2,3,4,5,6 [审计清单里 5 个未实现/未接线项]，每完成一项自主验证、更新 HANDOFF + 新增待测试，不报告不询问；并砍掉走路相关计划"。逐项推进，每项 cargo test --lib / check --tests / tsc 绿后勾选并提交；release exe 在批次末统一 rebuild）：
> - [x] **Item 2 接线 is_deep_focus（P14.3）**：审计发现 `commands.rs:352,446` + `proactive.rs` 全硬编码 `is_deep_focus:false` → `trigger_proactive` Rule1（深度专注抑制）永不为真、空转。新 `perception/focus.rs`：纯函数 `update_continuous`（同一 Work app 累积 / 切换 Work app 重置 / 非 Work 重置）+ 后台 30s 采样线程（镜像 cursor::start）发布 `CONTINUOUS_WORK_SECS`/`IS_DEEP_FOCUS` 全局 atomic；阈值 25min（计划 P14.3）。两生产点接真实值（`get_perception` + `check_proactive`，均按 `enable_window` 门控 #6）；消费端 `trigger_proactive` Rule1 现在真生效。DebugSnapshot + DebugPanel 加 Focus 分区（#11 可观测）。**lib 261（+6 focus 纯函数测）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D8。纯后端+前端，release 需 rebuild。
> - [x] **Item 3 推进 A2 Scheduler**（架构对齐版，**兑现 08-07 deferral ADR 留的"可观测/可扩展"开口，不引入被否决的 trait-Tick 多态**）：新 `lifecycle/scheduler.rs`——进程级注册表 `Vec<JobStat>`（11 任务：5 core aliveness 常开 + memory_decay/closeness_drift 常开 + 4 能力 reflection/consolidation/relationship_review/lifecycle_cleanup 可关），`record(name,enabled,status,msg)` 上报 ok/skipped/error（skipped 不盖时间戳）+ `snapshot()` 读出 + `should_run(flag)` 纯决策。`loop_runner` 全 11 个执行点接 `record`（medium: homeostasis/pending_check/emotion_push/presence_watch/lonely_nudge；slow: memory_decay/closeness_drift 永远 ok + cleanup/reflection/consolidation/review 按 config 门控）。`config [scheduler]` 加 4 个 enable flag（默认全 on，#6 优雅退化）。新命令 `get_scheduler_stats` + DebugPanel **Scheduler** 分区（11 行心跳：✅/⏭️/⚠️/⏸️ + 节拍 + 最近时刻 + 消息，#11 可观测）。新 ADR `2026-08-08-scheduler-observability.md` 取代旧 deferral（核心否决仍立，只补开放方向）。**lib 267（+6 scheduler 纯函数测）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D9。纯后端+前端，release 需 rebuild。
> - [x] **Item 4 Grounding B 档运行时阻断**：07-31 主动开口幻觉 A 档（prompt rule 8 软约束）已修，此为 B 档运行时后备。两段：① **`check_groundedness` 加中文 claim 模式**（你说过/你之前提到/你最喜欢…10 个高精度模式）——原 EN-only 对中文回复零命中，且修了**隐藏 panic**：`+40 字节`窗口尾在 CJK 多字节码点中间切片会崩，抽 `ceil_char_boundary` 步进到字符边界。② **运行时阻断**（仅非流式主动气泡）：新 `proactive::grounding_guard`——首遍 `check_groundedness` 标记 → 追加"这是编造，重说一句，不确定就只表达此刻感受"系统消息 `llm.chat` 重试一次 → 仍编造则**抑制**（None，不冒泡），用户永不见幻觉。三生成器（generate/generate_welcome_back/generate_lonely_bubble）同款尾部全接（replace_all）。**流式 chat 路径不守卫**（已流出的 token 无法撤回），其 grounding 保持 warn-only 可观测（Debug Panel grounding_violations）。**lib 270（+3 grounding 纯测：中文幻觉/中文 grounded/CJK 窗口不 panic）/ check --tests ✅**。→ 待实跑见 D10。纯后端，release 需 rebuild。
> - [x] **Item 5 推进 A1 全局 BrainState**：Task#9 的 ConverseCtx 统一了 converse 的*外*参（9→1）；本轮补*内*层——新 `mind/brain_state.rs::BrainState<'a>`（text/emotion/relationship/pending_due/retrieval 五借用字段，构造即指针拷贝零 clone），`planner::plan` 签名从 5 散参 → `&BrainState`（body 用 5 行别名桥接，字节不变），converse 构造一次 `brain` 传入。**采纳边界=planner**（旗舰纯决策）；prompt builder / budget allocator 各取子集，强制单一 mega-state 反而捆绑不需要的字段（项目已否决的投机抽象，见 §A2 ADR）→ 留干净 follow-up。**踩坑#4 命中并修**：改 plan 签名断 golden(7)+questioning(3) 共 10 harness 调用点 → 全包 `BrainState::new(...)`（两 harness 加 `use BrainState`）。planner.rs 4 import 降为 `#[cfg(test)]`（仅测试用）。**lib 270 / check --tests ✅ 无警告 / planner 11 单测全过**。纯重构无行为变化，无需手感验。
> - [x] **Item 6 personality_drift_score 语义版**：规则启发式层（GROSS 漂移：话痨/卖萌/依赖）只抓"明显的"，对"简短、无 emoji、却冷淡/粗暴"的语气漂移盲视。补**语义漂移层**（cosine over embeddings）：`evaluation.rs` 加 `LIRI_PERSONA_REFERENCE`（4 句典型璃语气，温柔/好奇/安静 archetypal）+ 纯 `cosine_similarity(a,b)`（f64 累积防精度流失、零向量兜底 0 非 NaN、mismatched 长度取 min）+ `semantic_drift_score`（cosine 经 `SEMANTIC_FLOOR=0.4` 映射 [0.4,0.95]→[0,1]，与规则层 overall 同标度）。**架构 #1 纯函数**：模块只做 cosine 数学、永不碰 embedding 模型/DB，调用方喂向量 → 合成向量单测 CI 跑（5 测：identity/orthogonal/zero-vector 不 NaN/monotonic/clamp）；真实 BGE-M3 由 `tests/evaluation.rs` 新 Layer 3 端到端测接（镜像 embedding_ab_harness 的 `EmbeddingService::new+load().expect` 模式）。**实跑信号确认**：on-persona「嗯，这么晚了。早点休息吧。」cosine **0.851** vs off-persona「行吧，随便你，我无所谓。」cosine **0.781**——两句**规则层都给 1.0（盲）**，语义层区分出 0.07 gap，断言 on>off 通过。**lib 275（+5 semantic 合成向量测）/ check --tests ✅ / `--test evaluation` 6 规则测 + 1 semantic E2E 实跑 ✅**。→ 待实跑见 D11。纯后端，release 需 rebuild。
> - [x] **砍掉走路相关计划 + 代码**：核验发现走路**不只是计划**——`src/animation/spatial.ts` + `App.tsx` 有正在运行的「走回窝」代码。AskUserQuestion 确认后代码一并砍。删 `spatial.ts` 整文件 + `App.tsx` 拆全部接线（import/spatialRef/实例化/setNest/物理循环走回块/isWalking state+className）+ `styles.css` 删 walking 规则；计划/设计文档（implementation-plan 12.2 整节 + Walk 状态 + walk.wav + FSM 图 + design 走路行）全标「已砍除 2026-08-08」移除。**tsc ✅ / vitest 24 ✅ / build ✅**。详见 §最近一轮 (2026-08-08) 走路砍除小节。release 需 rebuild。
> 详见批次末 §最近一轮 (2026-08-08) 汇总。

> **2026-08-08（续）自主推进中**（用户授权："2，3 按顺序跑，跑完用之前的策略——每项自主验证 + 更新 HANDOFF + 新增待测试 + commit，不报告不询问"。2=B5 语义评估深化[LLM-as-judge + ≥30 golden 集]，3=散落小项 + 架构债收尾）：
> - [x] **B5-深化 三层人格评估 benchmark**（B5 重第三线落地）：规则层(续⑧) + 语义 cosine 层(Item6) 是廉价可 CI 跑的两道线、各有盲区；补**重第三线 LLM-as-judge**——读人格圣经给 persona_fit 0-10 + 命名漂移维度，是唯一能抓「客服腔/鸡汤/动作描写」语气漂移的线。新 `tests/personality_judge_harness.rs`（永久评测资产）：`PERSONA_JUDGE_PROMPT`（璃 6 维度 + NOT 清单）+ `judge_persona`（`chat_reflection` 0.1/2048 踩坑#3 + JSON 提取 + **3 次指数退避重试**——30 连发撞 rate limit，无重试会静默零分"假通过"）+ 30 golden 集（On 10 / Gross 10[chatty×3/cloying×3/clingy×4] / Subtle 10[cold/mech×2/preachy×2/over_pos×2/action/套宠物]）+ 三层聚合断言 + **judge 可靠性闸**（失败>3 即 fail）。**实跑（全 30 真实评分 0 失败 65s）**：judge On **10.0** vs Gross **1.3** vs Subtle **2.0**；规则层对 Subtle **0/10 盲**、cosine 0.66 vs 0.59。**check --tests ✅ / 实跑 ✅**。→ 待实跑见 D12。**纯测试资产无生产变更，release 无需 rebuild**。详见 §最近一轮 (2026-08-08 续)。
> - [x] **3a Alt+Space 全局唤醒（P11.4）**：真·系统级全局快捷键——任何 app 前台按 Alt+Space 都把桌宠召出来对话。新依赖 `tauri-plugin-global-shortcut` v2.3.2 + `lib.rs` plugin（handler：`w.show()`+`set_focus()`+`emit("show-input")`）+ setup 里 `register(Shortcut::new(ALT, Space))`（失败仅 warn，非致命）；前端新 `show-input` listener（镜像 restore-from-tray：`setAwayMode(false)`+`setInputVisible(true)`+rAF focus 输入框）。**cargo check ✅ / tsc ✅ / lib 275 ✅**。→ 待实跑见 D13。**⚠️ 权衡**：Alt+Space 会**全局接管** Windows 窗口系统菜单键（键盘开 Move/Size/Minimize/Maximize 失效，所有窗口）——设计文档钦定此键，若嫌扰可在 setup 改 `Shortcut`。后端 `.register()` 是 Rust 直调不走 IPC，**无需 capabilities 权限**。release 需 rebuild（新依赖 + 前后端）。
> - [x] **3b 害羞慢现气泡（后端 mood 标签）**：设计 §6.3 把「害羞」列为情绪→气泡样式表里 开心/调皮/平静/难过/担心/疲惫 的同级条目（"慢慢浮现, 先半透明"），§6.2 又说低亲密度（陌生）→ 拘谨。**后端落 mood 标签**：`emotion/state.rs` 新 `derive_mood_label_with_closeness(state, closeness)`——以 `label_for_mood_full` 为单一真相源算 base label，再在 **closeness < `SHY_CLOSENESS_THRESHOLD=20.0`**（镜像 lonely-nudge / planner-Rule4 的 `closeness>=20` 门，取反）时把**中性/正向**标签（平静/开心/调皮）覆盖为「害羞」，但**不掩盖真实 distress**（担心/疲惫/难过照常——她和陌生人也会担心/累/难过）。**不改 `derive_mood_label` 签名**（踩坑#4：5 调用点 + 测试零波及，纯加法新 fn）。`converse.rs` 两处 emotion 落库点（silence:224 / normal:460）改调新 fn——closeness 从已读的 `relationship`（:176）取，标签写进 DB，loop_runner 30s 重发的是这份持久化标签，故害羞会自然驻留到下次对话。set_emotion 调试命令保留原 `derive_mood_label`（debug 覆写应字面）。**前端**：`bubbleClassForMood` 加 害羞→`bubble-shy`；`styles.css` 新 `bubble-shy` + `@keyframes bubble-shy-reveal`（1.2s 慢浮现，30% 处 opacity 0.35「先半透明」，终态 opacity 1 可读——对比 happy/playful 的 0.3s 弹出，shy 是迟疑试探的慢揭幕）。**lib 277（+2 shy 单测：低 closeness 中性→害羞 / 不掩盖 distress）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D14。**纯后端标签 + 前端样式，release 需 rebuild**。详见 §最近一轮 (2026-08-08 续³)。
> - [x] **3c idle_weights JSON 化（数据驱动）**：`microBehavior.ts` 的 `IDLE_BEHAVIORS` 8 条微行为（weight/cooldown/emotion_modifier/min_closeness/sleepy）原本是硬编码 const 数组、数据和逻辑混在一个 .ts。抽成纯数据资产 `src/animation/idle-behaviors.json`，`microBehavior.ts` 改 `import ... from "./idle-behaviors.json"` + `as IdleBehavior[]` 类型断言（tsconfig `resolveJsonModule:true` 早开），`pickNextBehavior`/`applySleepyWeight` 逻辑零改动。**好处**：调权重/冷却/昼夜倍率只改 JSON 不碰逻辑（数据↔逻辑解耦，便于后续手感微调）。**纯前端行为不变重构**：vitest 24（含 7 microBehavior 测，A5 yawn/look_around 日夜比断言仍过——证 JSON 数据字节等价）/ tsc ✅ / vite build ✅（JSON import 打包正常）。**release 需 rebuild**（前端）。无需手感验收（代码层单测已覆盖，见 §最近一轮 续⁴）。
> - [x] **3d 架构债 BrainState 扩到 prompt builder+budget（B6 follow-up）—— 经评估主动关闭**（ADR: `docs/decisions/2026-08-08-brainstate-prompt-budget.md`）。Item 5 把 `BrainState` 采纳边界定在 planner，留此为"干净 follow-up"。复核五个目标函数（`build_system_prompt`/`build_qa_system_prompt`/`allocate_and_compress`/`allocate_qa`/`compress_system_prompt`）的实际签名与字段消费：① 它们都吃 `(retrieval, emotion, intent)`，而 **`intent` 是 planner 的 *输出***（`plan(&brain)→Intent`），不能入 BrainState（循环依赖）→ 强行扩留个 `(brain, intent)` 半 bundle 比现状更别扭，**省不掉 intent 参数**；② BrainState 的 `text`/`relationship`/`pending_due` 三字段这五个函数**一个不用** → 扩进去正是 `brain_state.rs` 注释 + §A2 ADR 已否决的「投机 mega-state」；③ 纯化妆重写 + 踩坑#4 级（5 函数签名 + 多 harness 调用点），零用户/正确性价值。**决策：不扩，follow-up 关闭，采纳边界终态=planner。** 也评估了方案 B（窄类型 `PromptCtx{retrieval,emotion,intent}`）：比方案 A 干净但不捆绑问题、边际收益不抵新类型 + 截断 retrieval 碍事，现状 3 参紧签名已自解释。同步更新 `brain_state.rs` 顶部注释指向 ADR。**纯决策无代码行为变更**，无需 rebuild。详见 §最近一轮 (2026-08-08 续⁵)。
> - [x] **批次末 rebuild release exe**：3a/3b/3c 改了前后端 → `npx tauri build --no-bundle`（踩坑#6，先确认 desktop-pet.exe 未运行）。**exit 0**，产物 `D:\cargo-target\desktop-pet\release\desktop-pet.exe`（11:40:31 新鲜，51.8s Rust release + 2.1s 前端，CSS hash `index-HCg0t6XF.css` 含新 bubble-shy）。桌面快捷方式同路径免改。**3a-d 全部完成 + release 已重建 = 本批（2，3）收尾。** 待用户实跑 D12-D14（B5 benchmark / Alt+Space / 害羞气泡）。

> **2026-08-08（续⁶）真人感 prompt 调教（用户驱动，已收尾）**：用户"回复不够真人感、不需要每问都加提问"。四步闭环：① `client.rs` `thinking:{type:disabled}` 关 gate/extractor 思考（提速+根治空 content 踩坑#3，commit 8aa0d61）② harness 扩到 150 例+真人感指标+`CASE_FILTER`（eec094c）③ 基线 150 条诊断：提问结尾率 35%，G12分享 80%/G11琐碎 60%/G3闲聊 50% 严重超标，G5 喜讯"哇"克隆开场 5/10 ④ 改 prompt A/B/C（b5afac6）：system.txt 话术 engage"可不问"+4 条反 AI 味（禁客服收尾/禁情绪标签/允许自己的状态/像随手发消息）；样例 4→6 条仅 1 问；`grounding.rs` format_intent engage"then ask ONE"→"may ask ONE… often no question"。**复测**：提问结尾率 35%→**14%**，G3 50%→10%、G12 80%→30%（−50pp）、G5 哇开场 5/10→0/10、"想听细一点的我可以再讲"消失。**诚实权衡**：human_like 4.24→4.11（judge 一致"稍显简短"=变短非变冷）；模板词 23→23（构成迁移哇→恭喜，喜讯道恭喜属正常非 AI 味）；G14 碎念残留 40% 提问皆对天然邀请追问的输入（在吗在吗/啊啊啊），压低反损自然。**对比报告** `docs/review/realism-report-2026-08-08.md`；评测快照 `-baseline`/`-post`。**release exe 已 rebuild 17:48**。→ 待用户实跑验收手感；若嫌 G5 偏冷可微调 A3"一个字"措辞（见报告可选微调）。

> **2026-08-07 自主批次推进中**（用户授权长程自主："按优先级推进所有后续内容，每项自测后更新 HANDOFF，不询问；待实跑项统一整理"）。逐项推进，每项自测（cargo test --lib / check --tests / tsc）绿后勾选。**release exe 在批次末统一 rebuild**（中间项都以库单测 + check 编译通过为正确性证据；批次末 Task #14 前一次性 `npx tauri build --no-bundle`，避免每项重构都重编一次前端嵌入）：
> - [x] **Task #8 鲁棒性加固**：① main 空回复重试——converse `chat_stream` 把 `on_token` 改 `mut`、传 `&mut on_token` 复用，content 空时重试一次（镜像 extractor 重试；flash reasoning 吃光预算 finish_reason=length 空 content 的坑#3 瞬态）。② harness 启发式误报——Acknowledge/ForgetAck 关键词表加现实同义措辞（记着/记心里/放心吧/帮你记 + 不提/不会再/抹掉/清掉），治 705/1002「语义对无关键词」误报。**lib 259 / check --tests ✅**。纯后端 + 测试，release 需 rebuild。
> - [x] **Task #9 B6 BrainState**：converse 9 参 → `ConverseCtx<'a>` 统一快照（8 个引用字段 + `on_token` 留作独立泛型 `FnMut`——回调是流式旁路非状态，塞进 struct 会让整体变泛型）。函数体用 8 行别名桥接（`let text = ctx.text;`…），400 行 body 字节不变，最低风险。6 处调用全改：commands.rs（生产）+ memory_recall(×3)/conversation_harness/prompt_quality_harness。harness 里的 `get_context()` 临时 Vec 绑定本地避免跨 await 临时生命周期问题。**check --tests ✅ + lib 259 ✅**。纯机械包装，行为不变。
> - [x] **Task #10 B7 Scheduler —— 经评估主动搁置**（ADR: `docs/decisions/2026-08-07-scheduler-deferred.md`）。原计划 §A2 假设 Body 跑在 Rust（`ticks_1s` 动画/物理），但实际遵循原则 #5：Body 在前端，Rust 无 1s 动画 tick。审计 Rust 定时器仅 medium(30s)/slow(1h)/cursor(ms 感知)/两个 one-shot 启动——`start_life_loop` 已是唯一注册中心，无多态无注入需求，引入 trait object 是投机抽象（#9/#10）。高风险重写时序核心、零用户价值。搁置，何时复议见 ADR。
> - [x] **Task #11 记忆可视化编辑**：Debug Panel 从只读→可编辑。后端 3 新命令（复用既有 DB accessor，不写裸 SQL）：`forget_fact(id)`（`facts::expire_by_id` 软删，保审计轨/revive 路径）、`delete_episode(id)`（`episodes::delete` + `vectors::delete` 同步向量，拒删地标）、`set_emotion(EmotionEdit)`（`update_fields` + 重导 mood_label + 即时 emit `emotion-update` 让脸马上变）。pending 取消复用既有 `resolve_pending_event`（不另起路径）。`DebugFact` 加 `id` 字段。前端：Facts/Episodes/Pending 每行 ✕ 按钮（fact/episode 带 confirm 防误删）+ Emotion 编辑器（5 滑块 Apply）。2s 轮询 + mutate 后即时 refresh。**check --lib ✅ + lib 259 ✅ + tsc ✅**。→ 待实跑：F12 打开面板手动测编辑（见 verify-checklist）。
> - [x] **Task #12 loneliness 收尾**：① lonely-nudge 加 Sleeping 守卫——`App.tsx` 监听器加 `if (fsmRef.state===Sleeping) return`（镜像"该睡了"nudge 的同款守卫，睡着不冒"想你了"，原则 #12）。② `pet_head` 互动降孤独 -0.1（摸头是注意力的反面=孤独缓解；poke 是逗弄不减；~0.1 抵 15min idle 增长，一次摸头明显安慰但不让缓慢累积失效）。**tsc ✅ + check --lib ✅**。→ 待实跑：深夜 Sleeping 时确认不冒 lonely 气泡 + 摸头后 loneliness 回落（见 verify-checklist）。
> - [x] **Task #13 死代码清理**（核实后修正前提）：① **`trigger_proactive` 并非死代码**——`commands.rs:451` 生产调用它（前次"6 调用全测试"的判断过时/错误），**保留不动**。② **删 `emotion/homeostasis.rs` 整文件**（`apply_drift`+私有常量+`drift_toward`+4 测试）——生产用 `db::emotion::apply_homeostasis_time_aware` 自带一套 `TAU_*`/`drift_toward`，homeostasis.rs 全程零生产调用；**且其 `TAU_STRESS=3600` 与生产 `7200` 已分叉，留着会误导**（典型双实现坑）。同步删 `emotion/mod.rs` 的 `mod`/`pub use` + golden `GC_018`。③ **`tick_needs` 保留**——虽是测试专用包装，但正确委托给生产用的纯函数（不分叉、不误导），删它低价值中风险（需改写 needs.rs 共享文件的测试），留 + 注释说明。**check --tests ✅ + lib 255（原 259 −4 homeostasis 测试）✅**。
> - [x] **Task #14 统一待实跑清单**：扩写 `docs/verify-checklist.md`（原有 Body/circadian/sleep A5/A4/B3/A6 不动），新增「本批次验收」一节 D1-D7：D1 Debug Panel 记忆编辑（forget fact/delete episode/cancel pending/emotion 滑块）、D2 loneliness 主动找你、D3 loneliness 睡着抑制、D4 摸头降孤独、D5 Forget 流程、D6 QA 直答、D7 rest_need 疲惫眼——全部用本批次新增的 **Emotion 编辑器秒级触发**（原本需等几小时）。附「不易快速验收」表（关系 review/空回复重试/surfaced thought/B6 重构）。顺带给 Debug Panel Brain 行加 Lonely 显示（D2/D4 观察 loneliness 用）。**tsc ✅**。交付：用户照此清单 dev 模式手动验手感。
> 详见各任务 §最近一轮 条目（批次末汇总）。

> **2026-08-07（续）更新 · 激活 loneliness——璃会"想你"**：用户"读 handoff、用 codegraph 了解代码、继续开发"。AskUserQuestion 在 4 方向里确认走 **激活 loneliness**（服务"陪伴"北极星，未受阻低风险）。codegraph 核验发现 **loneliness 是最后一个死情绪字段**——`apply_homeostasis_time_aware`（生产 homeostasis）只更新 mood/energy/social/stress/rest_need，从不更新 loneliness（08-04 修了 rest_need，loneliness 漏了），冻结在种子值 → planner Rule 4「loneliness>0.6 + closeness≥20 → 主动陪伴」永远到不了。两段落地：① **核心（镜像 rest_need 修法）**——`needs.rs` 抽 `tick_loneliness` 纯增长规则 + 接进 `tick_needs`（DRY）；`apply_homeostasis_time_aware` 调它 + SQL UPDATE 加 `loneliness=?7`（renumber ?8）；② **主动气泡（镜像 welcome-back/proactive 模式）**——新 `generate_lonely_bubble`（镜像 generate_welcome_back：retrieve 锚 + Intent goal=accompany/action=lonely_nudge + 1 句温柔 prompt「别黏人别问问题逼答」+ LLM 4096 坑#3）+ `lonely_canned`（react.rs mood 分档降级 #8）+ `lonely_bubble` 命令 + 注册；`loop_runner::check_lonely_nudge`（门控 loneliness>0.6 + closeness≥20 + presence Active + 非对话中 + 30min 线程本地 cooldown → emit "lonely-nudge"）；App.tsx listener → invoke → showBubble。**closeness≥20 门控保证早期关系不主动找你**（Liri 非依赖人格安全阀）。**全程不改 fn 签名**（踩坑#4：新 fn + 新 action 字符串 + SQL 参数）。**lib 259 / check --tests / tsc / build / vitest 24 全绿**。**待实跑**：dev 攒 closeness≥20 + 离开 ~1.7h（loneliness 到 0.6）→ 看她主动冒"想你了"气泡；或回来后她回复带 accompany 暖意（planner Rule 4）。**release exe 已重建**（npx tauri build --no-bundle，D:\cargo-target\desktop-pet\release\desktop-pet.exe，前端+后端都改）。详见 §最近一轮 (2026-08-07 续)。**当前无进行中任务**。

> **2026-08-07 更新 · 关系进展摘要（Hermes 后台 review）落地**：用户"读 handoff、用 codegraph 了解代码、继续开发"。AskUserQuestion 在 4 个方向（关系进展摘要 / 激活 loneliness / 记忆可视化编辑 / 架构债 BrainState）里确认走**关系进展摘要**（服务"懂你"Soul 闭环深化）。每 15 个新 conversation episode，后台 reflection 模型回顾产出 1-2 句"你们关系最近状态"总结（璃视角、free text），注入为 always-on `[Relationship]` 区块——让她即使当前话题检索不到相关记忆，也带着对关系整体的理解。**3 新文件 + 6 改文件，全程不改 fn 签名（踩坑#4）**：新表 `relationship_reviews`（migration v3 + `db/relationship_reviews.rs`）+ 新 `soul/review.rs`（镜像 reflection.rs：纯谓词 `should_run_review` + `run_review` + `maybe_run_review_if_due`）+ RetrievalResult 加 `relationship_review` 字段走现成注入管道（`retrieve` 填充 → `format_memories` 输出 `[Relationship]`）+ slow_tick 调度 + budget RELATIONSHIP=80 + system.txt 指引。**踩坑#4 变体已修**：RetrievalResult 加字段后同步所有显式构造点（lib retrieval/budget×4/grounding×2/planner×2 + harness golden×7/evaluation/questioning；converse 用 `::default()` 自动 None）。**lib 257 / golden 30 / evaluation 6 全绿，check --tests ✅**。**待实跑**：dev 攒≥15 记忆后 slow_tick 触发 → DB 看 `relationship_reviews` 有行 + 对话语气带关系理解。**release exe 需 `npx tauri build --no-bundle`**（system.txt include_str! + 后端 + migration v3）。详见 §最近一轮 (2026-08-07)。**当前无进行中任务**。

> **2026-08-05（续⑤）更新 · 100 条提示词质量评测 4 轮迭代完成（98/100 通过，0 真乱扯）**：用户"自己写一套测试，100 条对话多方面测试提示词回复质量，汇总表格审查"。新增 `tests/prompt_quality_harness.rs`（**永久性评测资产**，100 条 × 10 组：G1 知识/G2 技术/G3 闲聊/G4 情绪/G5 喜讯/G6 记忆(种子DB)/G7 提醒/G8 边界/G9 关系/G10 修正遗忘；走完整 converse 链路 + 启发式硬检查 + LLM-as-judge 评分，写报告 `docs/review/prompt-quality-report-YYYY-MM-DD.md`）。**4 轮迭代修复链**（每轮 100 条实跑验证）：R1 发现 extractor 空输出整轮崩（4/100）→ 修 extractor 重试+降级；R2 发现 gate/correction 类别空洞 reasoning 爆预算（gate.txt 排除规则副作用）→ 修 gate/correction 重试+降级 + gate.txt 给排除项明确归宿 store_full + QA 模式加防编造句；R3 发现 extractor 算错日期（"明天"→2026-01-02）→ 修 extractor 注入本地今天日期+星期（{today} 占位，不改签名）；R4 启发式调优（合理澄清反问不再误报）。**最终：98/100 硬检查通过，0 真乱扯，知识问答 20/20 满分直答，记忆组 10/10 引用（"你记得我在忙什么吗"→"记得，你在找实习"），日期全对（下周二→2026-08-11）。** 剩余 2 fail 均启发式误报（705 语义已确认但无关键字 / 1002 同上）+ 1 偶发空回复（407，1/100 LLM 空输出）。judge 标"幻觉"3 条全为不知种子/注入机制的误判。**lib 248 passed**。**release 已重建**（本轮改动 gate/extractor/correction/gate.txt/extractor.txt 均 include_str! 或后端）。待办：407 类 main 空回复可加重试（低优先）。

> **2026-08-05 更新 · 选择性遗忘扩展至 fact/pending + FTS5 可行性证伪**：用户"读 handoff、用 codegraph 了解代码、按优先级继续开发"。**① FTS5 证伪（决定性）**：HANDOFF 把 FTS5 全历史检索标为"最高 ROI follow-up"。写 throwaway probe 测 bundled SQLite 三分词器对中文 2 字查询 '火锅' 的 MATCH——**FTS5 可用但 trigram/unicode61/ascii match count 全 0**（trigram 需≥3 字 / unicode61 不分 CJK / ascii 只认 ASCII；旧记"sqlite-vec 自带 fts5_cjk"**错误**——fts5_cjk 非标准、sqlite-vec 不捆绑 FTS5 分词器）→ **FTS5 对中文不可行，从 backlog 移除，勿再尝试**（除非引入 jieba 可加载扩展 / Rust 分词，远超干净 follow-up）。**② 转向选择性遗忘 fact+pending**（08-04 续 episode MVP 的 deferred scope："fact/pending 遗忘未做"，结构镜像 episode）。新 `forget_best_match` 调度器扫 episode/fact/pending 三路、各自 0.7 置信度门、取最高分执行一条（episode 硬删+向量清 / fact 软过期 `expire_by_id` / pending `mark_resolved`）；用户不说忘哪种 → 扫三种挑最佳；歧义时软动作（fact 过期可恢复）自然压过硬删。新 `char_overlap`（bigram 重叠系数 `|A∩B|/min`，修 Jaccard 把"忘掉咖啡"/"咖啡"稀释到 0.33 的问题→1.0）。**验证全绿**：lib **247 passed**（240+7）/ `cargo check --tests` ✅。**待实跑**：dev "忘掉X"（X=偏好/提醒）→ 确认回"好，我忘了"+ 后续不召回（Debug Panel 看 fact valid_to / pending status）。**release exe 需 `npx tauri build --no-bundle`**（纯后端 + gate.txt include_str!）。详见 §最近一轮 (2026-08-05)。

> **2026-08-04（续④）更新 · 审查并修复 opencode 续③ QA 直答代码的 4 处问题**：用户"代码库新增的是 opencode 写的，针对回复没逻辑的问题，看看 handoff 检查代码"。审 opencode 续③（QA 直答路由 + Hermes compress_conversation + Milestones）后发现并全修：① **[中] QA 模式丢失身份层**——`converse` qa_mode 用 `RetrievalResult::default()` 把 persona/relationship/user_profile 连同记忆一起丢了 → `build_qa_system_prompt` 的 `[Persona]` 退化为通用 fallback，璃的知识直答叫不出用户名字/丢关系。修：qa_mode 仍跳过 episodes/facts（防跑偏），但**补加身份 DB 读**（persona/relationship/user_profile，廉价无 embedding）→ 直答保留璃身份。② **[小] QA Debug budget 错**——prompt_debug 用正常 budget(2005)，但 QA 无 [Memories]。修：新 `qa_system_prompt_budget()=505`（PERSONA+EMOTION+INTENT+SCAFFOLD），qa_mode 用它。③ **[小] qa_mode 未强制 action**——罕见 planner silence 会吞掉问题答案。修：qa_mode `intent.action="normal"`（问题必答）。④ **[小] QA 仍跑 grounding check**——空 retrieval 只会误报。修：qa_mode 跳过 check_groundedness。**确认无问题的部分**：compress_conversation 重写逻辑正确（user 永留/驱逐最老 assistant/时序复原）、gate 4096（坑#3 已修）、Question 跳 extractor 合理。**验证**：lib **240 passed**（238+2 新：qa budget 值 + QA 保留身份）/ `cargo check --tests` ✅。**待实跑**：dev 问知识题确认璃叫得出你名字（fix#1）。**release exe 已重建**（`D:\cargo-target\desktop-pet\release\desktop-pet.exe`，08/04 22:30，`npx tauri build --no-bundle`，含本会话全部改动：#10 rest_need/speedModifier + 选择性遗忘 + QA 4 修复；桌面快捷方式同路径免改）。

> **2026-08-04（续③）更新 · QA 直答路由 + 提示词正向重写 + Hermes 记忆优化落地**：用户反馈知识问答体验差（"harness 是什么"被硬套宠物话题、回复生硬）。三部分完成：
> ① **Question 直答路由**（治"硬套"）：gate 新增 `question` 分类（gate.txt + `GateRoute::Question`）→ ingest 跳过 extractor（省一次 LLM 调用）→ converse QA 模式：跳过记忆检索（RetrievalResult::default()）、清空 intent memory anchor/engage 指令、跳过 pacing、跳过念头注入 → 新 `build_qa_system_prompt`（人格+情绪+中文直答指令，**无 [Memories]/[Grounding Constraint]**）+ `budget::allocate_qa`（QA 版 allocate_and_compress，签名不动避坑#4）。
> ② **system.txt 正向重写 + mes_example**（治"生硬"）：14 条禁令清单 → `[How to talk]` 正向说话方式 + **4 条中文示例对话**（知识直答/分享跟进/记忆自然引用/闲聊）。保留 persona 契约回归网（evaluation.rs）全部字样：6 维人格/话痨卖萌依赖/严禁编造/璃。改了一个 stale 断言：`test_empty_memories_section` 的 `[Memories]` 检查改 `- [Fact]`（system.txt 正文现在也提标签字样）。
> ③ **Hermes agent 落地**（调研 NousResearch/hermes-agent 225k⭐，记忆最佳实践）：**用户消息永不压缩**（`compress_conversation` 重写——user 消息 verbatim 全保留、超预算先挤 assistant 回复，修"用户倾诉被截断失真"）+ **关系账 [Milestones] 分组**（landmark episode 单独区块注入、不重复进 [Memories]，Hermes 双账本思想适配陪伴场景）。其余 Hermes 优化已天然满足（压缩/辅助走 flash、consolidation 容量跳过重试）或记 follow-up（FTS5 全历史检索、关系进展摘要、记忆可视化编辑）。
> **会话前半段**：Debug Panel 退出通道（面板全窗口覆盖挡住右键 → 加粘性工具栏 ✕关闭面板/⏻退出桌宠，走 handleQuit→quit_app）+ 快捷键重构（新 `src/shortcuts.ts`：`e.code==="KeyD"` 防中文输入法截获 key="Process"、Esc 无条件关面板）+ gate/correction `max_tokens` 2048→4096（踩坑#3 复发：flash reasoning 吃光 2048 预算 content 空 JSON 崩）+ 主对话模型切 `deepseek-v4-flash`（AppData config）。
> 验证：lib **238 passed** / golden 30 / harness 编译 ✅ / 前端 tsc ✅。**release 已重建**（npx tauri build --no-bundle，exe 18:07→最新，桌面快捷方式无需动）。

> 📋 **待办（下一会话起点）· QA/新提示词 runtime 实跑**：① `npm run tauri dev` 问知识问题（"什么是X"/"帮我解释报错"）→ 确认直答不套宠物、F12 面板 Last Turn 显示 route=question；② 分享类消息（"我今天…"）确认示例风格生效（简短+一个真问题）；③ 聊天几次后确认旧记忆仍自然引用（[Milestones] 里程碑出现）。**Hermes 高价值 follow-up**：FTS5 全历史检索（零成本毫秒级回忆，sqlite-vec 库自带 fts5_cjk 中文分词，替代部分 embedding 召回）、"关系进展摘要"（后台每 N 次对话异步总结，对应 Hermes 后台 review）、记忆可视化编辑（Debug Panel 只读→可改）。

> **2026-08-04（续）更新 · 选择性遗忘 episode MVP**：用户"开做选择性遗忘，做完跑 50 条功能测试，遇问题自检修复"。实现**用户主导的主动遗忘**（lifecycle_cleanup 的用户控制版）：用户说"忘掉X"→ gate 路由 `Forget` → 复用 retrieve 语义匹配最佳 episode → **置信度门在 `score_breakdown.semantic`（0.7，非 total score——total 混了 strength/recency，强近期无关记忆也能高分→删错）** + landmark 保护 → Rust 删 episode 行 + `vectors::delete` → converse 注入确认提示（"好，我忘了"，**禁复述**；无匹配则诚实"不记得"）。新 `mind/forget.rs` 模块（镜像 correction.rs）+ `db/episodes::delete`（保护 landmark）+ gate Forget 变体 + gate.txt 类别 + IngestionOutcome 加 `forget` 字段 + converse 提示。**全程不改 fn 签名**（踩坑#4：只加枚举变体 + struct 字段 + 内联分支）。**8 新单测**全绿。**自愈**：跑 golden 时 C 盘满（0.5GB，os error 112）→ 诊断 `src-tauri/target/release` 是 07/28 陈旧残留（release 早走 D 盘，活动 exe 在 D 08/03）→ 删之腾 2.31GB → golden 增量编译过。详见 §最近一轮 (2026-08-04 续)。

> 📋 **待办（下一会话起点）· 选择性遗忘 runtime + 扩展**：① **runtime 实跑**：`npm run tauri dev` 攒几条记忆后说"忘掉我说的X"→ 确认她回"好，我忘了"且后续不再召回（Debug Panel 看 episode 删了没）。② **MVP 边界（可选 follow-up）**：当前只删 top-1 episode、阈值 0.7 需真实样本调、无多轮消歧义（低置信直接"不记得"而非反问"你说的是…"）、fact/pending 遗忘未做。详见 §最近一轮。

：用户选"#10 生命感收尾"方向（非字面最高优先的 B6/B7 架构债——那是对运行中代码的推测性重构，违反"不重构没坏的东西"）。本轮补全两个长期标"低优先/follow-up"但服务北极星#10、且补全**已半接线系统**的缺口。**① rest_need 后端暴露+激活**——审计发现 `tick_needs`/`apply_drift`（emotion/needs.rs、homeostasis.rs）**只在自身测试里被调用、生产从未调**（生产走 DB 层 `apply_homeostasis_time_aware`，只漂移 mood/energy/social/stress，从不碰 rest_need）→ 单纯"暴露"会显示恒定种子值、毫无效果。故同时激活：新 `tick_rest_need(r,e,t)` 纯函数（低能量增长 + **恢复项 exp 衰减**，修原 tick_needs 单调只增永不恢复的设计缺陷）+ `tick_needs` 复用它 + 接进 `apply_homeostasis_time_aware`（UPDATE 加 rest_need 列）+ `EmotionResponse`/From/emit 三处加字段 + 前端 `EmotionData`/`toEmotionVector` 读取。效果：低能量时 rest_need 增长 → emotionDriver 半眯眼真的可见（之前恒 0）。**② circadian speedModifier 接动画速度**——`circadian.ts` 早输出 speedModifier（Morning 1.2 / DeepNight 0.4）但**零消费方**（只有 sleepiness 喂了 fsm）。Live2DCanvas 加 `speedModifier` prop → per-frame `focusTickerFn` 设 `app.ticker.speed` → 库的 idle 呼吸/眨眼/motion/physics 全局随昼夜变速（深夜真的变慢）。**验证全绿**：lib **227 passed**（226+1 恢复测试）/ `cargo check --tests` ✅ / `tsc` exit 0 / `vitest` 24 / `build` ✅（2.60s）。**待实跑**：dev 看 ① 低能量半眯眼（需攒状态或 CDP 注入 high rest_need）② 深夜 ticker.speed=0.4 全局变慢（`__pet.setHour(3)` 即时切换）。详见 §最近一轮 (2026-08-04)。**release exe 需 `npx tauri build --no-bundle` 才生效**（前端+后端都改了）。**当前无进行中任务**。

> 📋 **待办（下一会话起点）· runtime 实跑 #10 两项**：`npm run tauri dev` → ① 低能量半眯眼：Debug Panel 或 CDP 把 rest_need 拉高，肉眼确认眼睛半闭（emotionDriver EYE_REST_GAIN 生效）② 深夜变慢：`__pet.setHour(3)`（dev-only 钩子，重写 getHours 模拟 DeepNight）→ 观察呼吸/眨眼/motion 明显变慢（ticker.speed=0.4），`setHour(10)`（Morning）→ 略快。静态全过，仅剩渲染确认。验完勾掉。

：用户"继续 B4,B5 推进"。**B4-余余 两分区补全**（#11 Explainability 收尾）：① **AnimFSM**——fsm.ts 加 `getHistory()` getter 暴露末 5 微行为 history；App 传 `anim={state:behavior, history}` 给 DebugPanel；新 AnimFSM 分区显示当前态+recent history（"她现在在干嘛"）② **Prompt-token**——budget.rs 加 `system_prompt_budget()`（=2005）；converse 加 `PromptTokenDebug{system_tokens,input_tokens,budget,conversation_turns}` 挂 ConversationResult（**续③ 同款不改 fn 签名**，silence=None/normal=Some，在既有 system_tokens log 处复算）；commands 镜像 `DecisionPromptToken` 投影进 DecisionTrace；DebugPanel Last Turn 加 "Prompt: sys N/budget M | input K (N turns)"。**B5 Golden 评估框架**（审计确认原无 evaluation.rs/personality_drift_score/CI）：新 `src/mind/evaluation.rs`（DriftKind Chatty/Cloying/Clingy + DriftReport + `personality_drift_score` 规则启发式 + 7 单测）+ `tests/evaluation.rs`（**Liri 人格契约回归网** 4 测：6 维度/狐灵身份/NOT-list/严禁编造，锁续② 落地的人格 + 2 drift 端到端）。**验证全绿**：lib **226 passed**（219+7 eval）/ `cargo check --tests` ✅（evaluation.rs 编译 + 既有 harness 无破）/ `--test evaluation` 6 passed / `tsc` ✅ / `vitest` 24 / `build` ✅（1.89s）。**B4 前端两分区待 dev 实跑确认渲染**（静态全过；要看 AnimFSM/Prompt 分区需 `npm run tauri dev` 发消息开 Debug Panel）。详见 §最近一轮 续⑧。
> 📋 **待办（下一会话起点）· B4 两分区 runtime 实跑**：`npm run tauri dev` 发一条消息 → F12（或 Ctrl+Shift+D）开 Debug Panel → 肉眼确认 ① **AnimFSM** 分区显当前 state + recent history ② **Last Turn** 内显 `Prompt: sys N/budget M tok | input K (N turns)`。静态全过（compile/types/build/单测），仅剩渲染确认；后端 PromptToken→snapshot 链路续③ 已验活着。验完勾掉。

> **2026-08-03（续⑦）更新 · sleep 内容首次有测试**：用户"sleep相关的内容是不是还没有做测试"——确认 A4/A5/B3 全标"待实跑"、**前端零测试**（Rust 219 vs 前端 0）。补：① **加 vitest**（devDep + vitest.config.ts，node env，`npm test`/`test:watch`）② **抽纯逻辑**——`sleepLogic.ts::shouldAutoSleep`（从 App.tsx auto-sleep 条件抽出 A4 触发谓词）+ `microBehavior.ts::applySleepyWeight`（A5 公式 `w*=1+(sleepy-1)*sleepiness` 抽出，pickNextBehavior 复用）③ **24 前端单测**：circadian(10)/sleepLogic(7)/microBehavior(7)，覆盖 A5 输入（DeepNight 0.9/Morning 0.1 + 5 时段 + 边界）、A4 触发（DeepNight-only/非已睡/非 think-talk/idle 严 >阈值 各分支）、A5 效果（yawn 夜↑~3×/look_around 夜↓/白天 no-op/clamp）。**验证**：`npx vitest run` **24 passed** / `tsc --noEmit` ✅ / `npm run build` ✅（1.97s）。详见 §最近一轮 续⑦。**+ runtime CDP 验证（同轮）**：`npm run tauri dev` + `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` → Node WebSocket 连 CDP 驱动 `window.__pet`——A5 setHour(3)→DeepNight/0.9 & setHour(10)→Morning/0.1 ✅、A4 sleep()→"sleeping" & wake()→"hum" ✅、B3① 睡着 probeNudge×10 零气泡 ✅（awake sanity 1/15 证 nudge 本身没坏）、截图分析 sleeping=**闭眼**/awake=**睁眼** ✅。**唯一仍待人验：B3② sleep 音效**（`sound.sleep()` 入睡播放，我听不到——dev 仍开着，可右键→DevTools→`__pet.sleep()` 亲耳听）。**当前无进行中任务**。

> **2026-08-03（续⑥）更新 · 清测试**：用户"是不是还有几个测试没做？先清测试"。**发现 `cargo test --test golden_conversations` 有 2 个 stale 测试失败**（lib 219 一直绿，掩盖了集成测试回归）：① **gc_003_emotion_consistency**——断言"高 stress+焦虑→silence"，但 planner Rule 2 **故意改成** anxiety→`care/normal/gentle`（打破 焦虑→stress→silence 反馈环；planner.rs:112-127 注释详述 + 单测 `test_anxiety_routes_to_care` 钉契约；golden 测试漏同步）② **gc_012_first_run_seeds_persona**——断言 `trait_key=="gentle"`，但 续② Liri 迁移后 `seed_persona` 播种中文维度 `温柔`（firstrun.rs:32-39）。**两者均改测试不改生产**（生产是有意行为，测试 stale）。修复后 golden **30 passed** ✅。**全确定性测试绿**：lib 219 + golden 30 + questioning pacing 1 + embedding_ab 1 = **251**。**闭环1 `memory_recall` real-LLM 复验通过**（多文件 Rust 改动后确认核心链路：seed 持久化→noise 抑制→跨会话 recall "糯米" 全 ✅，46s）。详见 §最近一轮 续⑥。**当前无进行中任务**，下一会话按 B4-余余（AnimFSM 前端 / Prompt 动态 token）→ B5（Golden 评估，待 Liri 稳定）推进。

> **2026-08-03（续⑤）更新 · 两 follow-up**：① Settings 下载按钮 Qdrant 401→Xenova（hf-mirror）+ REQUIRED_FILES 加 `model.onnx_data` + download_all 处理 `onnx/` 子目录 external data；② **暂时离开 = 最小化到系统托盘**（lib.rs 建 TrayIcon 左键 click 恢复 + emit restore-from-tray；commands `hide_to_tray`；App handleAwayMode 调 hide + listener 清 awayMode；**原只设 awayMode 标志窗口根本没隐藏**）。tray-icon feature 早启用只差接线。验证：219 passed + tsc + check --tests + release rebuild(19:20) + 启动 sanity（进程活+vectors14 幂等）。**tray 交互（右键离开→托盘→点击恢复）+ 下载按钮待用户手动验证**（OS 层）。详见 §最近一轮 续⑤。

> **2026-08-03（续④）更新 · embedding 接入 + 检索质量翻倍**：用户"下载 embedding 装 D 盘 + 对比测试看提升"。已完成：① BGE-M3 装入 `D:\models\bge-m3`（Xenova/bge-m3；原 Qdrant/bge-m3-onnx 已 401）② config→D 盘 ③ **修 ort rc.12 加载 bug**（Level3→All；记忆 `ort-rc12-embedding-load-bug`）④ **加 backfill**（历史 episode 自动向量化，真实 DB 0→14 验证）⑤ benchmark（语义 Hit@3 33%→67%、avg sem 0.035→0.741≈21×）⑥ release rebuild + 端到端。详见 §最近一轮 续④ + 记忆 `bge-m3-model-location`。**当前无进行中任务**。**follow-up**：download.rs 的 HF_BASE_URL 仍指失效 Qdrant（Settings 下载按钮坏，手动下载已绕过）；B5 Golden 评估。

> **2026-08-03（续③）更新 · 深度审计 + #11 可观测簇**：用户要求"对计划未完成部分做深度审计 + 列优先级更新 HANDOFF + 按优先级继续开发"。审计对照 `implementation-plan.md` P0-P17/A1-A2 逐项核验**代码实际状态**（非仅信 HANDOFF 旧记录），结论见 [§审计 (2026-08-03 续③)](#审计-2026-08-03-续③深度审计--代码级核验)。**重排结论**：三闭环已全完成、生命感主轴在维护态，**Liri/Spine（真正的 #10 下一步）受阻于 Liri.spine 资产未交付** → 当前最高 ROI 且**未受阻**的是 **#11 Explainability 簇**：① **B4b conversations 死表（确认真 Bug）**——生产路径从未调用 `conversations::insert`（grep 0 命中 / callers 仅测试），07-31 幻觉因此无法回溯她原话；② **B4 Debug Panel 缺 5/9 分区**——Retrieved/Intent/Reflect 是"她为什么这么说"诊断链的核心，07-31 幻觉若有这些早定位了。本轮**已完成** **B4b（死表修复）→ B4-MVP（Retrieved+Intent+Reflect 三分区）→ B4-余 Cost（LlmClient 今日调用+token 计数）**——`cargo test --lib` **219 passed** / `cargo check --tests` ✅ / `tsc --noEmit` ✅ / `npm run build` ✅（详见 §最近一轮 续③）；AnimFSM(前端 fsm 上抛)/Prompt-动态 token 留 follow-up。**B1b 推迟**（07-31 A 档后无复发，条件触发）。**B6/B7 推迟**（在跑的架构债，重构风险高，非用户态）。**✅ 已实跑确认（2026-08-03 dev 真实 LLM）**：发一句话 → Ctrl+Shift+D（笔电 F12 被绑休眠，本轮加的备用键）看 Last Turn/Retrieved/Reflect/Cost 四分区全有值；conversations 0→2 行；**Cost 首次暴露单轮 3 次 LLM 调用（gate+extractor+main）**；Retrieved 的 sem≈0 暴露当前环境未加载 embedding（既有问题）。详见 §最近一轮 续③。release exe 需 `npx tauri build --no-bundle` 才生效。
> **2026-08-03（续②）更新 · 重大方向**：用户确认最终角色 = **璃 Liri（小狐灵）**，动画走 **Spine+PixiJS（不用 Live2D）**——当前 `Live2DCanvas.tsx` 是占位、将来迁移；FSM/emotionDriver/behavior→参数映射等技术无关层沿用。已落地：① 形象设计三文档拷入 [`docs/specs/liri/`](specs/liri/)（设计规范/动画设计/制作规范，原在桌面）② **人格配比落进 system prompt**——`system.txt` 身份+`[Core Personality]` 块改成 Liri（温柔35/好奇20/聪慧20/安静15/调皮5/神秘5 + 狐狸观察者本性 + NOT 话痨/卖萌/依赖/永远积极）；`firstrun.rs::seed_persona` core 维度同步改 Liri（中文 key，confidence=确信度非权重%；**仅对新装生效**，当前库仍是旧种子）。Liri 5 条行为原则大多已被现有 system.txt 规则覆盖（②不假记忆=rule8「严禁编造」）。验证：`cargo test --lib` **216 passed** ✅。**待 rebuild 进 dev/release 生效**（system.txt 是 `include_str!` 编译进二进制）。**待办**：① 当前用户库 core persona_traits 还是旧种子（gentle/patient/...），可选 reseed（`DELETE FROM persona_traits WHERE trait_type='core'` → 重启自动用 Liri 维度重种）；② 动画层 Spine 迁移是新方向（替换 Live2DCanvas，待 Liri.spine 资产）。详见 §最近一轮 (2026-08-03 续②) + 记忆 `liri-character-spine-direction`。
> **2026-08-03（续）更新**：完成 **B3 Sleeping 配套收尾**（纯前端，2 文件 ~10 行，原则 #1/#5/#6/#10/#11）。① **睡着抑制 nudge**——`App.tsx` DeepNight/LateNight nudge effect 加 `fsmRef.state===Sleeping` 守卫，睡着不再冒「早点睡」梦话（fsmRef 稳定无 stale-closure）② **接 sleep 音效**——`soundManager.ts` 加 `"sleep"` AssetKey + ASSET_PATH（`/audio/voice/sleep.mp3`，素材早已在）+ `sleep()` 方法（**mirroring `greet()`**：一次性状态进入 cue，**非**加权随机交互；mute 经 `ensureCtx` 尊重 #6），`App.tsx` 入睡 `forceState(Sleeping)` 后调 `sound.sleep()`（`fsm.state!==Sleeping` guard 保证每次入睡只响一次）③ **LateNight 不入睡只 yawn**——**已满足，零改动**：auto-sleep 本就 DeepNight-only（`App.tsx:239`），LateNight 经 sleepiness 权重调制多 yawn（Tier3 #7 早已做）。**验证**：`tsc --noEmit` ✅ / `npm run build` ✅（482 modules，2.16s）。**待实跑（已免改系统时间）**：为验收 A4/A5/B3 等需 DeepNight 的项，新增 **dev-only `window.__pet` 验收钩子**（`App.tsx`，`import.meta.env.DEV` 守卫，prod build grep `__pet`/`setHour` 0 命中✓）—— webview 内重写 `Date.prototype.getHours` 模拟时段（不改真实时钟、无 UAC）+ `forceIdle` 倒拨交互时间绕开 10min 入睡等待 + `sleep/wake/probeNudge` 直接触发。**操作清单见 [`docs/verify-checklist.md`](verify-checklist.md)**：`npm run tauri dev` → F12 → Console 用 `__pet.setHour(3)` 等。**当前无进行中任务**，下一会话按 **B4（P16 Debug Panel 补全）** → B5（Golden 评估框架）→ B8 推进；B1b（Grounding B 档运行时阻断）条件触发——实跑若仍偶发主动开口幻觉再升级。
> **2026-08-03 更新**：从 `D:\桌宠`（opencode 在本仓库副本上的工作）**合并** **B1 Consolidation 反向更新 Facts** + **B2 完整物理（自由落体/任务栏弹跳/1/3 飘落悬停）** + A4/A5 实跑方法论成果（CDP 自动化 + `Date.prototype.getHours` 重写模拟时段）。详见 §最近一轮 (2026-08-03)。两副本 base 完全一致（同 HEAD `50c45d2`，C/D 工作树在 grounding/reflection/Sleeping 等文件**逐字节相同**），故合并 = 纯增量复制 5 改文件 + 2 新文件（`gravity.ts`/`consolidation_harness.rs`），**零冲突**。验证：`cargo check --tests` ✅ / `cargo test --lib` **216 passed**（C 原 208 + B1 新增 8）/ `tsc --noEmit` ✅。清理了 harness 一处死变量（`ep_before`）。**当前无进行中任务**，下一会话按 B3（Sleeping 配套）→ B4（Debug Panel）→ B8 推进。
> **2026-07-31 18:01 更新**：三闭环 + 生命感主轴完成。**① 待验收代码层已全部闭环**——`cargo test --lib` 207 passed / `cargo check --tests` ✅ / `tsc` ✅ / `build` ✅，已 rebuild 进 18:01 release exe（含 A1/A2/A4 工作树 Rust 改动）。A1-A6 代码层 ✅、A7 勘误降级（未实现，单气泡覆盖）。**余下仅 GUI 运行时实跑**（A4/A5/A6 可立即验证；A1/A2/A3 需攒状态）——见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选) ①。**当前无进行中任务**，下一会话按 B1→B8 推进或先实跑 A4-A6。**主动开口幻觉已 A 档修复（19:10 rebuild，详见 §最近一轮）；残余：prompt 软约束无运行时阻断，B 档待命。**
**气泡 release rebuild 闭环（实跑确认 ✅ 2026-07-31）+ consolidation max_tokens 修复 + Reflection 触发器 Tier2 #5 + Sleeping 入睡机制（build 过 / 待实跑）。** 气泡：release exe 落后 dev 2 天，rebuild 后用户实跑确认居中。consolidation：生成任务 max_tokens 2048→4096（踩坑#3 复发）+ 空 content 防御。Tier2 #5：Reflection 事件驱动触发器（TurnThreshold 30 条对话记忆 / MajorEvent importance>0.85，1h 冷却，Daily→MajorEvent→TurnThreshold）。Sleeping：DeepNight(2-6) 无交互≥10min 自动入睡（forceState），交互（戳/摸/拖/对话/双击）markInteraction 唤醒 + 刷新 lastInteraction（天然 10min 清醒冷却）。后端 `cargo test --lib` 207 passed / 全 harness 编译 ✅；前端 `tsc`+`build` ✅。**下一步**：实跑 #4 converse thought / circadian 深夜 / 实跑 Sleeping（改系统时间 2-6 点+等 10min）/ 多气泡堆叠 / Tier2 #6。注：consolidation(≥100 episodes)/Reflection 触发器日常不易快速触发；Sleeping 需改系统时间到 DeepNight 验证。**全部已 rebuild 进 release exe（07-31 13:03），桌面快捷方式已含**；气泡已实跑确认，其余待择机实跑。

## §审计 (2026-08-03 续③)：深度审计 + 代码级核验

**任务**：用户要求审计计划（`implementation-plan.md` P0-P17/A1-A2）未完成部分、列优先级、按优先级继续开发。方法：**不轻信 HANDOFF 旧记录**，对照 codegraph + 源码逐项核验"声称未完成"是否属实、是否有遗漏。

**核验方法**：`codegraph_status`(103 文件/1442 节点) + `codegraph_explore` 看关键符号源码 + `codegraph_callers` 验调用方 + `Grep` 验生产路径 + Read plan P16/P17/A1-A2 验收标准。

**核验结论表**：

| 项 | HANDOFF 旧记 | 代码核验结果 | 证据 |
|---|---|---|---|
| **B4b conversations 死表** | backlog 普通项 | ❌ **确认真 Bug（#11 可追溯受损）** | `Grep conversations::(insert\|get_recent\|get_max_turn)` 于 `src-tauri/src` = **0 命中**；`codegraph_callers(insert)` 显示 `conversations::insert` 仅被测试 `test_insert_and_get_recent` 调用。plan P5.3 步骤 5 明确要求"原始对话日志写 conversations 表"。影响：无法回溯她原话（07-31 幻觉即因此无法定位）。 |
| **B4 Debug Panel** | "缺 5 分区" | ⚠️ **确认 6/9 分区** | `DebugPanel.tsx` = Brain/Counts/Facts/Episodes/Pending/Timeline。plan P16 还要 Prompt token / Retrieved score / Reflect / AnimFSM / Cost。后端 `DebugSnapshot`(commands.rs:689) 无对应字段。 |
| **B5 Golden 评估** | "框架不完整" | ❌ **确认无框架** | `tests/` 有 `golden_conversations.rs`（数据，42 符号）但**无 `evaluation.rs`**（plan P17 点名）。无 `personality_drift_score`、无 CI。Liri 人格刚落 system.txt → 缺回归网。**→ 已修复（2026-08-08 续）**：三层评估[规则/cosine/LLM-judge] + 30 golden 集全落地，见 §最近一轮 (2026-08-08 续)。 |
| **B6 A1 BrainState** | "架构债" | ⚠️ **确认债** | `converse()` = 10 参数（plan A1 要 `fn(brain:&BrainState)`），违反原则 #2 信号"参数>3"。在跑、重构触踩坑#4。 |
| **B7 A2 Scheduler** | "架构债" | ⚠️ **确认债** | `loop_runner.rs` = `std::thread::spawn`+`sleep`（medium 30s / slow 1h），非 plan A2 的 Scheduler trait。在跑。 |
| **B1b Grounding 阻断** | "条件触发" | ⏳ **确认条件成立、未触发** | `check_groundedness`(grounding.rs:235) 仅挂 converse、只 warn、`claim_patterns`(:256) 全英文（中文漏检）、未挂 proactive/welcome_back 输出端。07-31 A 档 prompt 收紧后**无复发报告** → 维持观察，不升级。 |
| **Liri/Spine 迁移** | 续②新方向 | 🟡 **确认受阻** | `Live2DCanvas.tsx` 是占位（Haru+f00-f05+emotionDriver），将来换 Spine+PixiJS。**受阻于 `Liri.spine` 资产未交付**。技术无关层（FSM/emotionDriver 逻辑/circadian/microBehavior）迁移时沿用——现无可做的代码。 |

**遗漏排查（HANDOFF 未单列但核验发现）**：
- **A7 多气泡堆叠**：旧 backlog 已正确降级（App.tsx 单气泡覆盖语义，非堆叠）✅。
- **③散落 follow-up**（Alt+Space 全局键 / ~~走路脚步声 loop~~（2026-08-08 随走路计划砍除）/ 害羞慢现 / rest_need 后端暴露 / speedModifier 接动画 / idle_weights JSON 化 / 选择性遗忘）均为小项，核验仍属未做，不升优先级。

**重排优先级（驱动：北极星 #10 + 阶梯 活着→记住→懂你→工具砍 + #8 成本 + #11 可观测 + "是否受阻"）**：

三闭环全完成 → 生命感主轴在维护态。真正的 #10 下一步（Liri/Spine 视觉角色）**受阻于资产**。故当前**未受阻的最高 ROI = #11 Explainability 簇**（B4b 死表 + B4 决策链分区）——它直接服务"她为什么这么说"的诊断，07-31 幻觉这类问题有它早定位了；且 B4b 是真 Bug。

| 优先级 | 项 | 理由 | 本轮 |
|---|---|---|---|
| **P1** | **B4b conversations 死表** | 真 Bug、小、外科手术式、解锁 #11 可追溯 | ✅ 本轮 |
| **P1** | **B4-MVP 决策链分区（Retrieved+Intent+Reflect）** | #11 核心、诊断幻觉/漂移、中等工作量、未受阻 | ✅ 本轮 |
| P2 | B4 余项（AnimFSM 前端 / Cost LLM 计数 / Prompt 动态 token） | #11 补全，但需前端 plumbing 或 LlmClient 插桩 | ⏳ follow-up |
| P2 | B5 Golden 评估框架 | 锁 Liri 人格防漂移；重（需真 LLM、≥30 对话、CI） | ✅ **完成（2026-08-08 续）** 三层[规则/cosine/judge] + 30 golden 集 |
| P3 | B1b Grounding B 档 | 条件触发（A 档后无复发） | ⏳ 观察 |
| P4 | B6 A1 BrainState / B7 A2 Scheduler | 在跑的架构债、重构风险高 | ⏳ 顺带改 |
| P5 | Liri/Spine 迁移 | #10 真正下一步，**受阻于资产** | ⏳ 等资产 |
| P5 | B8 二期 Shared World 等 | 二期愿景 | ⏳ 未来 |

**Scope 边界**：本轮只做 B4b + B4-MVP（三分区）。B4 余三项各有独立 plumbing 成本（AnimFSM 需前端 fsm 状态上抛、Cost 需 LlmClient 插桩、Prompt 动态 token 需记 last usage），单独立 follow-up 避免 scope 膨胀（原则 #9 刚够用）。

---

## §最近一轮 (2026-08-09 续¹⁰)：选择性遗忘 —— 多轮消歧义 + fact/pending 语义匹配

**任务**：08-05 续做的选择性遗忘（episode/fact/pending MVP）是**单轮、零状态、最高分赢家通吃**——gate→`Forget`→`forget_best_match` 扫三路各过 0.7 门、取置信度最高**直接删一个**，无候选则 converse 注入"不记得"。用户确认两个体验缺口都要解：① 多候选不澄清（猜删可能删错，违背 #1「Rust 绝不删错东西」）② 措辞不匹配太硬（char_overlap 字面不重叠，「忘掉早睡的事」匹配不到 fact「想早睡总是熬夜」）。实现深度定为**完整跨轮反问**：多候选→反问→slot 存候选→接第二轮→删指定。

### 关键约束（codegraph + 源码坐实，非假设）
1. **第二轮 gate 不进 Forget**：Forget 是动词驱动（"忘掉/删/取消"），"第一个/前者"会被分到 Silence → **接第二轮必须在 gate 之前拦截**，不能依赖路由。
2. **converse 对 AppState 是"瞎的"，但有跨轮注入范式**：`ConverseCtx.pacing: &Mutex<QuestionPacing>`（`converse.rs:69`）是现成的 turn-spanning slot → `pending_forget` 照抄，零架构新概念。
3. **踩坑#4 雷区**：改 `converse`/`ingest` 签名会断所有 harness。本轮**只加 enum 变体 / struct 字段，不改函数签名**；`ConverseCtx`/`AppState` 加字段则同步所有构造点（3 harness + lib.rs init + commands.rs send_message）。prompt_quality case 1009 种子下**双候选**（fact「想早睡总是熬夜」+ episode「熬夜写代码…早睡」非地标）→ 新逻辑从 ForgetAck 翻成反问 → 启发式判 FAIL，**必须同步**。

### 模块 A：多候选反问 + 跨轮消歧义（`forget.rs` 主体重写）
**新类型**（替 `ForgetResult`）：`ForgetOutcome::{Deleted{summary}, Declined, Ambiguous{candidates}}` + `PendingForget{query,candidates,created_at}` + `ForgetCandidate{target,id,summary,confidence}`（均 `#[derive(Debug,Clone)]`）。三态比 bool+Option 清晰：删了一个 / 诚实拒绝 / 需反问。

**`forget_best_match` 改纯决策**（`forget.rs:307`）：三路候选（含模块 B 语义匹配）→ `≥2` 返回 `Ambiguous`（**不删**，landmark 已被 episode 腿过滤，候选皆可删）→ `==1` 删它 `Deleted` → `0` `Declined`。纯决策**不碰 slot**（#1：Rust 决定删什么；slot 读写归 converse）。

**第二轮解析纯函数**（可单测，无 DB/模型）：`resolve_candidate` 先 `ordinal_index`（第N个/前者/后者/最后/1/A/甲乙，含 `cjk_to_digit`）→ 命中返回索引；否则各候选 char_overlap 取最高≥0.4；都不中 None。`is_off_topic`：无序数且全候选 char_overlap<0.2 → 判换话题（保守：疑似仍在话题内就留循环重问，只对明确新话题清 slot）。

### 模块 A 续：converse 控制流汇合（`converse.rs`，最复杂）
**入口（ingest 之前）拦第二轮**（`converse.rs:209`）—— `resolve_pending_forget`：take-and-clear **一次锁**（>90s stale drop，clone 候选出作用域后不持锁跨 DB 擦除）→ `resolve_candidate` 命中 → `execute_candidate` 删第 i 个 + 清 slot → `Resolved`；off-topic → 清 slot + `Proceed`（正常 ingest）；仍不明 → 清 slot + `Reask(candidates)`（**只重问一次**，slot 已清防循环）。

**跳过 ingest**（`converse.rs:210`）：Resolved/Reask 合成 `IngestionOutcome{route:Silence,…全 None}`——二轮"第一个"**绝不存为新记忆**（erase 已在 resolve 发生，ingest 只会污染）；但 emotion/retrieve/plan/chat 仍跑以产出回复。

**注入块**（`converse.rs:468`）：`Resolved` → "好我忘了"确认提示；`Reask` → `disambig_prompt`（列候选 summary 让 LLM 自然问"你说的是 A 还是 B？"，引真实摘要减少编造）；`Proceed` → 看 `outcome.forget`：Deleted/Declined 照旧，Ambiguous → **写 slot**（PendingForget）+ `disambig_prompt` 反问。三路径汇合既有 chat 生成。

### 模块 B：fact/pending 语义匹配兜底（`forget.rs`）
`find_fact_candidate`/`find_pending_candidate` 加 `embedding: Option<&EmbeddingService>`：char_overlap 粗筛 → 若 `emb.is_ready()` 调 `semantic_rerank`——**char_overlap top-5 现场 embed_batch + cosine**，`cosine_similarity` 未归一故 `((cos+1)/2).clamp(0,1)` 映射匹配 `retrieval::compute_semantic`，0.7 门读法不变；embedding 任意 hiccup 退回 char_overlap（#6 优雅退化）。效果：「忘掉早睡的事」语义命中「想早睡总是熬夜」。成本：每次 forget 最多 1 query + 5 value embed（forget 低频，可接受）。

### 模块 C：harness 同步（踩坑#4 全程未踩）
- `ForgetCandidate` 漏 `#[derive(Debug,Clone)]` → `ForgetOutcome`/`PendingForget` 的 Vec 成员要求它 → 5 处编译错全此一因，加 derive 即解。
- `IngestionOutcome.forget` 字段名不变、类型 `ForgetResult`→`ForgetOutcome`，ingest Forget 分支类型自动推断无需改。
- 3 harness（conversation_harness / memory_recall / prompt_quality）每个 ConverseCtx 构造点加 `pending_forget: &Mutex::new(None)`（memory_recall 有 3 处用 replace_all）。
- prompt_quality：`Expect::ForgetAsk` 启发式（回复含 哪/还是/具体/哪个/哪件/哪条/哪段 即 pass）已加但**当前无 case 触发**——case 1009 经验证为单候选（见下「修正」），保持 ForgetAck；case 1002/1005/1007（单候选/0候选/单候选）亦 ForgetAck。

### 验证
- **lib 293 passed**（forget 18 测含 6 新：`forget_best_match_ambiguous_keeps_both` 钉 ≥2 候选不删、`resolve_candidate_ordinals/keyword_overlap/unresolvable/out_of_range`、`is_off_topic_detects_new_subject`；语义路径 embedding=None 退回 char_overlap 由 `find_fact_candidate_below_gate_is_none` 等覆盖 #6）。
- **cargo check --tests ✅**（17 测试二进制全编译，含 3 harness ConverseCtx 同步）。
- **release `npx tauri build --no-bundle` exit0**（17:20，`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 24.4MB；先 `taskkill //IM desktop-pet.exe //F` 避坑#6 文件锁）。
- ✅ **prompt_quality G10 全 9 例 hard-check 0/9**（真模型，含修正见下）。

### 修正（commit 9bc3dac）—— 语义精排假阳性 + 1009 种子假象
1. **BGE-M3 假阳性根因**：首轮 harness 跑出 1002/1005/1007 误判 Ambiguous 引用无关 fact（早睡/实习）。根因——`semantic_rerank` 原本对**所有** fact（含 char_overlap=0）做语义精排，而 BGE-M3 无关文本基线 ~0.5 raw cosine → `((cos+1)/2)` 映射后 **0.75 > 0.7 遗忘门** → 无关 fact 被伪造成候选（"忘掉火锅"误命中"想早睡总是熬夜"）。**修复**：语义精排改为**只提升 char_overlap>0 的条目**（字面锚点过滤基线噪声），同时仍能捕捉近义（"忘掉早睡的事"→"想早睡总是熬夜"共享「早睡」）。lib 测用 embedding=None 不触发该路径故未暴露，harness 真模型才暴露——**遗忘这类「无 fallback 的语义门」改动必须真模型验**。
2. **1009 是单候选非双候选**：Fix B 后 1009「忘掉早睡」只命中早睡 **fact**（Deleted→"好，我忘了"）。规划时假设的 episode「熬夜写代码早睡」**未被 episode leg 命中**——`find_episode_candidate` 用 `retrieve(top_k=1)`，种子 offer 地标（importance 0.9）blend 排序第一 → should_forget 拒（地标）→ 早睡 episode 排第二根本没被看到 → episode leg 返回 None。**这是 episode leg 既有 top_k=1 局限（08-05 至今）+ 种子假象**（生产中"忘掉咖啡"无地标干扰 → episode leg top-1 直接相关 episode → 双候选正常）。期望回退 ForgetAck；多候选 Ambiguous 路径由 lib `forget_best_match_ambiguous_keeps_both` + D15 手动覆盖。`Expect::ForgetAsk` 启发式保留（识别合法反问行为，留待 D15 自动化或新增干净多候选 case）。

### 待实跑（D15）
dev 聊出两条同主题记忆（如偏好「猫」+ 一次「和糯米看猫」episode）→ 发"忘掉猫" → 见**反问**（"你说的是哪个？"）→ 答"那次经历"/"第二个" → 见"好，我把那段忘了"+ Debug Panel 确认 episode 删、fact 保留；再测序数"第一个"、换话题清 slot、90s 超时。

---

## §最近一轮 (2026-08-09 续⁹)：记忆卫生层 —— 结构性治三类易复发缺陷（写闸门 + 检索纯化 + 去重视野）

**任务**（用户原话）："1先观察，2治理。另外，不能只是完成这一次治理。你需要设计更好的结构来承担记忆任务，避免之后出现同样的或者类似的问题。设计完成之后需要自己复盘3次（合理否/会否引新问题/有无更优解）。先不要急着自己造，去其他地方看看有没有可以直接复用的框架。设计并复盘后自主执行并进行测试。"

### 调研（firecrawl，决定"不造什么"）
- **mem0**：`REJECT` 闸门 + ADD-only（无原地改）+ `supersede_by` 软废弃链；**V3 已砍 LLM-as-judge 二次校验**（V1/V2 的 extract→verdict 引发回归 + 成本，业界收敛到确定性规则闸门）。复用其负向规则 + 软废弃形态（我们 `expire` 机制已是）。
- **MemGPT / Letta**：blocks + 容量上限 + CAS（archive）+ **sleep-time 后台 worker**（把状态维护关进后台）。我们 consolidation + loop_runner slow_tick 已是这个形态。
- **Zep / Graphiti**：bi-temporal 知识图谱（节点+边带 valid_from/valid_to）。判 **overkill**（39 facts/单用户/成本#8 规模），且我们 `facts(valid_from/valid_to/source_episode)` 已是 bi-temporal 形状。

### 三类结构性缺陷（读码定位，非一次性脏数据）
| 缺陷 | 根因（代码） | 表现 |
|---|---|---|
| **A 抽取无校验** | `store_fact` 全盘信任 extractor 输出 + LLM 自打 confidence；extractor prompt 写对但 LLM 违规 10-20% | "太阳东升西落"conf0.98、"user is asking about my dreams"、知识问答入库 |
| **B 读路径强化** | `retrieve()` 每次**读**都副作用**写** `reinforce()`（+strength、+recall_count）；forget / proactive / **测试** 都触发 | recall_count 刷爆(382/445/446)、strength 饱和钉 1.0、富者愈富 |
| **C 去重视区** | `converse.rs:94` known_facts 只拉 `preference` 类 | 糯米跨 relationship/preference/profile 碎片化、extractor 看不到 → 重抽 |

> ⚠️ **复盘纠正**：原判 strength"只升不降"。**错**——`db::episodes::decay_strength`（×0.998/天）已在 `loop_runner.rs:309` 每日运行。B 的真正根因是"读路径也强化"，不是"无衰减"。

### 设计：两层确定性卫生（LLM 只提议，Rust 校验，原则 #1）
- **Part 1 写入闸门**（治 A，新 `mind/memory_gate.rs`）：`admits(fact)->bool` / `filter_facts`，无 LLM、可单测，`store()` 写库前调用。三条独立 deny：① category 白名单（preference/relationship/goal/profile/school/work/health，对齐 extractor.txt）；② 噪声 key（结尾 `_question`/`_gap`/`_knowledge` 或 `belief_in_*` 前缀——中文 trivia "太阳东升西落" 的 key 是 `knowledge_question`，靠此抓）；③ 噪声 value（英文 + 对齐 proactive `is_anchorable_fact`：asked about / asking about / user asked / user is asking / does not know / curious about / busy with work…）。
- **Part 2 检索纯化**（治 B，**零签名变更**）：`retrieve()` 删 reinforce 副作用 → 纯读；新增 `reinforce_top(db, episodes)` 辅助，仅 genuine-recall 调用方用（converse 非 QA / proactive 3 处）。**不新增衰减**（decay 已存在）。**为何不用 `reinforce:bool` flag**：retrieve 回归纯函数语义更清 + 签名零变更 → forget/tests/embedding_ab 调用点无需改（避坑#4）。
- **Part 3 去重视野**（治 C）：`converse.rs` known_facts `get_by_category("preference") take(20)` → `get_all_active(30)`（按 mention_count/confidence 排序）。

### 三次多视角复盘（设计定稿前，全文见 ADR）
1. **架构/正确性**：纠正 B"无衰减"为假 → 砍新衰减子系统；value 黑名单全英文漏中文反例 → key 黑名单兜住；旧 `test_strength_reinforcement` 会挂 → 改纯读契约 + 新 `reinforce_top` 单测。
2. **回归/副作用**：签名零变更确认；两个固定断言会失败（retrieval + gc_008）→ 已改；迁移误杀 `current_reading` → 改显式 expire（非 blanket 重放）；stale 注释（forget/embedding_ab）→ 已更新。
3. **小马尾/更优解**：砍 ~40% 代码（filter_facts 内联、`reinforce_top` 替 flag、衰减子系统全砍）；known_facts 全类保留。

### 不做（复盘收敛）
知识图谱（overkill）/ LLM judge 二次校验（翻车+成本）/ 新衰减子系统 & importance 地板（decay 已有效，无过衰减证据，地板治未病且可能保噪音）/ `enable_memory_gate` kill-switch（gate 与 `dedup_insert`/`expire_old` 同属零成本确定性 ingest 闸门，后者也无 toggle；#6 kill-switch 专给昂贵/LLM 能力省成本，gate 无成本可省；threading config 进 store() 是坑#4 级签名动荡）。

### 数据治理（一次性，用户 #2）
`scripts/migrate_memory_hygiene.py`（python sqlite3，镜像 memory_gate 模式 + 重置测试期饱和 strength，dry-run 默认 / `--apply` 提交，先备份 `.bak-hygiene`）。**执行结果**：expire **10 噪声 facts**（知识问答/自我语境/越界类，保留 current_reading + 糯米副本）+ **19** 非地标 episode strength snap 回 importance → facts 36→26 active、episodes 0 饱和（原 7）、排序现按 importance（小猪去世 0.8 居顶 / 素数 trivia 0.1 落底）。recall_count 不动（不参与评分，仅诊断）。

### 闭环2 测试途中修了续⁸ 既存 bug（非续⁹ 回归）
`cargo test --test closed_loop2_harness`（真实 LLM）首跑 **FAILED**：`proactive_bubble_brings_up_due_pending` 断言 pending 被触发，但 generate 走了 lively 分支（"伸了个懒腰…"）跳过到期 pending。**根因**：续⁸ 的 lively 70% 概率早返回（`proactive.rs:210` `gen_range(0..100)>=30`）在 `pending_due` 检查**之前** → 到期提醒被 70% 随机跳过。续⁸ 当时只跑 `check --tests`（编译）没跑 harness，漏掉。**不是续⁹ 回归**（测试用全新内存 DB，lively 早返回我未触碰，我的 reinforce_top 只在 non-lively 分支）。但它**破坏核心承诺**（北极星：到期提醒该被带出）。**一行守卫根因修复**：`is_lively = pending_due.is_empty() && rng.gen_range(0..100)>=30` —— 到期提醒在则强制走 memory 分支（确定性触发 mark_triggered），无到期提醒时 70/30 多样性原样保留（尊重续⁸"先观察"）。`generate_welcome_back` / `generate_lonely_bubble` 无 lively 概率分支，不受影响。**修复后闭环2 ✅ 1 passed**（anchor="明天有个大公司的实习面试" goal=care，pending anchored: true）。

### 验证（全绿）
`cargo test --lib` **287 passed** / `--test golden_conversations` **29 passed** / memory_gate 6 单测 / `--test closed_loop2_harness` **1 passed**（真实 LLM）/ 17 测试二进制全编译零签名破坏。commit `7f4af17`（卫生层）+ proactive 一行守卫（待提交）。

### 改动清单
新 `mind/memory_gate.rs`（admits/filter_facts + 6 单测）；`mind/mod.rs`（注册）；`mind/store.rs`（写库前过闸门）；`mind/retrieval.rs`（删 reinforce 块→纯读 + reinforce_top + 测试改纯读契约）；`mind/converse.rs`（known_facts 全类30 + 非 QA reinforce_top）；`pending/proactive.rs`（3 处 reinforce_top + **续⁸ lively 守卫**）；`mind/forget.rs`+`tests/embedding_ab_harness.rs`（stale 注释）；`tests/golden_conversations.rs`（gc_008→纯读契约）。ADR + 治理脚本。**release 待 `npx tauri build --no-bundle`**。

---

## §最近一轮 (2026-08-09 续⁸)：自主冒泡频率修复 + 灵性重构（记忆30/灵性70）

**起因**：用户体感——自主冒泡①频率太高（几分钟一次）②内容单一（全和糯米有关，要像真人突然找你聊天，话题任意，可自言自语/撒娇）。firecrawl 调研（companion app 主动对话：频控靠 cooldown、内容靠多类型+情绪+时段驱动，避免单记忆锚定重复）+ AskUserQuestion 定：频率=30min（修 bug + config 可调）/ 比例=记忆30:灵性70。

**频率根因（bug 非 design）**：`commands.rs:470` `let last_bubble = chrono::Utc::now() - chrono::Duration::minutes(31);` 硬编码——每次都满足 trigger_proactive Rule2（elapsed<1800 → 31min>30min 恒过）→ 前端 5min 轮询（App.tsx:407）每次拿 action → 高频。30min 设计（proactive.rs MIN_BUBBLE_INTERVAL_SECS=1800）本身对，是上游传参造假。

**内容根因（design 倾斜）**：`proactive.rs::generate`(168-259) ① 固定 query 每次召回同一批（糯米=强记忆）② 强制三选一 anchor（pending>fact>episode），无锚点 `Ok(None)` 沉默 ③ prompt "只能围绕它原意...绝不能换别的" → 永远糯米。

**改动（4 文件 surgical）**：
- `config.rs`：新 `ProactiveConfig{min_interval_secs:i64}` 默认 1800，`#[serde(default)]` 进 AppConfig（旧 config.toml 无 [proactive] 段用默认，无需改 AppData）。
- `commands.rs`：AppState 加 `last_proactive_bubble: Mutex<Option<DateTime<Utc>>>`；check_proactive 读真实值（None→now-36500days 哨兵，elapsed 巨大放行首次）传 trigger_proactive，**过门控即占位** `*t=Some(now)`（在 proactive_bubble 生成前；生成失败/None 也不让 5min 轮询区间内重复触发，conservative 宁少勿突兀）。
- `pending/proactive.rs`：① trigger_proactive 加 `min_interval_secs` 参数（删常量，Rule2 用参数；6 单测调用点同步+1800 踩坑#4）。② generate 入口 rand 加权：rng 收敛块内（ThreadRng 非 Send 不能跨 await）算 `(is_lively,query)` 后 drop；`>=30` 走新 generate_lively；memory 分支 query 从 `MEMORY_QUERIES`(5 条) 随机选 + 无锚点降级 lively（不再沉默）。③ 新 `generate_lively`(70%)：**不调 retrieve**（省 embedding）用 `RetrievalResult::default()`——空检索让 grounding_guard 自然禁任何用户过往编造（只能说自己的感受/环境/时间）；Intent goal=converse/tone=`lively_tone`(mood≥.7→playful/lonely>.6→gentle/else curious)；prompt=`lively_prompt(emotion,hour)` 纯函数——注入本地时段（format("%H")→早上/快中午/下午/傍晚/晚上/深夜）+情绪（想ta/不错/平静/闷闷），"此刻心里冒句话"（自言自语/撒娇/碎碎念，禁总结/套话/逼问），过 grounding_guard + record_interaction。④ 3 新纯函数测（lively_tone 三分支 / lively_prompt 六时段+防幻觉 / min_interval 可配证参数生效）。
- `lib.rs`：AppState 构造点初始化 `last_proactive_bubble: Mutex::new(None)`。

**两编译坑（已修）**：① ThreadRng(Rc-based) 非 Send 跨 await → tauri Future 不 Send → 收敛 rng 到独立块 drop。② chrono Timetrait::hour() 解析报错 → format("%H").parse() 不依赖 trait。

**验证**：cargo test --lib **280 passed**(277+3) / cargo check --tests ✅（generate 签名未变故 harness 无波及）/ release `npx tauri build --no-bundle` exit0（1m10s Rust+2.64s 前端，前端未改 CSS hash 不变）。

**待实跑**：观察 ① 冒泡间隔≈30min ② 内容多样性（**续⁸b 已用 harness 验证**，见下；Debug Panel Last Turn action=lively_bubble vs proactive_check 区分）。可调：AppData config 加 `[proactive] min_interval_secs=900` 改频率。

**续⁸b（2026-08-09，lively prompt 反同质化，commit 4a7516c）**：续⁸的 `bubble_content_check` harness 第一轮暴露 lively 气泡雷同——hour=11/loneliness=0.85 固定情境下 7 条全"快中午了+阳光/太阳+想你"变体。**根因**：`lively_prompt` 把 `time_desc`/`mood_desc` 当**成品词**直接拼进 prompt 句首（"现在是快中午了，你有点想 ta"），LLM 惰性照搬这两个词作输出骨架。**修复**（`proactive.rs` surgical +18/-15）：① `time_desc`/`mood_desc` → 描述性 `time_hint`/`mood_hint`（"中午时分"/"心里莫名有点空"，非可直接照搬的成品短语）；② 配 `time_avoid` 显式禁各时段套路报时词（快中午了/早上好/夕阳…）；③ 通用禁套路「忽然/突然+想你」「阳光正好/太阳正暖」；④ 强调具体小切入点菜单（动作/细节/身体感/荒唐念头/自言自语）+"不是打招呼、不是表达关心"破套话退路。新增 `tests/bubble_content_check.rs`（N=15 真实 LLM 内容回归资产，校验 70/30 比例 + 0 编造/套话/多重提问）。**第二轮验证**：11 lively/4 memory=**73:27**，0 编造/套话/多重提问全过，反套路词 0 命中，lively 11 条各异（数灰尘/影子/团成一团/后背咔哒/哈欠叹回/屏幕发烫/饭菜香/肚子咕噜），同质化根除。**残留**：伸懒腰动作重复 4 次（后续各异，可接受）；memory 仍 100% 糯米（**记忆数据集中度，非 prompt 问题**——库里糯米是唯一强记忆，等用户积累更多记忆自然分散）。

**续⁸c（2026-08-09，lively 允许轻好奇提问，commit 5075db2）**：用户反馈"不必全陈述句，也可以对我在做什么或其他事好奇并提问，像真人"。续⁸b 的 prompt 完全排除提问（"不是要 ta 回答" + 切入点全陈述），过严。改 `lively_prompt`：① "也不是要 ta 回答" → "也不一定要 ta 回答"；② 切入点菜单加"此刻有点好奇的小问题（ta 在忙什么/累不累/小疑问）"；③ 反套路补「在吗/在干嘛/有事吗」+ "想问只问一个、不必答、别追问别查岗"。守住单问号（multi_question 检测）+ grounding_guard 仍拦编造（lively 无 anchor）。**验证**：13 lively/2 memory，0 编造/套话/多重提问；提问自然出现（#15"你那边现在是晴是阴？"）。**本轮 lively 多样性较续⁸b 下降**（"打哈欠×4/晒光×6/犯困×3"聚集）= hour=11 固定情境放大收敛，真实多时段流变会分散；提问率 1/13 偏低（小样本方差）。**判断**：同质化是概率性非确定性 bug，继续加禁词是打地鼠（续⁷ Ali:Chat 教训：规则压不过模型倾向），不再追加禁词；真实使用观察后再定。

**memory 诊断（续⁸c 旁支，非改码）**：查 `%APPDATA%/DesktopPet/desktop_pet.db`——facts 39/episodes 21/pending 3。糯米 dominate 因：① 宠物是库里**唯一成簇**记忆（8 facts+5 episodes，confidence 0.80–0.98 最高档），向量检索 top-3 必被簇包揽；② 其他记忆（奶茶/星际穿越/实习/考试）语义稀疏单点斗不过多点簇；③ 测试期反复聊糯米，真实分布。**附带发现两问题**（待用户定夺是否治）：extractor 误抽（"太阳东升西落"等知识问答当 fact 存，conf 0.98~1.00；"dream_interest=user is asking about my dreams" 把桌宠被问语境误存用户 fact）；recall_count 被测试刷爆（桌宠 rc382/429、火锅445、素数446、work476，memory_strength 钉死1.00）污染检索加权。

## §最近一轮 (2026-08-08 续⁷)：速度（主回复关思考 ≤5s）+ 性格回归 + 记忆幻觉根因修复（6 轮 A/B，⏳ 未 rebuild）

**任务**：用户两条抱怨——① "消息回复速度还是有些慢" ② "现在的回复好像弱化了很多性格方面的部分…更像是通用回答，体现不出性格差异"。沿用续⁶ 的 150 条 A/B 闭环 + 分档提问率（G3/G11-G15<30%、G1/G2 直答 0%、G5 自然好奇）。**硬门（用户原话）**："等复测结果。单次回复在5S之外则做A"——A = speculative-parallel（gate+extractor `tokio::join` 并行 + 条件丢弃）。即：复测单次 ≤5s 则**不做 A**。

**调研·AIRI 速度**（firecrawl+GitHub）：AIRI 快因**本地 vLLM + 流式**（本地推理零网络 RTT、首字即出）。我们 API-bound（DeepSeek 远程），每轮 = 3 次**串行** LLM 调用（gate 分类 → extractor 记忆 → main 流式回复），无法复制 AIRI 本地优势；只能砍串行链里的思考开销。

**速度诊断 + 5s 门裁决**：续⁶ 已关 gate/extractor 思考（commit 8aa0d61）。剩余首字延迟杠杆 = **main reply 思考**。三档实验（150 条同 CASES 同 judge）：

| 配置 | main 思考 | FULL 时延（migration→emotion-react）| >5s | 质量 |
|---|---|---|---|---|
| round1(post) | enabled | 慢（reasoning 独占首字） | 多 | 续⁶ 基线 |
| retest4 | `reasoning_effort:low` | max **6s** | **破门** | 无增益（25%≈27%，方差内）|
| retest6（终态）| **disabled** | max **4s** / mean **2.7s** / P95 4s | **0/119** | 性靠 grounding 非推理 |

→ **OFF 满足 5s 门**（retest6：FULL max 4s mean 2.7s 0 超时；MAIN 流式 mean 0.9s）。**option A 不做**（用户"5s 外才做 A"，未触发）。

**性格回归（system.txt round-2）**：soul block 改**无条件**注入（续⁶ 的"空记忆时不连过去"条件化 → retest3 实测**反而更差**——经典反模式，"不要编造"反而 prime 该行为，已回退）；样例 6→**8 条**，memory-threading 加重（ex2"记得你之前念叨了好久"/ex3"你上次立的早睡flag"/ex8 唯一提问"肥瘦怎么样"）。恢复"上次说/我记得你"连结 tissue——这正是续⁶ 后用户感觉"弱化"要找回的。合同锁定串（温柔/好奇/.../璃/狐/严禁编造）全保留。human_like **4.07**。

**幻觉根因（grounding.rs，本轮核心发现）**：`format_memories` 空记忆时 `return String::new()` → **整段 [Memories] 不进 prompt** → 模型看到 soul block"thread memory"指令却无记忆段 → 从样例编造"你上次说…"（retest2 起空记忆组幻觉主因）。**修复**：空时返回显式内联标记 `[Memories]\n（暂无相关记忆——不要提及或编造任何过往，只就当下回应）`；非空时尾注"以上即全部记忆，不得添加未列项目、不得编造'上次说/提过/念叨'出处"（防 G6 越界）。**教训：内联信号 > 埋藏规则**（thinking-off 尤甚——模型不细读 system 规则，但读紧贴上下文的标记）。

**6 轮幻觉 arc**（验证根因 + 修复路径）：

| 轮 | 配置 | 总幻觉 | G6 | 备注 |
|---|---|---|---|---|
| round1(post) | ON + 旧 6 样例 | 9 | 4 | 续⁶ 收尾态 |
| retest2 | OFF + round-2 8 样例 | 12 | 4 | 关思考提速，方差升 |
| retest3 | OFF + soul 条件化 | 12 | 5 | "勿编造"反模式 → **回退** |
| retest4 | LOW effort | **19** | 4 | 最差 + 破 5s 门 → **回退** |
| retest5 | OFF + 空标记 | 10 | 7 | **空记忆修复**（fresh 组全 0）|
| retest6 | OFF + 空标记 + footer | 11 | 6 | footer 几乎无效（7→6，方差内）|

**方差洞察**：temp 0.8 + thinking-off → **~8pp run-to-run 方差 > 微调效果**。结论：**可靠性只能靠 grounding 层内联信号，不靠思考模式或 system 埋藏规则**。空记忆修复（结构层）是唯一稳健胜利；G6 越界压不动。

**诚实权衡**：
- ✅ **空记忆幻觉已修复**：retest5/6 所有 fresh-DB 组（G1/G2/G4/G5/G11/G13/G15）= 0 幻觉。
- ⚠️ **G6 越界残留 6/10**：真实记忆（奶茶/火锅/猫糯米/找实习）+ 编造"上次说/念叨"**出处**（topic 有据，出处虚构）。**根因 = 样例本身**（ex2/ex3 教"上次说"framing）= **用户要的性格**（soul block"常把现在和过去连起来，这是你最像你的地方"）。**G6 越界与所求性格是同一机制，不可完全分离**——这是 trade，非 bug。footer（内联规则）压不过样例（Ali:Chat "sample outranks rules"）。
- ✅ **提问结尾率 17%**（retest6）< 30% 达标；human 4.07 稳。
- 其余幻觉：G7 提醒 3（提醒语境偏客服化）、G3/G10 各 1。

**代码改动（lib 277 全过，client.rs 注释已修诚实）**：
- `converse.rs`：main reply `ThinkingConfig::disabled()`（关思考）+ `reasoning_effort: None`。
- `client.rs`：加 `reasoning_effort` 字段 + `chat_stream` 扩参 + `enabled()` 构造器——**LOW 回退后全部 dormant**（converse 传 None，`skip_serializing_if` 不上线）。注释已改"reserved plumbing / currently unused"，明天接手**勿误以为在用**。
- `system.txt`：round-2 无条件 soul block + 8 样例。
- `grounding.rs`：空记忆显式标记 + 非空 footer + `test_empty_memories_section` 断言更新（已过）。

**⏳ 未完成（明天接手，按序）**：
1. **rebuild release**（重操作，本轮未做）：`taskkill //IM desktop-pet.exe //F` → 等 ~3s → `npx tauri build --no-bundle` → 产物 `D:\cargo-target\desktop-pet\release\desktop-pet.exe`。
2. **向用户完整诚实报告**：速度已解决（≤5s，不做 A）/ 性格已回归（human 4.07）/ 幻觉根因定位并修（空记忆全 0）/ **披露 G6 trade**（越界=性格同源，不可全除）+ 方差（~8pp）。
3. **可选 follow-up（若用户在意 G6）**：① 软化 ex2/ex3 的"上次说"出处 framing（**会削弱性格，需用户权衡**——不建议默认做）② grounding B 档运行时阻断（backlog B1b：`check_groundedness` 补中文 claim 模式 + proactive/welcome_back 输出端检测丢弃）。

**报告快照**：`docs/review/prompt-quality-report-2026-08-08-retest{2..6}.md` + 同名 `-raw.log`（延迟来源）。retest6 = 当前默认 `prompt-quality-report-2026-08-08.md`（已备份 -retest6）。

## §最近一轮 (2026-08-08 续⁶)：真人感 prompt 调教 —— 150 条 A/B 闭环（提问结尾率 35%→14%）

**任务**：用户指出"回复不够真人感、找不出原因"，明确线索"回复中不需要每个问题都加上最后的提问"。要求：先调研（firecrawl/GitHub/humanizer/opencode）→ 汇报 → 列前后对比 → 用户批准后才改 → 同题复测对比。用户额外设**分档指标**（非全局阈值）：G3/G11-G15 提问结尾率 <30%（核心战场），G5 喜讯不设硬指标（自然好奇，换看开场去重），G1/G2 直答 0%，并强调"避免为了达标牺牲自然"。

**诊断（基线 150 条）**：全局提问结尾率 **35%**（52/150）。核心战场全超标——G12 分享 **80%**、G11 琐碎 60%、G3 闲聊 50%、G14 碎念 50%。G5 喜讯提问率 90%（不计）但"哇"克隆开场 5/10（502/503/505/507/508）。G1 知识 case 101 残留"想听细一点的我可以再讲"AI 客服尾巴。human_like 均值 4.24。

**调研收敛**：firecrawl/GitHub 陪伴+人机恋+真人感 + humanizer（去 AI 味 33 规则取 ~7 条 chat 相关）+ opencode 方案。跨源共识——① Ali:Chat 法：example 对话教人格优于 trait 描述（"sample outranks rules"）② humanizer 统计均值洞察：LLM 默认最高频/最通用的回复，真人感=偏离均值 ③ 反模式：每条收尾提问（客服感）、客服式尾巴（humanizer #20）、谄媚（#22）、过度共情=AI tell。

**改动 A/B/C（commit b5afac6，合同锁定串温柔/好奇/聪慧/安静/调皮/神秘·璃/狐·话痨/卖萌/依赖·严禁编造 全保留）**：
- **A `system.txt` 话术**：engage 条"再问一个问题"→"可不问，一句接得住的话就够"；"想问只问一个"加"更多时候陈述/感叹/观察让对话停住"；新增 4 条反 AI 味（禁客服式收尾"想听细一点的我可以再讲"/禁情绪标签"我理解你的感受"/允许自己的状态可困可懒可没话说/像随手发消息嗯哎半句话欲言又止）。
- **B `system.txt` 样例**：4→6 条混合，仅 1 条提问（喜讯 ~17%），删"想听细一点的我可以再讲"，破 G5"哇"开场克隆。
- **C `grounding.rs` format_intent engage**：`then ask ONE genuine follow-up` → `you may ask ONE… but often a single heartfelt line with no question is more natural. Never ask a generic '怎么样'`（提问可选且必须内容相关）。

**复测 150 条（同 CASES 同 judge 同模型）**：提问结尾率 **35%→14%**（−21pp）；G3 闲聊 50%→**10%**（达标）、G12 分享 80%→**30%**（−50pp）、G11 琐碎 60%→30%、G5 哇开场 5/10→**0/10**（开场变多样：满分！/赢了？/涨工资啦？）、"想听细一点的我可以再讲"消失；当提问出现时全内容相关（1207"汤头喝干净了没"/1208"有记下名字吗"，无套路"怎么样"）。G7 提醒 human_like 反升 3.7→3.9。

**诚实权衡（用户"别为达标牺牲自然"原则的实践）**：
- human_like 4.24→**4.11**（−0.13）：judge 备注一致"稍显简短"——回复**变短非变冷**（logical/on_topic 全 5.0），A3"可以一个字"被用足。仍稳 4+。
- 模板词 23→**23** 持平：但构成迁移"哇"(5→0)→"恭喜/棒/厉害"，而喜讯道恭喜是正常人类反应非 AI 味，真病灶（克隆开场）已除。
- G14 碎念残留 **40%**：4 个提问全对天然邀请追问的输入（在吗在吗在吗→什么事 / 啊啊啊啊→怎么了 / 想起来个事→什么事 / 猜猜我→猜测），压到 0 等于对"啊啊啊"回"嗯"，反人类——**保留正确**。

**交付**：对比报告 `docs/review/realism-report-2026-08-08.md`；评测快照 `prompt-quality-report-2026-08-08-baseline.md`/`-post.md`；harness `tests/prompt_quality_harness.rs`（150 例 + `CASE_FILTER` 环境变量按 id/组过滤）。**release exe 已 rebuild** 17:48（1m05s，0 警告）。→ 待用户实跑验收手感；可选微调见报告末尾（若嫌 G5 偏冷，软化 A3"一个字"为"别只剩一个字"）。

## §最近一轮 (2026-08-08 续⁵)：BrainState 扩到 prompt builder/budget —— 经评估关闭（ADR）

**任务**：用户"2，3 按顺序跑"——3 的第四子项（3d，架构债收尾）。Item 5（2026-08-08）把 `BrainState` 采纳边界定为 planner，`brain_state.rs` 注释留「prompt builder / budget allocator 取重叠子集，是干净 follow-up」。本轮复核该 follow-up 是否真的该做。

**复核（证据驱动）**：读五个目标函数实际签名 + 字段消费：

| 函数 | 签名 | 实际消费 |
|---|---|---|
| `build_system_prompt` | `(retrieval, emotion, intent)` | retrieval.{persona_traits,relationship,user_profile} + emotion + intent |
| `build_qa_system_prompt` | `(retrieval, emotion, intent)` | 同上（QA 版） |
| `allocate_and_compress` | `(retrieval, working_memory, emotion, intent)` | 上述 + working_memory |
| `allocate_qa` | `(retrieval, working_memory, emotion, intent)` | 同上（QA 版） |
| `compress_system_prompt` | `(prompt, retrieval, emotion, intent)` | 超预算时用**截断 retrieval** 重建 |

两个**致命不相容**：
1. **`intent` 是 planner 输出**（`planner::plan(&brain) → Intent`），不是规划输入——**不能入 BrainState**（brain 喂 planner、planner 产 intent，output 塞回 input = 循环依赖）。强行扩留个 `(brain, intent)` 两参，没比现状 `(retrieval, emotion, intent)` 三参更短，**省不掉 intent**。
2. BrainState 的 `text` / `relationship` / `pending_due` 这三个字段，五个函数**一个都不用** → 扩进去正是注释 + §A2 ADR 已否决的「投机 mega-state」。

**方案评估**：
- **方案 A（字面扩 BrainState）**：不相容（intent 循环）+ 捆绑 3 无用字段 + 踩坑#4 级（5 签名 + harness 调用点）+ 零价值。否决。
- **方案 B（窄类型 `PromptCtx{retrieval,emotion,intent}`）**：比 A 干净（不捆绑、不碰循环），但 `compress_system_prompt` 内部用截断 retrieval 重建（bundle 要先拆再组），且 3 字段都被每函数全用、本就是紧签名——为新类型 + 间接层消除 3 处重复，是「为单一用途加抽象」。边际，不采纳。
- **方案 C（保持现状）**：采纳。`(retrieval, emotion, intent)` 紧签名每参必用，自解释。

**决策**：follow-up 关闭，`BrainState` 采纳边界终态 = planner。ADR `docs/decisions/2026-08-08-brainstate-prompt-budget.md`（镜像 scheduler-deferred 先例：调研→结论不做→写 ADR→债标"已决策"）。同步更新 `brain_state.rs` 顶部注释从「干净 follow-up」改为「已调研并主动关闭，见 ADR」。

**向北星靠拢**：原则 #9（just-enough，不为对齐计划文本而加抽象）/ Karpathy 简单性（不为单一用途加抽象）/ #1+#2（intent 是输出非状态，不能强行入 BrainState）。**这是"不做"的正确决策**——与 B7 Scheduler 同款纪律：调研充分、证据确凿时，"决定不重构"也是债的合法收尾。

**验证**：纯决策，无代码行为变更（仅 ADR + 一处文档注释）。`cargo check --lib` ✅（注释改动）。**无需 rebuild**。**何时复议**：见 ADR 末（intent 下游消费者 ≥5 处重复且各自加字段 / planner 输入维度翻倍时，再考虑方案 B 窄类型）。

---

## §最近一轮 (2026-08-08 续⁴)：idle_weights JSON 化 —— 数据驱动微行为表

**任务**：用户"2，3 按顺序跑"——3 的第三子项（3c）。`microBehavior.ts::IDLE_BEHAVIORS` 8 条微行为（blink/look_around/tilt_head/yawn/stretch/sway/hum/peek）的 weight/cooldown_ms/emotion_modifier/min_closeness/sleepy 全硬编码在 const 数组里，**数据和逻辑混居**——调手感要在一个塞满 import/函数的 .ts 里翻找数值。

**完成**：纯数据抽离，行为字节不变。
- 新 `src/animation/idle-behaviors.json`：8 条行为表（纯数据，字段与原 const 一一对应）。
- `microBehavior.ts`：`import idleBehaviorsData from "./idle-behaviors.json"` + `export const IDLE_BEHAVIORS = idleBehaviorsData as IdleBehavior[]`。`IdleBehavior` interface / `applySleepyWeight` / `pickNextBehavior` **逻辑零改动**——它们本就消费 `IDLE_BEHAVIORS`，数据源从内联换成 JSON import 对它们透明。
- tsconfig `resolveJsonModule:true` 早开（circadian/sleepLogic 等 .ts 同款），无需改配置。

**向北星靠拢**：服务"#10 生命感"——微行为权重是手感微调的高频旋钮（哪个动作常出现/夜里多打哈欠），数据↔逻辑解耦后**调参只动 JSON**，降低手感迭代摩擦（原则 #9 刚够用：不引入运行时配置/后端下发——权重是调谐常数非用户配置，那是投机灵活性）。

**验证**：`vitest` **24 passed**（含 7 `microBehavior.test.ts`——A5 yawn 日夜比 >2×、look_around 夜里下降、sleepiness=0 白天不变、0.01 floor 钳位**全仍过**，证 JSON 数据与原 const 字节等价）/ `tsc --noEmit` ✅（`as IdleBehavior[]` 断言通过）/ `npm run build` ✅（Vite 打包 JSON import 正常，2.50s）。**纯前端，release 需 rebuild**。**无需手感验收**（行为不变重构，代码层单测已覆盖；类比 A1 BrainState 重构入"不易快速验收"类）。

---

## §最近一轮 (2026-08-08 续³)：害羞慢现气泡 —— 后端 closeness-aware mood 标签

**任务**：用户"2，3 按顺序跑"——3 的第二子项（3b）。设计 §6.3 情绪→气泡样式表里「害羞 = 慢慢浮现, 先半透明」是同级条目，但代码从未产出此标签、前端无对应样式。

**架构决策（关键）**：害羞的触发信号是什么？
- `derive_mood_label(&EmotionState)` 只看 mood/stress/social_battery——EmotionState **没有** attention/closeness 维度。设计 §6.6 把害羞系于「被注视（Focused attention）」、§6.2 系于「陌生拘谨（低 closeness）」，**两者都不是情绪向量属性**。
- 强行塞进 `label_for_mood_full` 会构造一条无信号支撑的任意规则（违反 Karpathy #1）。
- 后端**唯一可知**的害羞信号 = **closeness（亲密度，DB 里 0..100）**——而产出气泡 mood 标签的 `converse` 两处落库点已经读了 `relationship`（含 closeness）。
- 故：害羞 = **低 closeness（<20）时的中性/正向情绪**（§6.2 陌生拘谨）。closeness≥20 后自然解除（她放松了）。这是后端可知、设计支持、且**随关系进展真实变化**的信号。

**完成**：
- **后端** `emotion/state.rs`：新 `pub const SHY_CLOSENESS_THRESHOLD: f64 = 20.0`（镜像 lonely-nudge/planner-Rule4 的 `closeness>=20` 门，取反——同一个亲密度里程碑的两面）+ 新 `derive_mood_label_with_closeness(state, closeness)`：先 `label_for_mood_full` 算 base（单一真相源，零分叉），再 `closeness < 阈值 && !matches!(base, "担心"|"疲惫"|"难过")` → 覆盖「害羞」。distress 不掩盖（她和陌生人也会担心/累/难过）。**不改 `derive_mood_label` 签名**——5 调用点（commands×2 / converse×2 / grounding / retrieval）+ 测试零波及，纯加法。+2 单测（低 closeness 中性/正向→害羞、阈值边界、不掩盖 distress 三类）。
- **后端** `mind/converse.rs`：两处 emotion 落库点（silence:224 / normal:460）`derive_mood_label` → `derive_mood_label_with_closeness`。closeness 在 `planner::plan` 后算一次（`relationship.as_ref().map(|r|r.closeness).unwrap_or(0.0)`），两处共用。标签写进 DB → loop_runner 30s 重发的是这份持久化标签 → 害羞自然驻留到下次对话改写。set_emotion 调试命令保留原 fn（debug 手动覆写应字面反映所设情绪）。
- **前端** `App.tsx`：`bubbleClassForMood` 加 `害羞→bubble-shy`。
- **前端** `styles.css`：新 `.chat-bubble.bubble-shy` + `@keyframes bubble-shy-reveal`——1.2s ease-out（vs happy/playful 的 0.3s 弹出），0%→30% 停在 opacity 0.35（"先半透明"的迟疑）→100% opacity 1（可读）。慢揭幕 = 害羞试探。

**向北星靠拢**：服务"陪伴"核心——早期关系她**拘谨慢现**、亲密度上来后**自然放开**（气泡节奏从 1.2s 慢浮现 → 0.3s 爽快弹出），关系进展**肉眼可感**（原则 #10 生命感优先）。**架构 #1**（纯函数，标签 = 已有数据的投影）/ **#5**（情绪标签是 Mind 投影，closeness 是 Relationship 事实，不混入 EmotionState 向量）。

**验证**：`cargo test --lib emotion::state` 4 passed（+2 shy）/ `cargo check --tests` ✅（无踩坑#4）/ `cargo test --lib` **277 passed**（275+2）/ `tsc --noEmit` ✅。运行时手感验收属手动 → verify-checklist **D14**。**release 需 rebuild**（后端标签 + 前端样式都改）。

**遗留 / follow-up**：§6.6 的「被注视→害羞」**反应式**触发（attention===Focused 瞬间害羞）未做——那是前端 attention 驱动的瞬时态，与本次「关系阶段」驱动的稳态害羞是两个正交维度；当前稳态版已交付完整 §6.3 害羞气泡，反应式版留作增强。

---

## §最近一轮 (2026-08-08 续²)：Alt+Space 全局唤醒（P11.4）

**任务**：用户"2，3 按顺序跑"——3 的第一子项。设计文档 §6.4/§9.1「Alt+Space 快捷唤醒，直接开始说话」。

**完成**：真·系统级全局快捷键（任何 app 前台按 Alt+Space 即召出桌宠对话），非窗口内热键。
- **后端** `lib.rs`：新依赖 `tauri-plugin-global-shortcut` v2.3.2；`.plugin(Builder::new().with_handler(...))` —— handler 在 `ShortcutState::Pressed` 时 `get_webview_window("main").show()+set_focus()`（覆盖托盘隐藏态）+ `emit("show-input")`；setup 里 `app.global_shortcut().register(Shortcut::new(Some(Modifiers::ALT), Code::Space))`（失败仅 `log::warn!` 非致命——被别的 app 占了就降级，桌宠照常）。
- **前端** `App.tsx`：新 `show-input` listener（镜像 restore-from-tray 的自包含 useEffect + `cancelled` 防 StrictMode 泄漏）：`setAwayMode(false)` + `setInputVisible(true)` + `requestAnimationFrame` 后 `querySelector(".input-bubble input")?.focus()`（setInputVisible 异步，等一帧再 focus）。
- **权限**：后端 `.register()` 是 Rust 直调、不走 IPC，**无需 capabilities 权限**（IPC 权限只门前端 JS API）。

**⚠️ 权衡（设计钦定，已知）**：Alt+Space 是 Windows 窗口系统菜单键（键盘开 Move/Size/Minimize/Maximize/Close）。全局注册会**接管所有窗口的该键**——桌宠运行时键盘调窗口系统菜单失效。设计文档明确选此键；若用户嫌扰，setup 里改一行 `Shortcut` 即换键（如 `Ctrl+Space` / `Super+Space`）。

**向北星靠拢**：服务"无面板"原则（§3.4）+ 输入 UX（§6.4）——随时一句话唤起，降低对话门槛，陪伴更即时。**架构 #6**：失败非致命降级。

**验证**：`cargo check` ✅（新 plugin v2.3.2 编译过）/ `tsc --noEmit` ✅ / `cargo test --lib` 275 ✅。运行时按键验收属手动 → verify-checklist **D13**。**release 需 rebuild**（新依赖 + 前后端都改）。

---

## §最近一轮 (2026-08-08 续)：B5 三层人格评估 —— LLM-as-judge 第三线落地

**任务**：用户"2，3 按顺序跑"（2=B5 语义评估深化）。B5 规则层(续⑧) + 语义 cosine 层(Item6) 是廉价可 CI 两道线、各有盲区；本项补**重第三线 LLM-as-judge**——读人格圣经、给 persona_fit 0-10、命名漂移维度，是唯一能抓「客服腔 / 鸡汤 / 动作描写」等细微语气漂移的线。30 条永久标注 golden 集三层交叉验证。

**完成**：新 `tests/personality_judge_harness.rs`（永久评测资产，镜像 prompt_quality/embedding_ab 的"真 LLM 手动跑"模式）：
- **PERSONA_JUDGE_PROMPT**：璃人格定义（温柔/好奇/聪慧/安静/调皮/神秘 + NOT 话痨/卖萌/依赖/强行乐观 + 短/口语/直接/无动作描写/无服务式寒暄/陪伴非助手），输出 JSON `{persona_fit:0-10, drift:none|chatty|cloying|clingy|cold|mechanical|preachy|over_positive|action_desc, reason}`。
- **judge_persona**：`chat_reflection` temp 0.1 / max_tokens 2048（踩坑#3），JSON `{...}` 提取。**关键鲁棒性**：3 次指数退避重试（2s/4s）返回 `Result<_, String>`——30 连发 judge 撞 provider rate limit（实测无重试时 ~8 条连续 Err 被 `.ok()?` 静默零分、测试"假通过"），重试让失败可见可数。
- **30 条 golden 集**（三组各 10，永久回归资产）：On=璃典型语气；Gross=规则层必抓（chatty×3 >200CJK / cloying×3 emoji 堆 / clingy×4 marker）；Subtle=规则层必盲（cold / mechanical×2 客服腔 / preachy×2 说教 / over_positive×2 鸡汤 / action_desc 动作描写 / 套宠物）。每条标 expected_drift。
- **三层聚合 + 断言**：On/Gross/Subtle 各算 rule/cosine/judge 均值；断言 judge On>Gross & On>Subtle（judge 抓全部漂移）、cosine On>Subtle（语义层抓语气）、rule Gross 全 flag & Subtle 0 flag（规则层边界）、rule On>Gross。**judge 可靠性闸**：失败>3 即 fail，防零分假通过。
- **样本调试实跑发现**（首轮暴露，已修）：① 原 chatty 样本 ~150 CJK 未过 200 阈值→规则层漏（实测确认规则层盲区，延长到 >200）；② over_positive 样本用「！」被规则层当 cloying 抓→去感叹号使其成纯语气漂移（规则层真盲）；③ 模糊 On 样本"你最近在忙什么呀"被 judge 误判服务式寒暄、模糊 cold"哦"被 judge 误判高分→换清晰样本。调试本身即验证了三层各自真实边界。

**实跑信号（全 30 真实评分，0 失败，65s）**：

| 层 | On | Gross | Subtle | 覆盖边界 |
|---|---|---|---|---|
| 规则 | 1.000 | 0.660 (10/10 flag) | 1.000 (**0/10 盲**) | 只抓 GROSS 风格（长/emoji/marker） |
| cosine | 0.660 | 0.627 | 0.592 | 抓规则漏的语气漂移 |
| judge | **10.0** | 1.3 | 2.0 | 抓全部 + 命名维度 |

→ judge 是唯一抓「客服腔/鸡汤/动作描写/套宠物」的线；cosine 也区分出 0.66 vs 0.59 语气 gap 但 judge 给明确低分 + 维度名。

**向北星靠拢**：#11 可观测/可解释（"她像不像璃"三层可量化 + 漂移维度可命名）；锁 Liri 人格防未来 system.txt 改动回归。**架构 #1**：规则层纯函数 CI 跑（evaluation.rs 合成向量测 + 规则测），judge 是重手动线。

**验证**：`cargo check --tests` ✅ / `--test personality_judge_harness` 实跑 ✅。**纯后端测试资产，无生产代码变更，release 无需 rebuild。** → 待实跑见 verify-checklist D12。

---

## §最近一轮 (2026-08-08)：自主批次推进 —— 深度专注接线 / Scheduler 观测 / Grounding B 档 / BrainState / 语义漂移

**任务**：用户授权长程自主——"挨个推进 2,3,4,5,6 [审计清单里 5 个未实现/未接线项]，每完成一项自主验证、更新 HANDOFF + 新增待测试，不报告不询问；并砍掉走路相关计划"。逐项推进，每项自测（lib 单测 + check --tests + tsc）绿后落 HANDOFF + commit。详记见 §当前任务 清单，此处只叙事。

**完成**（5/5 + 走路砍除）：
- **Item 2 深度专注接线**：审计发现 `is_deep_focus` 全硬编码 `false` → 深度专注抑制空转。新 `perception/focus.rs`（纯 `update_continuous` + 30s 采样线程）+ 两生产点接真实值 + DebugPanel Focus 分区。lib 261 ✅ / check ✅ / tsc ✅。
- **Item 3 Scheduler 观测层**（兑现 08-07 deferral ADR 留的"可观测"开口，**不**引入被否决的 trait-Tick 多态）：新 `lifecycle/scheduler.rs` 进程级注册表（11 任务）+ `loop_runner` 全执行点接 `record` + config `[scheduler]` 4 enable flag + `get_scheduler_stats` 命令 + DebugPanel Scheduler 分区（11 行心跳图标）。新 ADR 取代旧 deferral。lib 267 ✅。
- **Item 4 Grounding B 档**：`check_groundedness` 加 10 中文 claim 模式（原 EN-only 中文零命中）+ 修隐藏 CJK 切片 panic（`ceil_char_boundary`）；新 `proactive::grounding_guard` 对**非流式主动气泡**首遍标记→重试→仍编造则抑制（None，不冒泡）。流式 chat 路径保持 warn-only（已流出的 token 无法撤回）。lib 270 ✅。
- **Item 5 全局 BrainState**：补 Task#9 的*内*层——新 `mind/brain_state.rs::BrainState<'a>`（5 借用字段，零 clone），`planner::plan` 5 散参 → `&BrainState`（body 别名桥接字节不变）。**采纳边界=planner**（旗舰纯决策）；强制单一 mega-state 反而捆绑不需要的字段（已否决的投机抽象）。踩坑#4 命中并修（断 golden 7 + questioning 3 共 10 调用点）。lib 270 ✅ 无警告。
- **Item 6 语义漂移层**：规则层只抓 GROSS 漂移（话痨/卖萌/依赖），对"简短无 emoji 却冷淡"盲视。补 `evaluation.rs` 纯 `cosine_similarity` + `semantic_drift_score`（`LIRI_PERSONA_REFERENCE` + `SEMANTIC_FLOOR=0.4` 映射）。架构 #1：模块只做数学、调用方喂向量 → 5 合成向量单测 CI 跑；真实 BGE-M3 由 `tests/evaluation.rs` Layer 3 端到端接。**实跑信号**：on-persona cosine **0.851** vs off-persona **0.781**（两句规则层都给 1.0、盲），语义层区分出 gap，断言通过。lib 275 ✅。
- **走路计划砍除**：见下"走路相关计划砍除"小节。

**向北星靠拢**：Item 2/3/4 服务 #11 可观测 + 防编造（深度专注不扰 + 后台心跳可见可关 + 主动气泡绝不冒幻觉）；Item 5/#6 服务 #2 统一快照 + #11 人格防漂移（规则层之上的语义回归网）。纯后端为主，release exe 批次末统一 rebuild。

---

### 走路相关计划 + 代码砍除（2026-08-08）

用户指示"砍掉和走路相关的计划"。核验发现走路**不只是计划**——`src/animation/spatial.ts` + `App.tsx` 里有正在运行的「走回窝」代码（离窝 ~15min 后 OS 窗口自动走回角落窝，带 walking CSS 动画）。**AskUserQuestion 确认后，代码一并砍**（默认推荐项）。砍除范围：
- **计划/设计文档**（`implementation-plan.md` + `specs/...design.md`）：BehaviorState 的 `Walk` 状态、FSM 可打断列表的 `Walking`、audioMap 的 `walk.wav`、**12.2「空间记忆—有窝」整节**（走回窝 locomotion）、任务栏表的"行走路径"、Physical Energy 的"走路消耗"、design FSM 图的 Walk 节点 + 音效表走路行——全部带「已砍除 2026-08-08」注释移除/标记。
- **运行代码**：**删 `src/animation/spatial.ts` 整文件**（`SpatialMemory` 类纯走回窝逻辑，无走路即死）；`App.tsx` 拆接线（import / `spatialRef` / 实例化 / `setNest` / 物理循环里的 `spatial.tick`+`isWalking`+`setPosition` 走回块 / `isWalking` state / `walking` className）+ 物理循环 deps 从 `[awayMode,isThinking,attention,inputVisible]` 收为 `[awayMode]`（三者仅服务于走回 interacting 判定）；`styles.css` 删 `.walking` 规则 + `walk-bob` keyframes + 改分区标题。
- **保留**：B2 物理自由落体/任务栏弹跳/拖拽（被交互非自主行走）、窝的 spawn 锚点语义（init 定位仍在，只是不再走回）。
- **③ 散落 follow-up「走路脚步声 loop」**：从 backlog 移除（§审计 ③ 散落项已划线）。

砍除理由统一：走路是工具性/探索性能力，违反"优先生命感不优先功能"（#10）且无陪伴语义；桌宠定位"桌面陪伴驻留"，拖到哪停哪。**验证**：`tsc --noEmit` ✅ / `vitest` 24 ✅ / `npm run build` ✅（3.81s）。release 需 rebuild。

---

## §最近一轮 (2026-08-07 续²)：自主批次推进 —— 鲁棒性 / BrainState / 记忆编辑 / loneliness 收尾 / 死代码 / 验收清单

**任务**：用户授权长程自主——"按优先级推进所有后续内容，每项自测后更新 HANDOFF，不询问；待实跑项统一整理"。逐项推进既定队列 #8-#14，每项自测（lib 单测 + check --tests + tsc）绿后落 HANDOFF。详记见 §当前任务 清单，此处只叙事。

**完成**（7/7）：
- **#8 鲁棒性**：converse 主回复空 content 重试一次（`&mut on_token` 复用，镜像 extractor）+ harness 启发式关键词表扩（治 705/1002 误报）。lib 259 ✅。
- **#9 B6 BrainState**：converse 9 参 → `ConverseCtx<'a>` 统一快照（on_token 留独立泛型），8 行别名桥接保 body 字节不变。6 调用点全改（commands + 3 harness）。check --tests ✅ + lib 259 ✅。
- **#10 B7 Scheduler**：**经评估主动搁置**（ADR `docs/decisions/2026-08-07-scheduler-deferred.md`）——计划 §A2 假设 Body-in-Rust，与原则 #5（Body 在前端）冲突；`start_life_loop` 已是定时器注册中心，引入 trait object 是投机抽象、高风险零价值。
- **#11 记忆可视化编辑**：Debug Panel 只读→可编辑。3 新命令 `forget_fact`/`delete_episode`/`set_emotion`（复用既有 DB accessor，pending 复用 `resolve_pending_event`）+ 前端 ✕ 按钮 + Emotion 5 滑块。check --lib ✅ + lib 259 ✅ + tsc ✅。
- **#12 loneliness 收尾**：① lonely-nudge 加 Sleeping 守卫（睡着不冒"想你"，#12①）。② `pet_head` 降孤独 -0.1（poke 不降）。tsc ✅ + check --lib ✅。
- **#13 死代码清理**（**修正前提**）：`trigger_proactive` **非死**（commands.rs:451 生产调用，前次判断过时）→ 保留；删 `emotion/homeostasis.rs` 整文件（零生产调用 + `TAU_STRESS` 与生产分叉会误导）+ GC_018；`tick_needs` 保留（正确委托纯函数、不误导，删它低价值中风险）。check --tests ✅ + lib 255 ✅。
- **#14 验收清单**：扩写 `docs/verify-checklist.md` 新增 D1-D7（记忆编辑/loneliness/Forget/QA/rest_need，全用新 Emotion 编辑器秒级触发）+ 不易快速验收表。Brain 行加 Lonely 显示。

**交付**：① release exe **已重建**（`npx tauri build --no-bundle` ✅，47s；`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 24.3MB，桌面快捷方式即更新）。② 用户照 `docs/verify-checklist.md` D1-D7 在 dev 模式手动验手感。

**向北星靠拢**：Debug Panel 记忆编辑让"记住你/懂你"可被人工纠偏（错了能改/删，非黑箱）；loneliness 闭环（想你→摸头缓解）+ Sleeping 守卫让"陪伴"更自洽。纯重构（#9/#13）与 ADR（#10）降技术债但不改行为。

---

## §最近一轮 (2026-08-07)：关系进展摘要 —— Hermes 后台 review 落地

**任务**：用户"读 handoff、用 codegraph 了解代码、继续开发"。三闭环全跑通、当前无进行中任务，AskUserQuestion 在 4 个可独立完成的方向里确认走 **关系进展摘要**（对应 Hermes 后台 review，服务"懂你"Soul 闭环深化）——未受阻、低风险、复用 reflection/consolidation 成熟模式；其余三项（激活 loneliness=行为变更需评估 / 记忆可视化编辑=开发者工具不直接服务陪伴 / 架构债 BrainState=重构在跑代码风险高）未选。

**设计**：每 N(=15)个新 conversation episode，后台用 reflection 模型回顾最近的记忆，产出 1-2 句"你们关系最近状态"总结（璃视角、free text），注入为 always-on 的 `[Relationship]` 区块——让她即使当前话题检索不到相关记忆，也带着对关系整体的理解。Hermes 精神：关系账独立、总是注入（而非靠检索运气）。

**实现**（3 新文件 + 6 改文件，全程不改 fn 签名，遵守踩坑#4 + #1 LLM 只表达）：
- **新表 `relationship_reviews`**（`migrations/003_relationship_reviews.sql` + `db/relationship_reviews.rs`，migration v3）：`insert` / `get_latest` / `latest_created_at` / `count`。独立于 episodes（关系总结 ≠ 单事件，不污染事件检索 / 不被遗忘 / 不消耗向量）。表留全历史（#11 可追溯），注入只取 latest 1 条。+2 单测。
- **新 `soul/review.rs`**（镜像 reflection.rs 风格）：`should_run_review(db)` 纯谓词（自上次 review 起 ≥15 个新 conversation episode；episode-gated 自然限频，非 conversation episode 如 consolidation 不计）；`run_review(db, llm)`（取最近 30 episode + 20 active facts + relationship 状态 + user_nickname → inline 中文 prompt → LLM `chat_reflection` temp 0.5 / 4096 max_tokens 坑#3 → free text → 空内容防御返 Err 重试 → 写表）；`maybe_run_review_if_due`（scheduler 入口）。+6 纯谓词单测（不足/足够/只计 conversation/上次后少/上次后足够/不计上次前）。
- **注入走现成管道**（零新机制，与 `relationship` 字段同款）：`RetrievalResult` 加 `relationship_review: Option<String>` → `retrieve()` 查 `get_latest` 填充（廉价 DB 读、无 embedding）→ 纯函数 `format_memories` 输出 `[Relationship]` 区块（在 `[Milestones]` 后、`[Memories]` 前，关系锚点优先）。`budget` 加 RELATIONSHIP=80 slot（system_prompt_budget + compress 同步 +RELATIONSHIP；qa budget 不加因 QA 不注入记忆）。`system.txt` 第 19 行加 `[Relationship]` 指引（LLM 别照搬复述、自然影响语气）。
- **调度**：`loop_runner::slow_tick` 在 reflection / consolidate 后挂 `maybe_run_review_if_due`（每小时检查、episode-gated 罕触发、失败 log 不致命 #6）。

**踩坑#4 变体（已修，harness 同步）**：`RetrievalResult` 加字段后，所有显式构造点同步——lib 内（retrieval 1 / budget 4 / grounding 2 / planner 2）+ harness（golden 7 / evaluation 1 / questioning 1）全补 `relationship_review: None` 或 `clone()`。`converse.rs` 用 `RetrievalResult::default()`（Default 衍生，新字段自动 None）无需改。`check --tests` 一次定位全部遗漏点，逐一补齐后通过。

**架构契合**：#1（LLM 只写 free text 总结；Rust 决定触发/存储/注入）/ #3（prompt 明令只基于真实记忆、不编造）/ #8（reflection 模型 + episode-gated 罕触发，每 ~15 轮对话 1 次额外调用）/ #9（MVP top-1 latest 注入 + 15 episode 阈值，刚够用）/ #11（relationship_reviews 表 + format_memories `[Relationship]` 区块 + log + 9 新单测可追溯）/ 不改 fn 签名（struct 字段 + 新模块 + 内联分支，规避踩坑#4）。

**验证（全绿）**：`cargo test --lib` **257 passed**（248 + 9 新：6 review 谓词 + 2 relationship_reviews db + 1 `[Relationship]` 注入）/ `cargo check --tests` ✅（全 harness 编译）/ `cargo test --test evaluation` **6 passed**（Liri 人格契约回归网全过，system.txt 加 `[Relationship]` 未破 6 维度/狐灵/NOT-list/严禁编造契约）/ `cargo test --test golden_conversations` **30 passed**（集成场景无回归）。**合计 294 确定性测试全绿**。

**待实跑**：`npm run tauri dev` 攒 ≥15 条记忆后（slow_tick 每小时检查）→ `relationship_reviews` 表出第一行 + 对话里她语气带关系理解（如用户说"最近真累"她不只回"累"相关，还带"我们聊了这么久"的关系感）。CDP 不易触发（需攒 episode + 等 slow_tick），主要靠 dev 自然积累。**release exe 需 `npx tauri build --no-bundle`**（system.txt include_str! + 后端 + migration v3 需重编；旧 DB 启动自动 migrate v3 建表）。

**Scope 边界 / follow-up**：① 阈值 15 是安全起点，实跑后按 review 频率/质量调（太少则陈旧、太多则频繁 LLM 调用）。② review 产 free text 无结构化（不像 reflection 产 JSON traits/thoughts）——关系状态用自然语言够用；若未来要结构化（亲密度趋势 / 里程碑标签）再加。③ 只注入 latest 1 条（历史 review 留表不注入）；若要"关系演变轨迹"注入多需改 format_memories。④ review 不进 episodes / 不向量化（注入是 always-on latest，非检索驱动）——省 embedding 成本，但旧 review 不被语义检索（只服务"当前关系背景"）。⑤ `run_review` 无 LLM 端到端单测（需真模型、慢；靠 `should_run_review` 6 纯谓词测 + 实跑覆盖，镜像 reflection/consolidation 不测 LLM 端到端的惯例）。⑥ 同一 slow_tick 若 reflection + consolidation + review 三者同时 due，是 2-3 次 LLM 调用（每小时上限，且 review 15 episode 才触发一次，可接受 #8）。

**当前无进行中任务**。下一会话起点：① runtime 实跑本轮（dev 攒记忆看 review 生成）② 或回前面留的待实跑（选择性遗忘 fact/pending / QA 直答 / #10 rest_need/speedModifier / B4 AnimFSM/Prompt 分区 / sleep 音）③ 或 B6/B7 架构债（如接受重构风险）④ Liri/Spine（等资产）⑤ Hermes 余项（记忆可视化编辑）。

---

## §最近一轮 (2026-08-07 续)：激活 loneliness —— 璃会"想你"

**任务**：用户"读 handoff、用 codegraph 了解代码、继续开发"。三闭环全跑通、当前无进行中任务。AskUserQuestion 在 4 个未受阻方向（激活 loneliness / 记忆可视化编辑 / B6-B7 架构债 / 鲁棒性加固）里确认走 **激活 loneliness**——服务"陪伴"北极星、未受阻、低风险、镜像 08-04 修 rest_need 的成熟模式。其余未选：记忆可视化是开发者工具不直接服务陪伴；B6/B7 是在跑代码的推测性重构（违反"不重构没坏的东西"）；鲁棒性加固价值较低。

**核验（codegraph + grep）发现关键死字段**：`emotion::tick_needs`（needs.rs）让 loneliness 增长，但 `codegraph_callers` 证它**仅测试调用、生产零调用**（与 08-04 审计发现 rest_need 同病）。生产 homeostasis 走 `db::emotion::apply_homeostasis_time_aware`，08-04 已接 `tick_rest_need`，但 **loneliness 从不更新** → 冻结在种子值。后果：`planner` Rule 4（loneliness>0.6 + closeness≥20 → goal=accompany / proactive）**永远到不了**——loneliness 只能被对话里 `react.rs` 的 delta −0.08/轮往下压，永远爬不到 0.6。即"她想你"这条设计好的规则是死的。另一处 `pending::proactive::trigger_proactive`（Rule 5 loneliness→random_chat）`codegraph_callers` 证同样是**死函数**（6 callers 全测试）——活路径是 generate / generate_welcome_back。

**设计**：两段。① 核心激活（镜像 rest_need）让 loneliness 真增长、planner Rule 4 复活（你回来后她回复带 accompany 暖意）；② 主动气泡（镜像 welcome-back / proactive-prompt emit 模式）让她在你 idle 时**主动**戳你——即选项描述里承诺的"主动找你"。

**实现**（6 改后端文件 + 1 改前端，全程不改 fn 签名，遵守踩坑#4 + #1 Rust 决策）：
- **`emotion/needs.rs`**：新 `pub fn tick_loneliness(loneliness, elapsed)` 纯增长规则 `(l + elapsed*LONELINESS_RATE).min(1.0)`（仅增长项——交互下降由 converse 里 react delta 处理，homeostasis 只建模 idle 增长）；`tick_needs` 非交互分支改调它（DRY，既有 test_loneliness_growth / test_interaction_reduces 仍绿）。+1 纯函数测（增长/累积/clamp）。
- **`emotion/mod.rs`**：re-export `tick_loneliness`。
- **`db/emotion.rs::apply_homeostasis_time_aware`**：加 `new_loneliness = tick_loneliness(current.loneliness, elapsed)` + SQL UPDATE 加 `loneliness = ?7`、`last_homeostasis_at/updated_at` 重编号 `?8`、params 加 `new_loneliness`。+1 测（1h idle → +0.36，证生产路径真增长）。
- **`pending/proactive.rs::generate_lonely_bubble`**（镜像 generate_welcome_back）：load emotion → retrieve 取可选锚（fact/episode，无锚也说话）→ Intent{goal=accompany, action=lonely_nudge, tone 按 mood, proactive=true} → allocate_and_compress → push 1 句 prompt（「你一个人待了一会儿有点想 ta…轻轻戳一下，不是催回复，别黏人别问问题逼答，规则8 严禁编造」）→ LLM chat temp0.8/4096（坑#3）→ record_interaction("lonely_nudge") → reply。
- **`emotion/react.rs::lonely_canned(mood)`**：mood 分档 canned 降级（高"嘿~你还在呀，真好"/低"……你也在呢吧"/中"突然想跟你说说话~"），#8 graceful。
- **`commands.rs::lonely_bubble`** + **`lib.rs`** 注册：薄命令，LLM 路径优先、空则降级 lonely_canned（镜像 welcome_back_bubble）。
- **`lifecycle/loop_runner.rs::check_lonely_nudge`**（镜像 check_presence_transition）：medium_tick 后调；门控 loneliness>0.6（LONELY_NUDGE_THRESHOLD）+ closeness≥20（LONELY_NUDGE_CLOSENESS，镜像 planner Rule 4 早期不主动）+ presence Active（不戳空桌）+ recent_interaction>120s（非对话中）+ 30min 线程本地 cooldown（LONELY_NUDGE_COOLDOWN_SECS，稀有惊喜非 spam）→ emit "lonely-nudge"。thread-local `last_lonely_nudge` 加进 medium 线程闭包（镜像 away_since）。
- **`App.tsx`**：listener `listen("lonely-nudge")` → invoke lonely_bubble → showBubble(reply, 10000, moodClass)。onboarding/away 守卫同 welcome-back。

**架构契合**：#1（Rust 决定何时触发/门控/存储；LLM 只表达 free text 气泡）/ #6（closeness 门 + presence 门 + cooldown + canned 降级，每环失败不致命）/ #8（homeostasis 零 LLM；lonely 气泡 cooldown-gated 30min 罕触发，可接受）/ #9（MVP top-1 气泡 + 0.6/20 阈值 + 30min cooldown，刚够用）/ #10（"她想你"= 生命感核心）/ #12（canned 含静默向"……你也在呢吧"，沉默也是表达）/ 不改签名（新 fn + 新 action 字符串 + SQL 参数，规避踩坑#4）。

**验证（全绿）**：`cargo test --lib` **259 passed**（257 + 2 新：tick_loneliness 纯函数 + homeostasis_grows_loneliness 生产路径）/ `cargo check --tests` ✅（全 harness 编译，无签名变更未破）/ `tsc --noEmit` exit 0 / `npm run build` ✅（2.45s）/ `npx vitest run` 24 passed。**release exe 重建中**（npx tauri build --no-bundle，前端+后端都改）。

**待实跑**：`npm run tauri dev` → ① 攒 closeness≥20（多聊几次）② 离开/不说话 ~1.7h（loneliness 增长到 0.6）且 presence Active → 看她主动冒"想你了"类气泡（30min 一次，回复后 loneliness 降停止）；或 ③ 离开后回来发消息，她回复带 accompany 暖意（planner Rule 4，Debug Panel 看 intent.goal）。CDP 不易触发（需攒 closeness + 等 loneliness 增长），主要靠 dev 自然积累。

**Scope 边界 / follow-up**：① **pet/nudge 不降 loneliness**——loneliness 仅由对话 react(−0.08) 降、homeostasis 增；戳/摸更新 last_interaction_at（suppress nudge 2min）但不降 loneliness。次要边缘（戳了不说话 loneliness 仍涨），MVP 接受；若要"互动也解闷"需把降 loneliness 接进 markInteraction，超本任务。② **lonely 气泡走 LLM**（每 30min 一次，cooldown-gated，成本可控）——若要零成本可全走 canned，但重复机械违反生命感。③ **trigger_proactive 仍是死函数**（6 测试 caller），非本轮引入，surgical 不清。④ **lonely-nudge 无 Sleeping 守卫**——靠后端 presence 门（DeepNight 用户通常不在 → 不 emit）；若深夜在桌且璃睡着，理论上会冒"想你"气泡（视觉不一致，罕见边缘，follow-up 可加 fsmRef Sleeping 守卫）。⑤ 阈值 0.6/20/30min 是安全起点，实跑后按频率/质量调。

**当前无进行中任务**。下一会话起点：① runtime 实跑本轮（dev 攒 closeness + 等 loneliness）② 或回前面留的待实跑（关系进展摘要 review / 选择性遗忘 fact-pending / QA 直答 / #10 rest_need-speedModifier / B4 AnimFSM-Prompt / sleep 音）③ 或 B6/B7 架构债 ④ Liri/Spine（等资产）。

---

## §最近一轮 (2026-08-05)：选择性遗忘扩展至 fact/pending + FTS5 可行性证伪

**任务**：用户"读 handoff、用 codegraph 了解代码、按优先级继续开发"。HANDOFF §下一步总清单 把 **FTS5 全历史检索** 标为"最高 ROI follow-up"（续③ Debug Panel 检索退回关键词兜底的诊断遗留）。

**FTS5 可行性证伪（决定性，写入避免重复踩）**：写 throwaway probe（`tests/fts_probe.rs`，已删）测 bundled SQLite（`rusqlite` `bundled` feature）的 3 个 FTS5 分词器对中文 2 字查询 '火锅' 的 MATCH——**FTS5 可用（建表 OK）但 trigram/unicode61/ascii 三者 match count 全 0**。根因：标准 SQLite FTS5 无 CJK 分词——trigram 需 ≥3 字查询；unicode61 把 CJK 连续段当单 token；ascii 只认 ASCII。HANDOFF 旧记"sqlite-vec 自带 fts5_cjk"**错误**（fts5_cjk 非标准 tokenizer，sqlite-vec 也不捆绑 FTS5 分词器）。→ **FTS5 对本库主语言（中文）不可行**，除非引入重依赖（jieba 可加载扩展 / Rust 端分词喂 FTS5），远超"干净 follow-up"范畴。**结论：FTS5 从 backlog 移除/降级，勿再尝试**。probe 省了一轮建错。

**转向**（FTS5 既不可行，取下一未受阻、低风险、服务"记住你"核心的项）：**选择性遗忘扩展至 fact + pending**——08-04 续 episode MVP 明确 deferred（`"fact（偏好）+ pending（提醒）遗忘未做"`），结构镜像 episode 路径，有现成模式可抄。

**实现**（6 文件，全程不改既有 fn 签名，遵守踩坑#4 + #1 Rust 决策）：
- **`db/facts.rs::expire_by_id(conn,id,now)->Result<bool>`**：精确软删单条 fact（`UPDATE ... SET valid_to=? WHERE id=? AND valid_to IS NULL`）。区别于 `expire_old`（按 category+key 批量过期，用于矛盾事实到达）——expire_by_id 只过期用户指明的那条，保留行供 `dedup_insert` revive + 审计轨迹（forgotten 偏好 = 停止浮现，用户再说起会自然复活）。+1 单测（精确性：同 category+key 两条 value，只过期指定 id；已过期/缺失返 false）。
- **`db/pending.rs::get_all_pending(conn)->Vec<PendingEvent>`**：返回所有 `status='pending'` 事件（triggered/resolved 不参与匹配——已完成的提醒不该再被"忘"）。forget 动作复用既有 `mark_resolved`（状态机终态，无硬删，pending 模型本就用 status 生命周期）。+1 单测（排除 resolved/triggered）。
- **`mind/retrieval.rs`**：`keyword_similarity` 改 `pub(crate)`（forget 复用成熟 CJK 匹配器，DRY）+ 新 `pub(crate) char_overlap(a,b)` = **字符 bigram 重叠系数** `|A∩B|/min(|A|,|B|)`（非 Jaccard 的 union 归一）。关键：短记忆被请求词包围时 Jaccard 被稀释（"忘掉咖啡" vs "咖啡" Jaccard 0.33），重叠系数按较小集归一 → 1.0。+1 单测。
- **`mind/forget.rs`**（核心）：新 `ForgetTarget{Episode,Fact,Pending}` 枚举 + `ForgetCandidate{target,id,summary,confidence}` + 三个 finder（`find_episode_candidate` 复用 retrieve + should_forget 门 + landmark 保护；`find_fact_candidate` char_overlap(text,value) 扫活跃 fact；`find_pending_candidate` char_overlap(text,title) 扫 pending）+ `execute_candidate`（Episode 硬删+向量清 / Fact 软过期 / Pending resolve）+ **`forget_best_match(text,db,embedding)`** 调度器：三路扫描、各自信任门（0.7）、取最高置信度执行一条。用户不说忘哪种记忆（"忘掉咖啡"可能是偏好/提醒/事件）→ 扫三种、挑最佳。置信度度量按类型不同（episode=embedding 语义 / fact·pending=char_overlap）但都 0..1、0.7 门读作"≥70% 确定"。两类型都达标时高分赢——fact（软过期可恢复）自然倾向压过 episode（硬删），歧义时取更安全动作。**保留 `forget_episode`**（窄 API，episode-only；其 execute_forget 4 测仍钉 landmark/置信度门行为，未迁移以遵守 surgical）。+4 新测（fact 过期 / pending resolve / 无匹配诚实拒绝 / 低置信不候选）。
- **`mind/mod.rs`**：re-export `forget_best_match`；Forget 分支 `forget_episode` → `forget_best_match`。
- **`resources/prompts/gate.txt`**：forget 类别例子扩偏好/提醒（"忘掉我爱喝咖啡"/"忘掉那个提醒"/"取消那个闹钟"），帮 gate LLM 路由非事件类遗忘请求。

**架构契合**：#1（forget 纯 Rust 决策删谁 + 置信度门 + landmark 保护；LLM 只分类意图 gate + 确认 converse）/ #9（MVP top-1 + 0.7 门 + 软动作为默认）/ #11（char_overlap 注释 + forget log 含 target 类型 + 8 新测可追溯）/ 不改签名（枚举 + 新 fn + 内联，规避踩坑#4）。

**验证（全绿）**：`cargo test --lib` **247 passed**（240 + 7 新：char_overlap / expire_by_id 精确 / get_all_pending 排除非 pending / forget_best_match fact·pending·no-match·低置信）/ `cargo check --tests` ✅（全 harness 编译，forget_best_match 新 fn + ForgetResult 未改未破）。**待实跑**：dev "忘掉X"（X=偏好/提醒）→ 确认她回"好，我忘了"且后续不召回（Debug Panel 看 fact valid_to / pending status）。**release exe 需 `npx tauri build --no-bundle`**（纯后端 + gate.txt include_str!，需重编）。

**Scope 边界 / follow-up**：① fact/pending 匹配用 char_overlap（关键词级，无 embedding）——短值/简练标题匹配好（1.0），冗长标题（"明天的面试"）+ 简练请求（"忘掉面试"）会低于 0.7 诚实拒绝（可接受，用户可换措辞 "忘掉明天的面试"；语义级匹配需给 fact/pending 也加向量，重，未做）。② 仍只忘 top-1 最佳匹配（多匹配需重复请求）。③ forget_best_match 的 episode 路径经 retrieve 有 +0.03 强化副作用——若 fact/pending 赢，某 episode 被无害强化一次（已注释说明，#9 MVP 接受）。④ FTS5 已证伪移除——若未来要零成本全历史检索，需先解决 CJK 分词（jieba 扩展或 Rust 分词）。

---

**实跑发现的 bug 修复（2026-08-05 续）· 同 key fact 冲突未即时淘汰（记忆准确性）**：dev 实跑发现"我说喜欢咖啡，重启后问'我喜欢什么饮品'，她答奶茶"。诊断（写只读探针读真实 DB，已删）：`preference/beverage_preference` 同时有 `"likes milk tea"`(mentions=7) 和 `"likes coffee"`(mentions=1) 两条 **active**——新咖啡没顶掉旧奶茶。根因：`store_fact`（ingest 存 fact 路径）只调 `dedup_insert`（只处理同 value 去重/复活），**没调 `expire_old`**（同 key 不同值的冲突淘汰）；而 `correction`/`consolidation` 路径都调了 → ingest 路径与系统其余部分不一致。`store_fact` 的 doc 还谎称"update or expire old"但 expire 半没实现。旧奶茶 mentions 高、检索排前 → 胜出 → 答奶茶。原本要等后台 consolidation（≥100 episodes）才清理，用户卡在窗口期。system.txt grep 无"奶茶"→排除 prompt 抄袭，纯记忆冲突。**修复**：`store_fact` 在 `dedup_insert` 前加 `expire_old(category,key,now)`——同 key 新值立即顶掉旧值（单值槽，与 correction/consolidation 对齐）；同 value 走 expire+revive 保留 mention 累加（dedup_insert case 2，既有 `test_store_fact_dedup` 仍绿）。+1 回归测试（新偏好顶旧偏好，旧值 expired 不删、留审计）。**lib 248 passed** / `check --tests` ✅。**注**：forward-only——用户现有 DB 里奶茶+咖啡仍共存，重启后再说一次"我喜欢咖啡"即触发清理。设计取舍：(category,key) 视为单值槽，新声明替换旧的——与既有 expire_old 设计一致；若未来要"多值"（如同时喜欢咖啡和奶茶），需改数据模型，当前 single-valued 够用。

---

**当前无进行中任务**。下一会话起点：① runtime 实跑本轮（dev 忘偏好/提醒）② 或回前面留的待实跑（#10 rest_need/speedModifier、QA 直答、B4 AnimFSM/Prompt 分区、sleep 音）③ 或 B6/B7 架构债（如接受重构风险）④ Liri/Spine（等资产）。

---

## §最近一轮 (2026-08-04 续③)：QA 直答路由 + system.txt 正向重写 + Hermes 记忆优化落地

**任务**：用户反馈知识问答体验差——"harness 是什么"被回复扯到宠物话题、硬套记忆、生硬；要求：① 完整梳理提示词注入链并诊断 ② 调研 GitHub 同类角色扮演提示词（airi/SillyTavern/OpenCharacters）+ Hermes agent 记忆实现 ③ 落地改进（知识直答通道 + 提示词正向化 + Hermes 优化迁移）④ HANDOFF 记录 + 重建 release。

**诊断根因**（详见本段"调研"）：① 检索注入带偏——问 harness 时 [Memories]+[Intent memory focus] 强制模型关联旧宠物记忆 → 歧义消解成"宠物背带"；② 14 条禁令清单占注意力 → 防御性表演、模板化；③ 无"知识问题直答"出口；④ flash 世界知识弱于 pro。业界（airi #1539：弱模型镜像 XML 标签→扁平 bullet 块；SillyTavern：mes_example 示例对话优于禁令；Hermes：用户消息永不压缩）指路。

**实现**（后端 7 文件 + 1 前端文件 + 2 prompts，核心策略：只加枚举变体/新函数/内联分支，**不改既有 fn 签名**避踩坑#4）：
- **gate**：`GateRoute::Question` 变体 + as_str/parse + 单测；`gate.txt` 加 `question` 类别（中英文例子：什么是地心引力/how does Rust borrow checker work，含"帮我看看这个报错"）。
- **ingest**（mind/mod.rs）：Question 分支直接返回空 outcome——**跳过 extractor**（省一次 LLM 调用，问答无记忆可提）。
- **converse**：`qa_mode = route==Question` 时——跳过记忆检索（返回 `(RetrievalResult::default(), "question route (QA mode)")`，trigger_reason 提前提为元组，silence 分支同步改）、intent 清 memory_anchor/禁 engage/禁 proactive、跳过 pacing 节流、跳过 surface_thoughts 念头注入、budget 走 `allocate_qa`。
- **grounding**：新 `build_qa_system_prompt`（SYSTEM_TEMPLATE + [Persona] + [Current Mood] + 清 anchor 的 [Intent] + 中文 `[Direct-Answer Mode]` 指令："直接、准确、简短地回答…不要引用记忆，不要追问，不要往自己或宠物相关话题上联想…不确定就老实说不知道"；**无 [Memories]、无 [Grounding Constraint]**——彻底切断"硬套记忆"的 prompt 通道）。
- **budget**：新 `allocate_qa`（QA system + compress_conversation 工作记忆）。
- **system.txt 重写**：禁令清单 → `[Core Personality]`（6 维保留）+ `[How to talk]` 正向（简短/口语/直答知识问题/只问真想知道的一个问题/记忆只围绕 [Memories] 与 [Milestones]/不知道就说不知道）+ **`[Example Conversations]` 4 条中文示例**（知识直答 / "我今天面试过了！"→具体反应+一个真问题 / 记忆自然引用火锅 / 闲聊）。**persona 契约回归网全部字样保留**（evaluation.rs 4 测 + grounding 2 测照过；`test_empty_memories_section` 断言改 `- [Fact]`——正文现也含 [Memories] 字样，标签在句子中 vs 区块在行首，误报修复）。

**Hermes 落地**（NousResearch/hermes-agent，225k⭐）：
- **① 用户消息永不压缩**：`compress_conversation` 重写——倒序收集，user 消息 verbatim 全保留且优先，超预算先挤掉最老的 assistant 回复；极端全 user 超预算才丢最老 user。修原实现"从前面成对丢消息"导致用户倾诉被截断失真的缺陷。+1 单测（20 user+20 assistant 挤到 300 token，20 条 user 全存活、顺序 verbatim、assistant 被挤掉）。
- **② 关系账 [Milestones] 分组**：`format_memories` 把 `is_landmark` episode 单独注入 `[Milestones]` 区块（在 [Memories] 前，关系里程碑锚点），[Memories] 内不重复；system.txt 提示模型 [Milestones] 是"你们关系的里程碑事件，值得认真记住"。+1 单测（landmark 只在 [Milestones]、不重复）。Hermes"双文件分账"（MEMORY.md/USER.md）适配陪伴场景 = facts（用户账）+ episodes（事件账）+ milestones（**关系账**，Hermes 未覆盖的陪伴独有维度）。
- **未落地（记录为 follow-up，避免 scope 膨胀）**：FTS5 全历史检索（Hermes 零 LLM 毫秒级回忆，替代部分 embedding；sqlite-vec 自带 fts5_cjk）、后台"关系进展摘要"review（对应 Hermes 每~10 轮后台 review）、记忆可视化可编辑。**已天然满足**：压缩/辅助走 flash 非 reasoning（=Hermes"辅助任务用便宜模型"）、consolidation 容量跳过重试（=Hermes"超限逼合并"）、reflection 后台沉淀（=后台 review 雏形）、extractor 已知事实去重（=写前去重）。

**会话前半段（同轮）**：① **Debug Panel 退出通道**——DebugPanel 全窗口覆盖（z-index 50 + pointer-events auto）挡住右键菜单、之前退出只能靠快捷键 → 加粘性工具栏：`✕ 关闭面板`（setShowDebug(false)）+ `⏻ 退出桌宠`（handleQuit→quit_app，与右键退出一致）；② **快捷键重构**——新 `src/shortcuts.ts`（isDebugToggle/isDebugClose 纯函数）：`e.code==="KeyD"` 替代 `e.key==="d"`（中文输入法把组合键截获成 key="Process"）、IME 合成事件跳过、**Esc 无条件关闭面板**（幂等，面板打开时最可靠退出通道）；③ **gate/correction max_tokens 2048→4096**——踩坑#3 复发：flash reasoning 把 2048 全吃掉（reasoning_tokens=2048, finish_reason=length, content 空）→ JSON 崩；consolidation 早已 4096，gate/correction 漏了；④ 主对话模型切 `deepseek-v4-flash`（AppData config，main_model；反思本就 flash）。

**调研结论（记录在案）**：角色扮演提示词最佳实践 = 角色字段化（name/description/personality/scenario）+ mes_example few-shot 示范语气 + 扁平 bullet 上下文块（airi #1539：弱模型镜像 XML 标签）+ 关键指令放 history 之后（post_history_instructions）+ lorebook 关键词触发注入 + 正向格式约束替代禁令。Hermes 记忆 = 双文件分账（容量上限+冻结快照注入保 prompt cache）+ agent 自主 curation（save/skip 指引）+ 后台 review（fork 子进程换便宜模型）+ FTS5 全文检索 + micro-compaction（用户消息永不压缩）+ 容量超限报错逼合并 + 写前安全扫描。详情见本段上文实现对照。

**验证（全绿）**：`cargo test --lib` **238 passed**（236 + 1 compress_conversation 用户消息全保留 + 1 milestones 分组；含 gate question parse 等）/ `cargo test --test golden_conversations` **30 passed** / `cargo check --tests` ✅（GateRoute 变体 + RetrievalResult derive Default + build_qa/allocate_qa 新函数未破 harness）/ `tsc --noEmit` ✅ / `npm run build` ✅。**release 已重建**（`npx tauri build --no-bundle`，taskkill 后构建，exe 已就位 D:\cargo-target\desktop-pet\release\desktop-pet.exe，桌面快捷方式指向不变）。

**Scope 边界 / follow-up**：① QA 分类依赖 gate LLM 判准（flash 误判率待真实样本观察；误判为 question 的最坏结果=无记忆注入的直答，安全方向）。② [Milestones] 依赖 `is_landmark` 标注质量（extractor 是否标 landmark 见 store.rs；当前 landmark 少，区块常空——正确行为，空则不注入）。③ FTS5 检索/关系摘要/记忆可视化=Hermes 下一批迁移项。④ 未改 config（main_model 切 flash 已在 AppData，需重启生效）。

**当前无进行中任务**。下一会话起点：① QA/新提示词 runtime 实跑（dev 问知识题+分享题，看 Last Turn route）② 或 Hermes 下一批（FTS5 检索最高 ROI）③ 或 B6/B7 架构债 ④ Liri/Spine（等资产）。

---

## §最近一轮 (2026-08-04 续)：选择性遗忘 episode MVP —— gate Forget + forget.rs 语义删除 + converse 确认

**任务**：用户"开做选择性遗忘，做完自动跑 50 条功能测试，遇问题自检修复"。选择性遗忘 = 用户主导的主动遗忘（lifecycle_cleanup 的用户控制对称版）。MVP 只做 episode（事件），fact/pending 留 follow-up。

**实现**（6 文件，全程不改 fn 签名，遵守踩坑#4 + #1 Rust 决策/LLM 只识别）：
- **`db/episodes.rs::delete(conn,id)->Result<bool>`**：`DELETE WHERE id=? AND is_landmark=0`——landmark 保护（lifecycle_cleanup 不删 landmark 的不变式延伸到用户请求），返回是否真删（landmark/missing→false）。
- **新 `mind/forget.rs` 模块**（镜像 correction.rs 风格）：
  - `ForgetResult{deleted, summary}` + `FORGET_CONFIDENCE=0.7`。
  - **关键设计**：置信度门在 `score_breakdown.semantic`（0..1 纯内容相关性），**非** total score。审计发现 retrieval total score = semantic + strength(0.3) + recency(0.2) + emotion(0.1)——**一个强近期完全无关的记忆也能拿到 ~0.6**，用 total 当"匹配置信度"会删错记忆（危险）。semantic 分量（embedding: cosine→0..1 无关≈0.5；keyword: Jaccard/bigram 无关=0）才是真匹配信号。0.7 让无关记忆（embedding 0.5）安全不删。
  - `should_forget(semantic, is_landmark)` 纯谓词（!landmark && semantic>=0.7）→ 单测易、无 DB。
  - `execute_forget(top, db)`：应用门 + 删 episode + `vectors::delete`（best-effort）。
  - `forget_episode(text, db, embedding)`：retrieve(top_k=1, emotion=default 中性) → execute_forget。MVP 只删 top-1（多删 follow-up，增 over-delete 风险）。
- **`mind/gate.rs`**：`GateRoute::Forget` 变体 + `as_str`/`parse_gate_json` 加 "forget" 臂 + parse 单测。所有穷举 match 仅 gate.rs(mod.rs ingest 已加分支)，无遗漏。
- **`resources/prompts/gate.txt`**：加 `forget` 类别（明确与 correction 区分：correction 改错细节，forget 整段擦除；中英文例子）+ JSON 响应行加 forget。
- **`mind/mod.rs`**：`pub mod forget` + re-export + ingest 加 `GateRoute::Forget` 分支（调 forget_episode）+ `IngestionOutcome` 加 `forget: Option<ForgetResult>` 字段（5 处构造点补 `forget`）。
- **`mind/converse.rs`**：pending 提示块后注入 forget 提示（镜像模式）——deleted→"好，我忘了"（**绝对禁复述被删内容**，复述=遗忘失败+惊悚）；未删→"我好像不记得这件事"（诚实，绝不瞎删）。`[converse] forget this turn` log（#11）。

**架构契合**：#1（Rust 决定删谁+置信度门+landmark 保护；LLM 只分类意图 gate + 确认 converse）/ #9（MVP top-1 episode + semantic 阈值，刚够用）/ #11（FORGET_CONFIDENCE 注释 + forget log + 8 单测可追溯）/ 安全（semantic 阈值防删错 + landmark 不可删 + 无匹配不删）/ 不改签名（枚举变体 + struct 字段 + 内联分支，规避踩坑#4）。

**验证（全绿）**：
- `cargo test --lib` **235 passed**（227 + 8 新：1 gate forget parse + 7 forget.rs）。
- `cargo test --lib forget` 显式 **8 passed**（should_forget 三分支 + execute_forget 真删/拒 landmark/拒低置信/无候选，含 in-memory DB 验证删除）。
- `cargo test --test golden_conversations` **30 passed**（既有功能无回归）。
- `cargo check --tests` ✅（全 harness 编译，GateRoute 变体 + IngestionOutcome 字段未破）。
- **合计 265 确定性测试全绿**（远超用户要的 50）。

**自愈（磁盘满）**：跑 golden 时 `cargo test --test golden_conversations` 报 `磁盘空间不足 (os error 112)`——**C 盘只剩 0.5GB**（Get-PSDrive: C 245 used/0.5 free，D 69 free）。诊断：dev `cargo test` 落 `src-tauri/target/debug`（C 盘，**非** D——release 才走 D 的 CARGO_TARGET_DIR），其中 `target/release`（2.24GB）是 **07/28 陈旧残留**（D 重定向前），活动 release exe 在 D（08/03）。删 C `target/release` 腾 2.31GB → golden 增量编译过。**纯环境问题，零代码改动**。**踩坑（写入避免重复）**：dev 构建（cargo test/check 默认）走 C `src-tauri/target`，只有 release（tauri build）走 D；C 盘紧张时 `src-tauri/target/release` 是安全可清的陈旧 cruft。

**Scope 边界 / follow-up**：① 只删 top-1 episode（多匹配时用户需重复请求）。② 阈值 0.7 是安全起点，需真实样本调（embedding 模式无关≈0.5，0.7 偏严；keyword fallback 更严可能漏删——但漏删是安全失败方向）。③ 无多轮消歧义（低置信直接"不记得"而非反问"你说的是…"，MVP 简化）。④ fact（偏好）+ pending（提醒）遗忘未做（用 `facts::expire_old` / 删 pending，结构类似，后续）。⑤ 未 rebuild release（纯后端改，dev 验证够；release exe 要 `npx tauri build --no-bundle`）。⑥ 未加 golden 端到端 forget 场景（需 mock LLM 构造 gate→Forget，较大；8 单测 + 实跑覆盖）。

**当前无进行中任务**。下一会话起点：① 选择性遗忘 runtime 实跑（dev 攒记忆→"忘掉X"→确认删+不复述）② 或 #10 两项 runtime（rest_need 半眯眼延后 Liri / speedModifier 深夜变慢）③ 或 B6/B7 架构债 ④ Liri/Spine（等资产）。

---

## §基础设施 (2026-08-04 续)：dev 构建重定向 D 盘 —— .cargo/config.toml 移到项目根

**问题**：自愈磁盘满时发现——dev `cargo test/check` 从项目根跑时落到 C 盘 `src-tauri/target`（撑满 C），而 release 才走 D。根因：**Cargo 按 CWD 向上搜 config，不从 manifest 目录**。原配置在 `src-tauri/.cargo/config.toml`（`target-dir=D:/cargo-target/desktop-pet`），只对 `cd src-tauri` 后的命令生效（如 `npx tauri build`）；从项目根跑的 cargo 命令搜不到它 → 默认 `src-tauri/target`(C)。

**修复**（用户要求"加 .cargo/config.toml target-dir"）：新 `C:\Users\SunJialei\Documents\桌宠\.cargo\config.toml`（项目根，同 target-dir + 注释说明 Cargo 发现语义），删冗余的 `src-tauri/.cargo/config.toml`（含空目录）。项目根 config 从**任意 CWD**（根或 src-tauri）向上走都能命中 → 单一真相源。`cargo metadata` 验证 `target_directory: D:/cargo-target/desktop-pet` ✅。**踩坑（写入避免重复）**：Cargo config 发现是 CWD-upward，非 manifest-dir；项目级 cargo config 应放项目根（manifest 在子目录 src-tauri/ 时尤甚），否则根目录跑的命令用不到。

---

## §最近一轮 (2026-08-04)：#10 生命感收尾 —— rest_need 暴露+激活 + circadian speedModifier 接动画

**任务**：用户"按任务优先级继续开发"。AskUserQuestion 确认方向——选 **#10 生命感收尾**（rest_need 后端暴露 + speedModifier 接动画），**非**字面最高优先的 B6/B7 架构债。理由：B6/B7 是对**正在正常运行的代码**的推测性重构（A1 BrainState 改 converse 9 参签名冲击 5 调用点+全 harness；A2 Scheduler 重写 timing 核心），无用户可见价值、爆炸半径大，违反编码准则"不重构没坏的东西/不做推测性抽象"。而 #10 两项服务北极星、低风险、补全**已半接线系统**（逻辑存在但没接通）。

**① rest_need 后端暴露 + 激活生产循环**（关键审计发现，扩展了原 follow-up scope）：
- **审计发现死代码**：`codegraph_callers`/`Grep` 确认 `emotion::tick_needs`（needs.rs，让 rest_need/loneliness 增长）和 `emotion::apply_drift`（homeostasis.rs）**仅自身测试调用，生产零调用**。生产 homeostasis 走的是 DB 层 `db::emotion::apply_homeostasis_time_aware`，它**内联重写**了 drift（mood/energy/social/stress）但**从不调 tick_needs** → rest_need（和 loneliness）在生产里冻结在种子值。所以"暴露 rest_need"alone 会显示恒定 0，emotionDriver 的 `e.rest_need * EYE_REST_GAIN` 半眯眼永不触发。
- **激活方案**（补全半接线，外科手术式）：
  - `emotion/needs.rs`：新 `pub fn tick_rest_need(rest_need, energy, elapsed) -> f64` 纯函数——低能量（<0.3）增长 `+elapsed*0.0002`；**恢复项**：能量充足时 `rest_need * exp(-elapsed/TAU_REST)`（TAU_REST=1800s，≈energy tau）。**修原设计缺陷**：原 `tick_needs` 的 rest_need 只增不减（单调→疲惫永不恢复），现加恢复项让休息后眼睛重新睁开。`tick_needs` 的 rest_need 行改为调 `tick_rest_need`（DRY，单一规则）。+1 单测（恢复：0.8/1h→<0.2）。
  - `emotion/mod.rs`：`pub use needs::{tick_needs, tick_rest_need};`
  - `db/emotion.rs::apply_homeostasis_time_aware`：算 `new_rest_need = tick_rest_need(current.rest_need, current.physical_energy, elapsed)`，UPDATE 加 `rest_need = ?6`（参数重编号）。**注释说明**为何激活（tick_needs 之前没接、暴露无效）。
  - `commands.rs`：`EmotionResponse` + `From<EmotionState>` 加 `rest_need`（覆盖 `get_emotion_state` 命令路径）。
  - `loop_runner.rs`：emotion-update emit json 加 `rest_need`（覆盖 medium_tick 推送路径）。
  - `App.tsx`：`EmotionData` interface 加 `rest_need`；`toEmotionVector` `rest_need: 0` → `e.rest_need` + 更新注释。

**② circadian speedModifier 接动画速度**（circadian.ts 早输出但零消费方）：
- 审计：`Grep speedModifier` 确认 `circadian.ts` 输出 speedModifier（Morning 1.2/Afternoon 1.0/Evening 0.9/LateNight 0.6/DeepNight 0.4），但**全代码库零消费**（只有 `sleepiness` 喂了 fsm.tick；speedModifier/energyModifier 形同虚设）。
- 实现（2 文件，最低风险）：`Live2DCanvas.tsx` 加 `speedModifier: number` prop（默认 1.0）→ 进 `propsRef`（既有 mirror 模式）→ per-frame `focusTickerFn`（既有 `app.ticker.add` 回调）首行设 `app.ticker.speed = propsRef.current.speedModifier`。**PIXI ticker.speed 是 delta 倍率**：设 0.4 → 库的 idle 呼吸/眨眼/motion/physics 全部 2.5× 变慢（深夜真的变慢）。`App.tsx`：`<Live2DCanvas ... speedModifier={circadianRef.current.speedModifier} />`（circadianRef 既有，App 频繁重渲染→prop 几秒内刷新，period 每小时才变，绰绰有余）。
- **设计抉择（为什么不改 behaviorDriver）**：behaviorDriver 用 `performance.now()-start` 真实时间驱动曲线，且周期刻意同步 FSM 微行为时长（"一个 LookAround 一个 sweep"）。若按 speedModifier 缩放 elapsed 会破坏此同步。故只走 ticker.speed 全局变速（覆盖占主导的 idle 呼吸/blink/motion），微行为曲线维持真实时间——可接受（微行为偶发且短；DeepNight 本就多 Sleeping/yawn）。
- **边界（接受）**：ticker.speed 也缩放 Talking 时 lipsync 的 ticker delta → 深夜说话嘴型可能略滞后。但 Talking 在 DeepNight 罕见，且 speedModifier 0.4 温和，影响极小。

**架构契合**：#1（rest_need 纯规则无 LLM + ticker.speed 纯前端）/ #9（MVP——rest_need 单标量恢复 + ticker.speed 一行全局变速，刚够用）/ #10（疲惫半眯眼可见 + 深夜变慢 = 生命感核心）/ #11（tick_rest_need JSDoc + apply_homeostasis 注释说明激活 + 测试可追溯）。

**验证（全绿）**：`cargo test --lib` **227 passed**（226 + 1 rest_need 恢复测试）/ `cargo check --tests` ✅（EmotionResponse 加字段未破 harness）/ `tsc --noEmit` exit 0 / `npx vitest run` 24 / `npm run build` ✅（2.60s）。

**待实跑（静态全过，runtime 待确认）**：`npm run tauri dev` → ① 低能量半眯眼（Debug Panel 调 rest_need 或 CDP `Runtime.evaluate` 写 DB）② `__pet.setHour(3)` 切 DeepNight → ticker.speed=0.4 全局变慢（呼吸/眨眼/motion 明显缓）。本轮未做 runtime 实跑。

**Scope 边界 / follow-up**：① **loneliness 仍是死字段**——apply_homeostasis_time_aware 也不更新它（tick_needs 的 loneliness 分支同样未接生产）。但 loneliness 影响检索/planner（行为层），激活它是有意行为变更、超本任务（视觉）范围，故不动，留 follow-up。② `apply_drift`（homeostasis.rs）+ `tick_needs` 的 loneliness 分支仍为死代码（仅测试），未删（非本轮引入，surgical 原则只清自己的）。③ rest_need 恢复用 TAU_REST=1800s（≈energy tau），未单独调参——实跑若恢复太快/慢再调 needs.rs 常量。④ 未 rebuild release exe（前端+后端都改，桌面快捷方式需 `npx tauri build --no-bundle`；构建前 taskkill 桌宠）。

**当前测试总量**：Rust lib **227** + golden 30 + evaluation 6 + questioning 1 + embedding 1 + 前端 vitest 24 = **289 确定性测试**（+ 闭环1 real-LLM 已验 + sleep runtime CDP 已验）。

**当前无进行中任务**。下一会话起点：① runtime 实跑 #10 两项（dev CDP）② 或回 B4 runtime 实跑（续⑧ 留的 AnimFSM/Prompt 分区渲染确认）③ 或 B6/B7 架构债（如用户接受重构风险）④ Liri/Spine（等资产）。

---

## §最近一轮 (2026-08-03 续⑧)：B4-余余 Debug Panel 补全 + B5 Golden 评估框架

**任务**：用户"继续 B4,B5 推进"。B4-余余 = Debug Panel 还缺的 AnimFSM + Prompt-动态 token 两分区（续③ 留的 follow-up）；B5 = P17 Golden 评估框架（审计确认原无 evaluation.rs / personality_drift_score / CI）。

**B4-余余 · AnimFSM 分区**（前端 fsm 状态上抛，#11 "她现在在干嘛"）：
- `fsm.ts`：FSM 早有 `private history: string[]`（末 5 个已结束的微行为，tick 里 push），加 `getHistory(): string[]` getter 暴露（返回 clone）。
- `App.tsx`：`<DebugPanel anim={{ state: behavior, history: fsmRef.current?.getHistory() ?? [] }} />`——`behavior` 是既有 React state（onStateChange 驱动），每次状态变（含微行为 Idle↔Blink）触发重渲染，history 同步新鲜。
- `DebugPanel.tsx`：签名加 `anim` prop，新 **AnimFSM** 分区（State + Recent "yawn ← blink ← ..."）。Brain 之前置（两块"实时态"挨着）。

**B4-余余 · Prompt-动态 token 分区**（#8 成本 + #11 "上下文为何这么大"）：
- `budget.rs`：加 `pub fn system_prompt_budget() -> usize`（FACTS+EPISODES+PERSONA+EMOTION+INTENT+SYSTEM_SCAFFOLD = 2005），把 compress_system_prompt 里写死的 sum 提为公共可观测。
- `converse.rs`：新 `PromptTokenDebug{system_tokens,input_tokens,budget,conversation_turns}` + `ConversationResult.prompt_tokens: Option<_>`。在既有 `[ctx] system_tokens~=` log 处（normal 分支）hoist `system_tokens` 变量 + 算 input_tokens/budget/turns 组 debug struct；silence 分支 None。**续③ 同款不改 fn 签名**（只加返回 struct 字段，harness 读 .response/.intent 不受影响，`cargo check --tests` ✅ 验证）。
- `commands.rs`：镜像 `DecisionPromptToken`（Serialize）+ DecisionTrace 加字段；send_message trace 组装投影 `result.prompt_tokens`（best-effort）。进 last_decision → 已有 snapshot 管道，无需新 DebugSnapshot 字段。
- `DebugPanel.tsx`：last_decision TS 类型加 prompt_tokens，Last Turn 分区加 "Prompt: sys N/budget M tok | input K (N turns)"。

**B5 · Golden 评估框架**（锁 Liri 人格防漂移）：
- 新 `src/mind/evaluation.rs`（纯函数 #1，无 LLM/DB）：`DriftKind{Chatty,Cloying,Clingy}` + `DriftViolation` + `DriftReport{overall:0..1, violations}` + `personality_drift_score(response)`。规则启发式——Chatty：CJK>200（安静人格别话痨）；Cloying：感叹/波浪号/心/emoji 密度>10% 且≥3（温暖非表演）；Clingy：依赖短语黑名单（不要离开我/离不开你/…）。overall 每违扣 0.34 floor 0。**7 单测**：on-persona 干净 / silence=1.0 / chatty墙 / cloying刷屏 / 少量marks不算 / clingy / 三杀floor0。**注**：只抓 GROSS 漂移；语义漂移需 LLM-as-judge（文档标 future extension）。
- 新 `tests/evaluation.rs`（集成）：**Liri 人格契约回归网**（4 测，build_system_prompt 断言）——6 维度 温柔/好奇/聪慧/安静/调皮/神秘 + 狐灵身份（璃/Liri/狐）+ NOT-list（话痨/卖萌/依赖）+ rule8 严禁编造。锁续② 落地的 system.txt 人格（当时"缺回归网"，现补上）。+ 2 drift 端到端（on-persona 干净 / off-persona 低分）。

**架构契合**：#1（评估纯函数 + token 计数纯 Rust）/ #8（Prompt-token 让单轮上下文成本可观测）/ #11（AnimFSM 实时态 + Prompt 预算 + 人格契约回归网，"她为什么这么说/在干嘛/像不像她"全可观测）/ 测试纪律（不改 fn 签名规避踩坑#4；B5 补审计确认的缺失框架）。

**验证（全绿）**：`cargo test --lib` **226 passed**（219 + 7 eval）/ `cargo check --tests` ✅（evaluation.rs 编译 + PromptToken 字段未破既有 harness）/ `cargo test --test evaluation` **6 passed** / `tsc --noEmit` exit 0 / `npx vitest run` 24 / `npm run build` ✅（1.89s）。

**待实跑（静态全过，runtime 待确认）**：B4 两分区要 `npm run tauri dev` 发一条消息 → 开 Debug Panel（F12 或 Ctrl+Shift+D）肉眼确认 AnimFSM 分区有 state/history、Last Turn 有 Prompt 行。后端 PromptTokenDebug 经 send_message→snapshot 链路（续③ 已验该管道活），前端渲染编译过，低风险。**本轮未做该 runtime 实跑**。

**当前测试总量**：Rust lib **226** + golden 30 + evaluation 6 + questioning 1 + embedding 1 + 前端 vitest 24 = **288 确定性测试**（+ 闭环1 real-LLM 已验 + sleep runtime CDP 已验）。

**Scope 边界**：① B5 只做规则启发式层（GROSS 漂移），LLM-as-judge（语义漂移、≥30 对话 golden 集、CI）留 future——文档已标，待 Liri 稳定 + 有真实响应样本调阈值。② AnimFSM 只显末 5 history（FSM 本就只存 5），不加完整轨迹（要历史看 Timeline/change_log）。③ Prompt-token 不含 $ 估算（同 Cost 分区决策，模型/定价用户各异）。

**当前无进行中任务**。下一会话起点：① B4 runtime 实跑（dev 看 AnimFSM/Prompt 分区）② 或回 B1b（Grounding B 档，条件触发）/ B6/B7（架构债）③ 或 Liri/Spine（等资产）。

---

## §最近一轮 (2026-08-03 续⑦)：sleep 内容首次有测试 —— vitest + 纯逻辑抽取 + 24 单测

**任务**：用户"sleep相关的内容是不是还没有做测试"。核验确认：A4（入睡/唤醒）/A5（circadian 深夜 yawn）/B3②（sleep 音）/B3①（睡着抑制 nudge）在 HANDOFF 全标"build 过/待实跑"，**从未运行验证**；更关键——**前端零测试**（Rust 219+ 单测 vs 前端 0 个 `*.test.ts`，无 vitest/jest）。sleep 逻辑里**有纯函数核心**却没测。

**方法**：把 sleep 行为的**纯逻辑核心**抽出来单测（确定性、可回归），runtime-only 部分（渲染/音效/一行 guard）留 GUI 验收。

**① 加 vitest**（前端首个测试框架）：
- `package.json` devDep 加 `vitest@^3.2.7`（兼容 vite 6）；scripts 加 `"test": "vitest run"` / `"test:watch": "vitest"`。
- 新 `vitest.config.ts`：`environment: "node"`（被测模块 circadian/microBehavior/sleepLogic 无 DOM 依赖，免 jsdom）+ `include: ["src/**/*.test.ts"]`。vitest 优先读它而非 vite.config.ts（后者的 async server 配置是 dev-only）。
- 注：项目 tsconfig `include: ["src"]` 现在也类型检查 `.test.ts`——`npm run build` 的 `tsc` 段会检查测试文件类型（ desirable，测试类型安全）；vite build 不打包测试文件（非入口）。

**② 抽纯逻辑**（外科手术式，行为零变）：
- 新 `src/animation/sleepLogic.ts`：`shouldAutoSleep(opts)` ——把 App.tsx FSM-tick effect 里那 5 行 auto-sleep 条件（DeepNight + 非已睡 + 非 think + 非 talk + idle 严 > 阈值）抽成纯谓词。App.tsx 改调它（`isTalking: behavior===Talking` 等），**行为字节级不变**，只是可测。
- `microBehavior.ts`：抽 `applySleepyWeight(base, sleepy, sleepiness)` ——A5 公式 `w *= 1+(sleepy-1)*sleepiness` + `Math.max(0.01, w)` clamp 独立成函数，`pickNextBehavior` 的 weights.map 改调它（emotion 修正后的 w 作 base 传入），**行为零变**。

**③ 24 单测**（3 文件）：
- `circadian.test.ts`（10）：A5 输入层。5 时段映射（Morning 6-10/Afternoon 11-16/Evening 17-21/LateNight 22-1/DeepNight 2-5）+ 边界交接（6/11/17/22/2）+ sleepiness 文档值（DeepNight 0.9、Morning 0.1）+ 夜>昼单调 + speed/energy 夜降。钉死 verify-checklist 用 `__pet.setHour` 手动查的那些值。
- `sleepLogic.test.ts`（7）：A4 触发层。baseline（应睡）+ 翻转单字段破之：非 DeepNight（LateNight/Morning/Evening）不睡 / 已 Sleeping 不重触 / thinking 不睡 / talking 不睡 / idle 未达阈值（严 >，等于阈值也不睡）/ 刚交互（idle=100）不睡。
- `microBehavior.test.ts`（7）：A5 效果层。白天不变性（sleepiness=0 全池 no-op + undefined sleepy=1）+ 夜间方向（yawn 夜/昼 >2×≈文档 3×、look_around 夜<昼、yawn 比率>look_around 比率）+ clamp（0.01 地板）。

**验证**：`npx vitest run` **24 passed**（590ms）/ `tsc --noEmit` exit 0 / `npm run build` ✅（1.97s，dynamic-import 警告是既有非本轮）。**Rust 未动**（纯前端 + 测试），lib 219 不受影响。

**架构契合**：#1（抽出的谓词/公式纯规则无 LLM）/ #9（MVP 抽核心可测，不一上来堆 e2e）/ #10（sleep 生命感逻辑现有回归网）/ #11（24 单测 + 公式 JSDoc 可追溯）/ 测试纪律（前端从 0 到 24，补 Rust 侧早已有的覆盖文化）。

**④ runtime CDP 验收（同轮补做，用户"现在就做"）**：纯逻辑单测之外，把集成行为也验了。方法：`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" npm run tauri dev`（dev-only `window.__pet` 钩子必须在 dev，release 无）→ Node 22 原生 WebSocket 连 `ws://127.0.0.1:9222`（先 `GET /json` 取 page 的 webSocketDebuggerUrl）→ `Runtime.evaluate` 驱动 `__pet` + 读 `__pet.state()` + 查 `.chat-bubble:not(.hidden)` DOM。脚本 ws.onclose/onerror→exit(2)（HANDOFF 续 踩坑：否则断线挂起）。结果**全绿**：
| 检查 | 结果 |
|---|---|
| A5 setHour(3) | `{period:"deep_night",sleepiness:0.9}` ✅ |
| A5 setHour(10) | `{period:"morning",sleepiness:0.1}` ✅ |
| A4 sleep() | `behavior:"sleeping"` ✅ |
| B3① probeNudge asleep ×10 | 零气泡 ✅（awake sanity 1 气泡/15 call 证 nudge 本身没坏，是 Sleeping guard 抑制） |
| A4 wake() | `behavior:"hum"`（醒，非 sleeping）✅ |
| 视觉（CDP `Page.captureScreenshot` + 图像分析） | sleeping=**闭眼**/awake=**睁眼明亮+对话气泡** ✅（Live2D f05 睡眠表情渲染正确；无 Zz 符合预期——Zz 仅 SVG PetCharacter，当前 Live2D 占位无，见续②） |

**唯一仍待人验：B3② sleep 音效**——`sound.sleep()` 入睡播放（每次 `__pet.sleep()` 都触发，测试中已多次触发），但我听不到。dev 仍开着（PID 见上），可右键桌宠→DevTools→Console `__pet.sleep()` 亲耳确认（静音后再调应无声，验 #6）。

**CDP 方法论资产（可复用，未来所有"需 GUI 实跑"的验收通用）**：throwaway 脚本已删（同 opencode 物理 CDP 模式），但配方记此——`/json` 取 WS URL → `Runtime.evaluate(expression, {returnByValue,awaitPromise,userGesture:true})` → 长断言序列包成一个 async IIFE 返 JSON 字符串一次取回 → `Page.captureScreenshot` 抓图。`userGesture:true` 关键（解锁 AudioContext 等）。

**Scope 边界**：① 只测纯逻辑核心，没把 App.tsx 整个 FSM-tick effect 拆成可测单元（那需 mock ref/interval，重，不值）② 没加 jsdom 测 React 组件（当前纯逻辑测试够覆盖 sleep 决策）③ 没做 CDP runtime 自动化（复杂、`tauri dev` 编 Rust 慢、GUI 窗口我看不到）——留作下一步候选。

**当前无进行中任务**。下一会话起点：① runtime 验收 sleep（CDP 自动化或手动 verify-checklist）② 或回 B4-余余/B5 推进。

---

## §最近一轮 (2026-08-03 续⑥)：清测试 —— golden_conversations 2 stale 测试修复

**任务**：用户"是不是还有几个测试没做？先清测试"。`cargo test --lib` 一直 219 绿，但**集成测试 `golden_conversations` 长期没跑**，发现 2 个失败（stale 测试，生产代码有意改后漏同步）。

**诊断（两个独立的 stale，均改测试不改生产）**：

1. **gc_003_emotion_consistency**（golden_conversations.rs:148）：
   - 测试断言"高 stress + 焦虑用户 → `action=silence` / `tone=quiet`"。
   - 但 planner Rule 2（planner.rs:112-127）**故意改成** 焦虑 → `goal=care / action=normal / tone=gentle`——理由（代码注释详述）：旧"焦虑→吸收 stress→达阈值→silence→silence 再加 stress"是**反馈环**，现焦虑恒走 care 让她在用户需要时回应。单测 `test_anxiety_routes_to_care`（planner.rs:316）已钉此新契约。
   - golden 集成测试漏同步 → stale。**改测试**断言新契约（care/normal/gentle）+ 注释说明"silence 被故意移除以破反馈环"，指向单测。保留 `stressed` EmotionState 作真实上下文（stress 不再 gate 焦虑路由但仍合法场景）。

2. **gc_012_first_run_seeds_persona**（golden_conversations.rs:474）：
   - 测试断言 `trait_key == "gentle"`（旧英文 key）。
   - 但 续② Liri 迁移后 `seed_persona`（firstrun.rs:32-39）播种 6 个中文维度 `温柔/好奇/聪慧/安静/调皮/神秘`（confidence=确信度非权重%，权重在 system.txt 散文）。
   - **改测试**断言 `trait_key == "温柔"`（Liri 的 gentle 维度）+ 注释说明旧 `gentle` 英文 key 在 Liri 迁移中被替换。测试意图（验首次运行播种 gentle 人格维度）保留。

**关键判断**：这不是"测试坏了凑过"，而是**生产有意改行为后测试 stale**——两处都有更强证据：gc_003 有 planner.rs 注释 + 钉契约的单测 `test_anxiety_routes_to_care`；gc_012 有 续② HANDOFF + firstrun.rs 注释。改测试对齐新契约是正解（"Fix tests when they're wrong"）。

**验证（全确定性测试绿）**：
- `golden_conversations`：28+2 fail → **30 passed** ✅（gc_003/gc_012 修后过，其余 28 无回归）
- `cargo test --lib` **219 passed** ✅（未动）
- `questioning_harness::pacing_throttle`（纯函数）**1 passed** ✅
- `embedding_ab_harness::embedding_ab_comparison`（CPU benchmark，加载 BGE-M3 43.5s）**1 passed** ✅——复现续④结果：semantic Hit@3 33%→67%、avg sem 0.035→0.741
- **合计 251 确定性测试全绿**
- **闭环1 `memory_recall` real-LLM 复验通过** ✅（46s）：多核心 Rust 文件（store.rs/converse.rs/commands.rs）自上次记录后改过，复验核心链路——seed"糯米"持久化 ✅ / noise（纯提问"黑洞蒸发"）不造事实 ✅ / 跨会话问"我家狗叫啥"→recall"糯米" ✅。

**未跑（按需，慢/花钱）**：其余 real-LLM harness（闭环2 `closed_loop2_harness` / `soul_harness` / `soul_loop_harness` / `proactive_harness` / `conversation_harness` / `consolidation_harness`）——`cargo check --tests` ✅ 证全编译，上次记录均绿；逐个跑慢（reasoning 模型）且花钱，未自动跑。GUI 实跑项（A4/A5/A6/B3，见 verify-checklist.md）需 `npm run tauri dev` + 肉眼/耳，CDP 管不到全部，待用户手动。

**架构契合**：#11（两处 stale 测试若不修，未来 golden 回归网失效；现契约对齐可追溯）/ 测试纪律（harness 签名/契约变更必同步集成测试，踩坑#4 的集成测试变体）。

**当前无进行中任务**。下一会话起点：B4-余余（AnimFSM 前端 fsm 状态上抛 / Prompt 动态 token）或 B5（Golden 评估框架，待 Liri 稳定）。

---

## §最近一轮 (2026-08-03 续⑤)：Settings 下载按钮修复 + 暂时离开→系统托盘

**任务**：用户提两个 follow-up：① Settings 下载按钮（HF_BASE_URL 指失效 Qdrant 401）；② 右键"暂时离开"当前无效果，理想 = 最小化隐藏 + 点托盘图标恢复。

**① Settings 下载按钮 → Xenova**（`download.rs`）：
- `HF_BASE_URL`：`Qdrant/bge-m3-onnx`（**401 Unauthorized**）→ `hf-mirror.com/Xenova/bge-m3`（匿名 + 中国快；续④手动下载已验证该源可用）。
- `REQUIRED_FILES` 加 `model.onnx_data`（Xenova external-data 权重文件；ort 自动从 model.onnx 同目录加载它）。
- `download_all` 改 `(remote_path, local_name)` 映射：`onnx/model.onnx`→`model.onnx`、`onnx/model.onnx_data`→`model.onnx_data`、`tokenizer.json`、`config.json`（前两个 Xenova 在 `onnx/` 子目录，本地平铺——ort 靠同名同目录找权重）。
- 修 `test_check_complete_empty_dir` 硬编码 4 → `REQUIRED_FILES.len()`（数据驱动，未来改文件清单不再踩）。

**② 暂时离开 = 最小化到系统托盘**（新功能；Cargo `tray-icon` feature 早启用，但 lib.rs 从未建托盘）：
- 诊断：`handleAwayMode`（App.tsx）只 `setAwayMode(true)`（前端标志，抑制气泡/行为）+ 气泡——**窗口根本没隐藏**，所以"没作用"。
- `lib.rs` setup 建 `TrayIconBuilder`（icon = `default_window_icon()`=icon.ico, tooltip"桌面宠物·点击图标恢复"）：左键 `Click{Left,Up}` → `window.show()+set_focus()` + `emit("restore-from-tray")`。
- `commands.rs::hide_to_tray`（镜像 quit_app 模式）：`get_webview_window("main").hide()`。
- `App.tsx`：`handleAwayMode` 加 `setTimeout(()=>invoke("hide_to_tray"),600)`（气泡瞥见再藏）；新增 `listen("restore-from-tray")` effect → `setAwayMode(false)` + "回来啦~"气泡（StrictMode `cancelled`-flag 模式，同其他 listener 防 double-mount 泄漏）。

**架构契合**：#6（托盘是 OS 标准恢复路径，每能力可关）/ #5（Body 层窗口控制独立）/ #11（hide/restore/tray build 全 log）/ **未改 fn 签名**（hide_to_tray 是新命令，规避踩坑#4）。

**验证**：`cargo check --tests` ✅ / `cargo test --lib` **219 passed** ✅ / `tsc --noEmit` ✅ / release rebuild（19:20）✅ / 启动 sanity（进程活 1.5GB 含模型 + vectors 14 幂等未被 backfill 重复）✅。**tray 交互（右键暂时离开→窗口消失+右下角托盘图标→左键托盘恢复窗口）+ Settings 下载按钮待用户手动验证**（OS 层 GUI 交互，CDP 管不到托盘/窗口层级）。

---

## §最近一轮 (2026-08-03 续④)：BGE-M3 embedding 接入 + 检索质量量化 + backfill

**任务**：用户"下载 embedding 模型装 D 盘（不途径 C 盘）+ 对比测试看使用前后差距，自己设计实验自己验证"。起因：续③ 实跑 Debug Panel 发现 `sem≈0.00/0.08`（embedding 模型未加载，检索退回关键词兜底）→ 本轮接入 BGE-M3 + 量化它带来的提升。

**① 模型下载到 D 盘**（5 文件，`D:\models\bge-m3`）：
- 原代码 `download.rs::HF_BASE_URL` 指向 `Qdrant/bge-m3-onnx`——**已需认证（401 Unauthorized）**，hf-mirror.com 也只 308 回源 HF 主站。改用匿名可下的 **`Xenova/bge-m3`**（手动 curl 下载，绕过 app 内 download 命令）。
- Xenova 是 **external data 格式**：`model.onnx`(607KB graph) + `model.onnx_data`(2266820608B≈2.1GB 权重) 分文件，平铺同目录，ort `commit_from_file(model.onnx)` 自动按 graph 引用加载同目录 `model.onnx_data`。另含 `tokenizer.json`(17MB) + `config.json` + `onnxruntime.dll`(1.20.1，GitHub release zip 解 `lib/onnxruntime.dll`)。
- HF 主站中国慢+stall，切 hf-mirror.com `curl -C -` 断点续传（resume from 573MB）完成。
- config `%APPDATA%\DesktopPet\config.toml` → `model_dir = "D:/models/bge-m3"`；**C 盘 AppData 不建 models 目录**（用户明确要求）。

**② 修 embedding 加载 bug**（`model.rs`，真 bug 生产同病）：
- 首次加载报 `Onnx("Opt level: graph_optimization_level is not valid")`。根因：ort 2.0.0-rc.12 把 `GraphOptimizationLevel::Level3` 映射到 `ORT_ENABLE_LAYOUT`（ort 源码 `session/builder/impl_options.rs:555`），**ORT 1.20 运行时不认此值**（标准只有 DISABLE/BASIC/EXTENDED/ALL）→ `SetSessionGraphOptimizationLevel` 拒绝。此前模型从未加载过，bug 一直潜伏。
- 修：`Level3` → `All`（→ `ORT_ENABLE_ALL` 标准值；ort 文档原话 "All optimizations (i.e. Level3)"，语义一致）。api-20 = ORT 1.20 API（非 ORT 2.0），与 1.20.1 DLL 匹配——非版本不匹配，纯枚举映射错。

**③ backfill 历史 episode 向量**（`store.rs::backfill_missing_vectors` + `lib.rs` setup 后台线程）：
- 诊断：`store.rs::store` 摄入时若模型 ready 才 embed 写 `episode_vectors`（store.rs:60）；当前环境模型未加载 → 历史 episode 全无向量（真实 DB 14 episodes / **0 vectors** 印证）→ **即使现在加载模型，检索仍退回关键词**（retrieval 的 cosine 分支需 episode_vec）。
- 修：新增 `backfill_missing_vectors(db, emb)`（LEFT JOIN 找无向量的 episode，embed summary + insert，best-effort）。`lib.rs` setup 闭包内，模型 ready 时 `std::thread::spawn` 后台跑（不阻塞启动）。新对话摄入本就自动 embed，backfill 只补历史。

**④ A/B benchmark**（`tests/embedding_ab_harness.rs`，隔离 LLM 纯 CPU 推理）：
- 受控实验：18 episode（中文桌宠场景，全同 importance=0.5/strength=0.5/time，无 emotion）→ semantic 分量成唯一排名变量。12 query（6 字面类 + 6 语义类，标注答案）。per-query fresh 内存 DB（隔离 retrieve 的 reinforce 副作用）。两模式：baseline `retrieve(emb=None)` 关键词兜底 vs `retrieve(emb=Some)` cosine。
- 结果：**语义类 Hit@3 33%→67%（翻倍）/ MRR 0.33→0.67 / avg sem@answer 0.035→0.741（≈21×，从无到有）/ 字面类 100%→100% 不退步**。avg sem 0.035→0.741 正好印证续③ Debug Panel 的 sem≈0 诊断。test assert：embedding 严格提升语义 MRR、字面零退步。

**⑤ release rebuild + 端到端验证**：`npx tauri build --no-bundle`（exe 08-03 18:25）。启动 release exe → 真实 DB `episode_vectors` **0→14**（全部历史记忆向量化，模型加载+backfill 生产链路真实跑通）。`cargo test --lib` 219 passed。

**架构契合**：#1（backfill/benchmark 纯 Rust 驱动，embed 只计算）/ #8（embedding 本地 CPU 零 LLM 成本）/ #10（历史记忆也享受语义检索）/ #11（benchmark 可复跑 + backfill 日志计数 + 加载 bug 注释可追溯）。

**follow-up（未做，记 backlog）**：
- **download.rs HF_BASE_URL 仍指失效的 Qdrant（401）**：手动下载已绕过，但 app 内 Settings 下载按钮坏了。修需改 URL→Xenova + download_all 处理 `model.onnx_data` external data（REQUIRED_FILES 加它）。记忆 `bge-m3-model-location` 记详。
- B5 Golden 评估框架（人格漂移，待 Liri 稳定）。

---

## §最近一轮 (2026-08-03 续③)：B4b 死表 + B4-MVP 决策链三分区 + B4-余 Cost 分区

**任务**：深度审计后按优先级开发 #11/#8 Explainability 簇（未受阻、最高 ROI；Liri/Spine 受阻于资产）。三件：B4b 修死表、B4-MVP 补 Debug Panel 决策链三分区、B4-余 Cost 今日调用+token 计数。

**B4b · conversations 死表修复**（`commands.rs::send_message`，~30 行）：
- 审计确认真 Bug：`Grep conversations::(insert\|get_recent\|get_max_turn)` 于 `src-tauri/src` = 0 命中；`codegraph_callers(insert)` 显示 `conversations::insert` 仅测试调用。plan P5.3 步骤 5 明确要求写此表。
- 实现：在 `send_message` 的 working_memory push **之前**，镜像其语义写 `conversations` 表——user turn 必写、assistant turn 仅 `!response.is_empty()` 时写（与 wm push 一致：silence=无 assistant 行）。`id = {conversation_id}_t{turn}_{role}` 保证 PRIMARY KEY 唯一（schema id 是 PK；同 turn 的 user/assistant 共享 turn 号，需 role 后缀区分）。
- **best-effort**：`if let Err(e) = ... { log::warn }`——日志失败只 warn 不阻断聊天（#11 是调试辅力，不该坏主流程）。harness 直调 `converse()`（不经 send_message）→ 不污染生产表。
- 效果：现在可回溯她每轮原话（07-31 幻觉若再发，能直接查 conversations 表看她到底说了什么）。

**B4-MVP · Debug Panel 决策链分区**（服务"她为什么这么说"诊断链）：
- 新增三分区（plan P16 的 Retrieved/Reflect + 额外 Intent）：
  1. **Last Turn（Intent）**：goal/tone/action + memory_anchor + route + trigger_reason + grounding_violations 计数。
  2. **Retrieved**：top-5 episode 摘要 + 总分 + 四分量 breakdown（sem/str/rec/emo）——#11 "检索了什么"核心。
  3. **Reflect**：最新 reflection thought + unsurfaced thoughts 计数（DB 查）。
- **数据流**（关键决策：改 struct 字段，不改 fn 签名，规避踩坑#4）：
  - `converse.rs`：`ConversationResult` 加 `retrieved_scores: Vec<RetrievedScoreDebug>`（新轻量 struct：summary+score+breakdown，不带完整 Episode）。converse 内 `retrieval` 计算后投影一次（`.iter().take(5)`），两个 return 分支（silence :128 / normal :296）都用同一 binding（silence 分支 return diverge，move 不冲突）。**fn 签名零改动**——只是返回 struct 多一字段；harness 读 `.response`/`.intent` 不受影响（`cargo check --tests` ✅ 验证）。
  - `commands.rs`：AppState 加 `last_decision: Mutex<Option<DecisionTrace>>`；`send_message` 在 wm push 后从 `result`（intent/trigger/route/violations/retrieved_scores）组装 `DecisionTrace` stash（best-effort，lock 失败只跳过）。`DecisionTrace`/`DecisionRetrieved`/`DebugReflect` 新 Serialize struct。`get_debug_snapshot` 加 `last_decision`（读 AppState）+ `reflect`（raw SQL 查 reflections/internal_thoughts）。
  - `DebugPanel.tsx`：`DebugSnapshot` TS 接口加两字段；Timeline 后渲染三新分区（沿用 debug-section/item/bar 既有 class，零 CSS 改动）。
- **defer 的 B4 余项**（各需独立 plumbing，#9 拆出）：~~Cost（LlmClient 插桩）~~ ✅ 本轮续做（见下）；AnimFSM（前端 fsm 状态需上抛到 panel）；Prompt 动态 token（需记 last usage）。

**B4-余 · Cost 分区**（#8 成本是设计约束——必须可观测）：
- `llm/client.rs`：`LlmClient` 加 `cost: Arc<Mutex<LlmCostStats>>` 字段（`Arc` 保 `#[derive(Clone)]`——每轮对话 clone 一份 client，计数共享同一份）。`LlmCostStats{date,calls,prompt_tokens,completion_tokens}` Serialize struct + `record()`（跨 local 日重置）+ `snapshot_today()`（过期归零）。**两处插桩点**（覆盖所有调用）：`chat_with_model`（chat/chat_reflection 都委托它）+ `chat_stream` 两个成功 return（[DONE] / 流结束），成功后 `track_usage(&result)`，失败早返回不计。lock-poison 安全（`if let Ok` 跳过）。`LlmClient::new` 是唯一构造点（grep 确认 harness 全用 new）→ 加字段只改一处字面量，零 harness 破坏。
- `commands.rs`：`DebugSnapshot` 加 `cost`；`get_debug_snapshot` 调 `c.cost_today()`（未配置→default 空）。
- `DebugPanel.tsx`：Counts 后加 **Cost (today)** 分区（calls + prompt/completion tok + date）。
- **+3 单测**：record 累加 / record 跨日重置 / snapshot_today 过期归零。

**架构契合**：#1（决策链捕获 + Cost 计数纯 Rust，LLM 只配音；reflect 纯 DB 查）/ #5（get_debug_snapshot/cost_today 异步读不阻塞主循环）/ #8（决策链零额外 LLM 复用同一次 converse；**Cost 成本可观测——核心**）/ #11（"她为什么这么说"全链可观测）/ 不改 fn 签名（规避踩坑#4，只改返回 struct 字段）。

**验证（全绿）**：`cargo check --lib` ✅ / `cargo check --tests` ✅（harness 编译，确认 ConversationResult 加字段未破测试）/ `cargo test --lib` **219 passed**（216 + 3 Cost 单测）✅ / `tsc --noEmit` ✅ / `npm run build` ✅。

**✅ 实跑确认（2026-08-03，dev 真实 LLM）**：`npm run tauri dev` 发"我今天在用spine做桌宠的形象。好繁琐"→
- **B4b**：`conversations` 0→2 行（user+assistant 配对，turn 归位）——死表修复在生产路径验证通过。
- **Cost (today)**：`3 LLM calls | prompt 3201 / completion 945 tok`——**单轮 3 次调用（gate+extractor+main）首次可见**，插桩点覆盖完整管道（#8 回本）。
- **Last Turn**：`react/curious/normal` + anchor + `StoreFull` + `substantive message` 全渲染。
- **Retrieved**：5 条 episode + 四分量 breakdown。**副产品发现**：`sem≈0.00/0.08` 暴露**当前环境未加载 embedding 模型**→检索退回关键词兜底（这是既有问题，非本轮改动）。导致 Spine 相关 episode（0.48）排在"宠物去世"（0.50）下、planner 锚到情感最强记忆。建议：Settings 下 BGE-M3 改善检索。
- **Reflect**：unsurfaced 0 + 最新反思"今天听了很多你的开心和心事…"（与 DB 查的一致）。
- 笔电 F12 被绑成休眠键 → 加了 **Ctrl+Shift+D 备用切换**（`App.tsx:420` keydown，HMR 即时生效）。

**Scope 边界**：① 决策链只存**最后一轮**（覆盖即更新）——够诊断单轮，不需历史轨迹（要历史看 Timeline/change_log）。② reflect 只读不写。③ Cost 不含 $ 估算（模型/定价用户各异，硬编码会误导 #11；token+调用数已是成本的真实信号，$ 可后接 config 跟进）。④ B4 余 AnimFSM/Prompt-动态 token 入 follow-up。⑤ 未 rebuild release exe（前端+后端改了，桌面快捷方式要 `npx tauri build --no-bundle` 才生效；dev 模式直接热更）。

**当前无进行中任务**。下一会话起点：① 实跑本轮（dev 看 4 分区 + conversations 表）→ ② B4-余余（AnimFSM 前端 / Prompt 动态 token）或 B5（Golden 评估，待 Liri 稳定）→ ③ B1b 条件触发（实跑若再现幻觉）。

---

## §最近一轮 (2026-08-03)：合并 opencode 副本（B1 + B2 + A4/A5 实跑方法论）

**任务**：用户指出 `D:\桌宠` 是 opencode 在本仓库副本上做的改动（"主要落地与回位"），要求对比、打分、把不合适的部分修改后合并。

**对比方法（关键）**：两副本同 HEAD（`50c45d2`），逐文件 diff C 盘工作树 vs D 盘工作树。结果——grounding.rs / proactive.rs / system.txt / reflection.rs 四文件**逐字节相同**（证明 opencode 完整继承了我 07-31 的工作，base 一致）。真正增量仅在 4 块：① B2 物理（gravity.ts 新 + spatial.ts + App.tsx）② B1 Consolidation 反向更新 Facts（consolidation.rs + facts.rs + harness）③ App.tsx `data-behavior` 插桩 ④ HANDOFF。故合并 = 纯增量复制，零冲突。

**opencode 增量打分**：

| 增量 | 分 | 评 |
|---|---|---|
| B1 Consolidation→Facts backfill | **8.5** | 架构契合极好（#1 LLM 只提议 JSON、Rust 验证 category 白名单+confidence clamp 写库 / #8 低频可接受 / #11 source_episode 可追溯+失败 log）。prompt 质量高（明确"不推断"、中文 key 利于合并、confidence 分档）。冲突检测=expire_old+dedup_insert 是 V2 合理 MVP。失败隔离（backfill 失败只 warn 不阻断已成功的压缩）。单测全面（parse/write/dedup/revive）。max_tokens 4096（踩坑#3 已规避）。**唯一点**：每批 consolidation 多 1 次 LLM 调用（prompt 复用 summary，可接受设计权衡）。 |
| facts.rs dedup revive（B1 配套） | **8.5** | 修真实边缘 bug：过期同值事实"复活"撞 UNIQUE(category,key,value) → 改 UPDATE 复活原行保 mention 历史 + 测试。 |
| B2 物理：bug 修复 + ref 重构 | **9** | **重要发现**：原生 `startDragging` 吞掉 webview 所有鼠标事件，旧 `onUp`(mouseup) 是**死代码**——拖拽后 petPos 从未同步、land 音效从未真正播过（HANDOFF 07-31"Foley 落地 onUp"记录不准）。改用 `onMoved` 事件同步位置 + rAF 静止检测（300ms 静止+半空→落体）。petPos useState→petPosRef 重构正确解决 state 依赖致 rAF 每帧重建/dt 抖动/视觉卡顿。onMoved 节流 100ms 修 refreshOrigin IPC 洪水。floorY=workArea 底部（任务栏上沿）。经 CDP+Win32 真实鼠标注入实测验证轨迹。 |
| B2 物理：gravity.ts + 1/3 飘落 | **7.5** | 纯函数清晰、常量集中、#1/#5 契合。**待确认**：① gravity.ts 宣称"任务栏弹跳"（BOUNCE_DAMPING/BOUNCE_STOP_VY），但 App.tsx 的 `fallLimitBottomRef`（用户 08-01 偏好"飘落"）让她只落 1/3 距离就 grounded → 反弹分支永不触发 = **bounce 是配置下死代码**（逻辑健全但被覆盖）。② `GRAVITY = 1200/9` 魔法算式可读性弱（注释有 sqrt 推导，但不如直接 133）。③ `stepGravity` 原地 mutate g（违反全局 immutability 规范，但物理循环务实，可接受）。 |
| spatial.ts RETURN_DELAY 30→900 | **10** | 1 行，用户明确要求（拖走 15min 才回巢，配合飘落悬停）。 |
| A4/A5 实跑方法论 | **9** | `Date.prototype.getHours` 重写模拟 DeepNight（绕开改系统时间，无 UAC、秒级切换）+ CDP 自动化（WebView2 `--remote-debugging-port` + Node 原生 WebSocket）+ `data-behavior` DOM 插桩——**方法论资产**，后续所有"需改系统时间/需 GUI 实跑"的验证都能复用。 |
| HANDOFF 更新 | 8.5 | 记录详实，但视角是 opencode（合并时已改成"合并自副本"）。 |

**总评 ~8.3/10**：核心价值高（B2 修了一个隐蔽的死代码 bug + B1 架构干净），缺陷集中在 B2 的 1/3 配置遗留（bounce 死代码、魔法常量）——非阻断，记入 follow-up。

**关键技术吸收（写入避免重复/复用）**：
1. **`startDragging` 吞 webview 鼠标事件**（B2 踩坑）：Tauri 原生 `win.startDragging()` 把拖拽交给 OS 合成器，期间 webview 收不到任何 mouseup/mousemove → 拖拽结束**不能用** mouseup 检测，必须用 `win.onMoved()` + 静止期。旧 onUp 路径全是死代码。
2. **`Date.prototype.getHours` 重写**（A4/A5 验证法）：模拟时段不需改系统时间。`circadian.ts` 是唯一 getHours 调用点，重写后 ~16ms 生效；`Date.now()` 不受影响（入睡计时仍走真实时钟）。验证后恢复原函数。
3. **CDP 自动化**：`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 启动 release exe → Node 22 原生 WebSocket 查 DOM/截图/派发点击。脚本断线必须 `ws.onclose/onerror→exit(2)` 否则挂起。
4. **物理循环必须 ref 驱动**：rAF 回调若依赖 React state（petPos），每次 setState 触发 effect 重建 → lastTime 重置 → dt 毛刺 → 卡顿。位置一律走 ref。

**合并执行**：纯增量复制 6 文件（gravity.ts/spatial.ts/facts.rs/consolidation.rs/App.tsx + consolidation_harness.rs）D→C，C/D 逐字节校验一致。清理 harness `ep_before` 死变量（opencode 调试残留）。`cargo check --tests` ✅ / `cargo test --lib` **216 passed** / `tsc` ✅。

**待确认（非阻断，入 follow-up）**：
- **bounce 死代码**：gravity.ts 的弹跳逻辑在当前 1/3-飘落配置下永不触发。若用户确认"只要飘落不要弹跳"，可删 BOUNCE_* 常量+反弹分支；若两者都要，需让 1/3 规则可配置或仅对贴近 floor 的释放生效。
- **B1 多 1 次 LLM**：已接受（#8 低频）。若要省，可让 consolidation 那次 LLM 同时输出 facts（prompt 合并），省一次往返。
- **A6 弃测**：emotionBridge 连续表情 opencode 标"用户弃测（过渡角色）"——等最终 Live2D 角色交付后补测。

---

## §最近一轮 (2026-08-03 续②)：Liri 角色方向 + 人格落进 system prompt

**起因**：用户指 `C:\Users\SunJialei\Desktop\形象设计\` 三份文档（设计规范/动画设计/制作规范）= Liri 角色 + Spine 制作圣经，要求读并记录要点。**重大方向确认**：最终角色 = 璃 Liri（小狐灵，6.2 头身少女），**动画走 Spine + PixiJS Runtime，不用 Live2D**（制作圣经明示"Spine 更适合程序控制"——Liri 需状态机/Emotion参数/Memory触发/动态行为）。→ 当前 `Live2DCanvas.tsx`（Live2D Haru + f00-f05 + emotionDriver）是**占位待迁移**；FSM/emotionDriver/behavior→参数映射/circadian/microBehavior 技术无关、迁移时沿用，只换渲染层。这也解释了 B3 验收时"没有 Zz"——Zz 仅在 SVG PetCharacter，Live2D 无；且 Live2D 本就临时，不宜深究其视觉。

**记忆**：要点写入跨会话记忆 `liri-character-spine-direction`（方向+迁移映射）+ `liri-design-bible`（人格35/20/20/15/5/5、耳尾情绪映射、5层优先级、MVP 27 动画、Spine PSD/骨骼/Mesh/Physics/命名/验收）。

**落地两件**：
1. **文档入仓**：三份拷至 `docs/specs/liri/`（原在桌面易丢）。+CLAUDE.md 文档导航加指针。
2. **人格落 system prompt**（用户要"把人格配比落进 system prompt / emotion 系统"）：
   - `system.txt` 身份行 + `[Core Personality - permanent]` 块：通用占位（"Gentle, patient, playful, curious, slightly mischievous"）→ Liri（璃/小狐灵身份 + 狐狸观察者本性 + 6 维配比散文化 + NOT 话痨/卖萌/依赖/永远积极 + "表达服务人格"）。**14 条规则全保留**（尤其 rule8 `严禁编造`——grounding 测试硬断言）。
   - `firstrun.rs::seed_persona`：core 维度（gentle/patient/curious/playful/caring）→ Liri 6 维中文 key（温柔/好奇/聪慧/安静/调皮/神秘）。**confidence=确信度（design-seeded 高），非权重%**——因 grounding 渲染 [Persona] 只取 trait_key、confidence 不进 prompt，故**配比%只能放 system.txt 散文**，种子只给维度标签。
   - **emotion 系统不动**：EmotionState 是 homeostatic 中性模型，personality % 无自然映射；人格通过 prompt 散文驱动情绪"表达风格"。#9 分层复杂度——不过度接线。

**架构契合**：#1（人格由 Rust 写进 prompt，LLM 只配音）/ #3（Liri 原则②不假记忆=既有 rule8）/ #6（core 人格永久，[Persona] 动态层叠）/ #11（system.txt 可追溯）。**未改签名**（converse/build_system_prompt 等零改动，规避踩坑#4）。

**验证**：`cargo test --lib` **216 passed** ✅（grounding `test_system_prompt_contains_chinese_grounding_ban` 断言 `严禁编造` 仍在、`test_system_prompt_contains_memories` 的 "gentle" 经 system.txt「温柔 (gentle)」+ mock 双保险）。system.txt 是 `include_str!` 编译进二进制 → **待 rebuild 进 dev/release 生效**（tauri dev 监听 src-tauri，改 .txt cargo 会因 include_str! 依赖追踪重编）。

**待办 / Scope 边界**：① **当前用户库 core persona_traits 仍是旧种子**（gentle/patient/curious/playful/caring，firstrun 已跑过、幂等不重种）——与 Liri 不冲突（兼容维度）但不一致；可选 reseed：`DELETE FROM persona_traits WHERE trait_type='core';` 后重启 → firstrun 用 Liri 维度重种。② seed_persona 改动**仅对新装生效**。③ 动画层 Spine 迁移是大方向（待 Liri.spine 资产 + PixiJS runtime，替换 Live2DCanvas；emotionDriver 参数映射需对接 Spine 骨骼/网格而非 Live2D param）。④ Liri 性格配比是否要进一步影响 emotion homeostasis 的反应曲线——留 follow-up，当前 prompt 驱动足够。

---

## §最近一轮 (2026-08-03 续)：B3 Sleeping 配套收尾（纯前端）

**任务**：用户"继续完成开发任务"。HANDOFF §下一步总清单 ②待开发 下一项 = B3（Sleeping 配套收尾，小项 ×3）。前序 A4（Sleeping 入睡/唤醒机制，07-31 build 过/待实跑）已让 Sleeping 能自动触发+交互唤醒，但三处配套缺口：① 睡着仍冒「早点睡」nudge（梦话）② sleep 音效素材早就在 `public/audio/voice/sleep.mp3` 但 soundManager 没接 ③ LateNight 行为需确认。

**实现**（2 文件 ~10 行，原则 #1/#5/#6/#10/#11）：
- **① 睡着抑制 nudge**（`App.tsx` nudge effect，:682 一带）：`setInterval` 回调在 `if (awayMode) return;` 后加 `if (fsmRef.current?.state === BehaviorState.Sleeping) return;`。一处守卫同时挡住 DeepNight（早点睡）+ LateNight（还不睡呀…）两分支。**fsmRef 是 ref、effect deps `[showBubble, awayMode]` 不含它 → 无 stale-closure，每次 tick 读最新 state**。睡着是安静态（#10），不该冒泡说话。
- **② 接 sleep 音效**（`soundManager.ts` + `App.tsx`）：
  - `soundManager.ts`：`AssetKey` 加 `"sleep"`、`ASSET_PATH` 加 `"/audio/voice/sleep.mp3"`（素材 07-29 就在，只是没接线）、新增 `sleep()` 公开方法——**刻意 mirroring `greet()`**：两者都是**一次性状态进入 cue**（启动 hi / 入睡 sleep），不是高频交互音，所以**不走 `TRIGGERS` 加权随机表**，直接 `playSample`。区别 greet：sleep 无需 `greeted`/`greetArmed` one-shot flag + autoplay deferral——入睡必在 DeepNight + 10min 无交互后，AudioContext 早被用户之前的交互解锁（`ensureCtx` 的 resume 兜底也够），调用点（auto-sleep guard）天然保证每次入睡只调一次。mute 经 `ensureCtx` 返回 null 尊重（#6）。
  - `App.tsx`：auto-sleep 分支 `fsm.forceState(BehaviorState.Sleeping);` 后加 `sound.sleep();`。该分支的进入条件含 `fsm.state !== Sleeping`，故**只在她从清醒→入睡的那一 tick 触发一次**（已成 Sleeping 后条件 false，不会每 2.5s tick 重响）。
- **③ LateNight 不入睡只 yawn**（**已满足，零改动**）：auto-sleep 唯一触发点 `App.tsx:239` 的条件 `circadianRef.current.period === TimeOfDay.DeepNight` 本就把 LateNight(22-2) 排除在外；LateNight 的 sleepiness=0.6 经 Tier3 #7 的 `sleepy` 权重倍数让 yawn 占比上升（HANDOFF §历史 2026-07-29 数学验证）。即 LateNight 现状正是"不入睡只 yawn"，无需改。

**架构契合**：#1 sleep 音/抑制都是纯规则无 LLM / #5 Body 层音效断网照响 / #6 mute 时 sleep() no-op / #10 睡着安静（不梦话）+ 入睡有轻 cue = 生命感 / #11 sleep() 方法 JSDoc + ASSET_PATH 集中可追溯。

**验证**（build 过 ✅ / 待实跑）：`tsc --noEmit` ✅（0 error）/ `npm run build` ✅（482 modules，2.16s）。**纯前端，无 Rust 改动**（cargo 不受影响，不必重跑 --lib）。**待实跑**：改系统时间到 2-6 点（DeepNight）+ 不交互等 10min → 观察入睡（闭眼慢呼吸+Zz）+ 听 sleep 音 + 确认不冒「早点睡」nudge；戳/摸/对话 → 即时唤醒（唤醒不响 sleep 音，符合直觉）。复用 A4/A5 方法论：`Date.prototype.getHours` 重写模拟 DeepNight 可秒级切换（无需真改系统时间）。

**Scope 边界**（follow-up，避免过度）：① sleep 音只入睡响、唤醒不响（现 markInteraction 唤醒无音——可接受，"醒来安静"也自然；若要 wake 音再加素材+方法）② Sleeping 时其他气泡（proactive/welcome-back 后端 emit）未守卫——但后端 proactive `trigger_proactive` 有 `MIN_BUBBLE_INTERVAL_SECS` + closeness 门，且 Sleeping 时用户大概率 awayMode/无交互，撞概率低；若实跑发现睡着仍冒后端气泡，再在 `welcome-back`/`proactive-prompt` listener 加 Sleeping 守卫（前端 fsmRef 可读）。

---

## §最近一轮 (2026-07-31 19:10)：主动开口幻觉 → grounding A 档收紧

**症状**：用户报桌宠主动开口说「你那个小睡衣项目怎么样了？」——把一件从未发生的事当真实记忆来追问（闭环3「她记得我」的反面：记得假事）。

**排查（三层 0 命中）**：代码/prompts 无"睡衣"；全数据库表（conversations 0、episodes 13、facts 35、reflections 6、internal_thoughts 12、pending 2）无"睡衣"。→ 纯幻觉。

**根因（纠正一处误判）**：主动开口路径**并非没有 grounding 约束**——`proactive`/`welcome_back` 经 `budget::allocate_and_compress` 注入了 system.txt 规则 8 + `MEMORY_CONSTRAINT`。失效在于：① **任务压力压过禁令**：规则 12 + proactive user prompt 强压"必须带出一个记忆并提问"，当检索到的真实记忆（如 `[work/current_project] desktop pet project`）不够具体/不好聊时，LLM 为完成"主动关心"而编造细节 ② **输出端零兜底**：`check_groundedness` 只挂 `converse`（且只 `warn` 不阻断），`proactive`/`welcome_back` **完全没挂**——编造的输出直达用户、连日志都没有 ③ **约束力打折**：规则 8 / `MEMORY_CONSTRAINT` 是英文、生成是中文，跨语言约束力下降。

**A 档修复（用户选 A：治本-prompt 收紧，3 处）**：
- `system.txt` 规则 8：英文禁令后追加**中文强禁令**（不得把记忆改写成别的项目名/事件、不得虚构、没合适话题就只简单招呼、不硬找话题不猜）。
- `proactive.rs::generate` user prompt：加运行时**锚点围栏**（只能围绕锚点原意、不得换成别的项目/编造），顺带修掉指向不存在的「规则 8/8a/8b」stale 引用 → 「尤其规则 8」。
- `proactive.rs::generate_welcome_back` anchor_clause：加同样围栏。
- `grounding.rs`：加 `test_system_prompt_contains_chinese_grounding_ban` 断言（防回归）。

**验证**：`cargo test --lib` **208 passed**（+1）✅ / release exe rebuild 19:10 ✅。**残余风险**：prompt 是软约束、无运行时阻断（那是 B 档）；若实跑仍偶发幻觉 → 升级 A+B（`check_groundedness` 加中文 claim + 主动开口输出端阻断）。

**附带发现（未修，入 backlog）**：① `conversations` 表生产路径 0 写入（`conversations::insert` 仅测试调用）= 死表，导致本次无法回溯她原话（#11 可追溯受损，C 档）② `check_groundedness` claim_patterns 全英文、中文漏检（B 档）。

---

## §最近一轮 (2026-07-31 续)：气泡 release rebuild + consolidation 修复 + Reflection 触发器 + Sleeping 入睡

**起因**：用户报"启动打招呼气泡位移没修好"。诊断：release exe（07-29 16:58）落后 3329d85（07-31 CSS translate 居中修复）2 天——上一轮"气泡实跑通过"仅 dev HMR 验证（自动热更 CSS），release exe 从未 rebuild，快捷方式跑老代码（`transform: translateX(-50%)` 被 bubble-* 动画 transform 覆盖→偏右跳变）。CSS 层面 3329d85 正确（`translate` 属性独立于 `transform`、无冲突声明），**纯打包问题，代码不动**。taskkill（PID 108248，踩坑#6 释放 exe 锁）→ `npx tauri build --no-bundle` → exe 07-31 12:10。用户双击快捷方式实跑确认居中、不再跳变（Foley 接线/circadian/converse thought 5 commit 一并进 release）。

**顺势修 consolidation llm-empty bug（踩坑#3 复发）**：`consolidation.rs:87` 生成任务（压缩总结）传 `max_tokens=2048`——reflection/extractor 同为生成都 4096，gate/correction 分类才 2048，consolidation 是**唯一**生成任务用 2048 的。DeepSeek-v4 reasoning 独占预算→content 空→静默写空白 episode（consolidation 返纯文本不解析 JSON，无 reflection 的 parse-fail 兜底）。修：① 2048→4096 ② 空 content 防御（warn + return Ok(0)，不写垃圾、下轮重试）。`cargo test --lib` **199 passed**。**待实跑**：触发需 episodes≥100，日常不易达；Rust 改动待下次 rebuild 进 release。

**Reflection 触发器（Tier2 #5，单测过 ✅）**：`ReflectionTrigger` 枚举早预留 TurnThreshold/MajorEvent 变体但只 Daily 被触发。补事件驱动触发：① TurnThreshold——自上次 reflection 起 ≥30 条 conversation episode ② MajorEvent——自上次 reflection 起有 importance>0.85 的 episode。两者共用独立 1h 冷却（Daily 仍 20h），`maybe_run_if_due` 优先级 Daily→MajorEvent→TurnThreshold 取首个命中。轮次源用 episodes（`conversations` 表零调用是死表、`total_conversations` 无基准），只数 source_type='conversation' 的记忆（gate 拦掉的不算）。抽 `last_reflection_at` helper 复用、重构 `should_run_reflection`。**不改签名**（maybe_run_if_due/run_reflection/ReflectionTrigger 签名都不变，slow_tick/commands 零改动，规避踩坑#4）/ **#8 成本**：事件驱动最多 1次/h + Daily 1次/天。`cargo test --lib` **207 passed**（+8：5 turn_threshold + 3 major_event，覆盖不足/达阈值/非 conversation/冷却内/无历史 + 高/低 importance）/ 全 harness 编译 ✅。**待实跑**：需攒 30 条对话记忆或高 importance 事件；Rust 改动待下次 rebuild 进 release。

**Sleeping 入睡机制（Tier3 续，build 过 ✅ / 待实跑）**：`Sleeping` 状态渲染全套早就绪（Live2D f05 / behaviorDriver 慢呼吸 / PetCharacter sleeping+Zz / styles sleeping 动画）但**从未被自动触发**——缺触发+唤醒。实现（纯前端 App.tsx）：① 入睡——FSM tick 定时器(2.5s)检查 `circadian.period===DeepNight(2-6) && state!==Sleeping && !isThinking && !Talking && Date.now()-lastInteractionRef>SLEEP_AFTER_IDLE_MS(10min)` → forceState(Sleeping) ② 唤醒——markInteraction()（摸/戳/拖/对话/双击 5 处入口）：刷新 lastInteractionRef + 若在睡 forceState(Idle)（forceState 绕过 transition 优先级锁——Sleeping 不可中断态只有 forceState 能出）③ 清醒冷却——唤醒刷新 lastInteraction 天然让 10min 内不再入睡。**不改 fsm/circadian/behaviorDriver**（渲染早就绪，只补 App.tsx 触发+唤醒）/ **#10 生命感**：她有睡眠周期 / **#1 纯规则无 LLM** / **#5 Body 层独立**。`tsc --noEmit` ✅ / `npm run build` ✅（1.96s）。**待实跑**：改系统时间到 2-6 点（DeepNight）+ 不交互等 10min → 观察入睡（闭眼慢呼吸+Zz）；戳/摸/对话 → 即时唤醒。follow-up：Sleeping 时抑制 DeepNight nudge 气泡（现睡着仍冒"早点睡"，像梦话）；LateNight(22-2) 不入睡只 yawn（现有）；Sleeping 音效（sleep 素材预留未接）。

**踩坑（写入避免重复）**：**dev HMR 验证 ≠ release exe 已含修复**——前端/CSS 改动 dev 下热更"看着修好"，但桌面快捷方式（release exe）不自动更新，必须 `npx tauri build --no-bundle`。涉及前端/CSS 的"实跑通过"必须在 release exe 上确认（本次气泡 + 上次 Foley 同一坑）。诊断 release 行为先比对 exe LastWriteTime vs commit 时间。

---

## §历史 (2026-07-31 早些)：Foley 接线补全 + 频率调整 + 气泡位移（实跑通过 ✅）

**起因**：用户实跑发现"音效完全没声"+"气泡启动偏右再跳变"+"戳身没声"。诊断：① Foley 的 **App.tsx 接线从未落地**（soundManager 孤立单例，无 import/调用；ContextMenu 要的 soundMuted/onToggleSound props 也没传，TS 报错被 vite esbuild 静默忽略）→ 旧 §历史"Foley 实跑通过 9 触发点"记录错误 ② 气泡 `bubble-*` 动画 keyframes 覆盖 transform 且不含 translateX(-50%)，动画期间丢居中→偏右，结束跳回居中 ③ 戳身无声 = HMR 后 sound 单例重建 buffers 空 + mount effect preload 不重跑 + poke cooldown 5000ms，首次现场加载无声后连戳全被 cooldown 拦。

**实现**（4 文件 + 素材，原则 #6/#10/#11）：
- `App.tsx`：补全 sound 接线——`import {sound, INTIMATE_THRESHOLD}` + `soundMuted` state + mount effect（`sound.preload()`+`sound.greet()`）+ mute sync + 8 触发点（摸头按 closeness 分档/戳按 pokeCount 分档 poke1-3/drag onMove/land onUp/send/dblclick）+ `handleContextMenu` 加 `sound.play("menu")` + ContextMenu 传 soundMuted/onToggleSound。
- `soundManager.ts`：权重调整（UI 类 dblclick/menu/send 必响；活物音 pet/poke/drag/land 出声概率 +15-20%，仍保留随机）+ 新增 `menu` trigger + poke cooldown 5000→2000（修戳身）。
- `ContextMenu.tsx`：soundMuted/onToggleSound props（此前 working tree 已改但 App.tsx 未接）。
- `styles.css`：气泡居中改用 `translate: -50% 0` 属性（独立于 transform），动画 transform 不再覆盖居中——`.chat-bubble`/`.bubble-pet`/`.hidden` 三处。
- `public/audio/`：11 素材（10 接入 + sleep 预留）。

**验证（实跑通过 ✅ 2026-07-31）**：tsc ✅ / dev HMR ✅ / 音频文件 fetch 全 200 / 用户确认：摸头/戳/拖动/落地/双击/右键/发送均有声 + 频率手感 OK + 戳身 cooldown 降后响 + 气泡不再偏右跳变。

**踩坑（写入避免重复）**：① **vite dev 用 esbuild 不做类型检查**——TS 错误（如缺 props）不阻断运行，运行时静默失败。改前端接线后必须 `tsc --noEmit` 验证，不能只看 dev 能跑 ② **HMR 后模块单例重建 + mount effect 不重跑**——sound 单例 buffers 清空、preload 不重新触发，首次交互现场加载。dev 下改音效代码后 F5 刷新；release 无 HMR 不受影响 ③ **CSS animation transform 覆盖元素 transform**——居中若靠 `transform: translateX(-50%)`，动画 keyframes 的 transform 会盖掉导致跳变。解法：居中用独立 `translate` 属性。

---

## §历史 (2026-07-31)：converse 注入 surfaced thought（Tier2 #4，build 过 / 待实跑）

**起因**：用户"按执行计划进度继续开发"。HANDOFF §下一步候选 Tier2 #1（北极星 #10 + Soul 深度）。审计发现 `surface_thoughts`（monologue.rs:18）只在 `generate_welcome_back`（proactive.rs:286）消费——**只有用户离开回来才浮现昨晚念头；正常对话从不带出**，thought 若用户没离开/没触发 welcome-back 就永远积压。

**实现**（1 文件 ~20 行，纯内联，原则 #1/#8/#10/#11）：
- `converse.rs`：Step 8 reminder bridge 之后、user message 之前加 `thought_clause`——调 `surface_thoughts(db)` 取首个 next_interaction thought，拼 system message 注入。措辞**克制**（区别 welcome-back 的"招呼里带出"）：「话题自然关联才轻带，无关别强提，正常聊」——避免她每轮翻昨晚念头。Err 降级空串（不阻断对话）。
- **不改签名**（踩坑 #4 规避，converse 内部加逻辑，send_message + 全 harness 零改动）/ **不增 LLM 调用**（#8 复用同一次 converse 的 LLM turn）/ **消费性**（surface_thoughts mark surfaced；welcome-back 与 converse 谁先触发谁消费，thought 只浮现一次，自洽）/ **#11 可追溯**（`[converse] surfaced thought` log）。

**架构契合**：#1 Rust 拼 prompt（含克制措辞），LLM 只配音 / #8 零额外 LLM / #10 正常对话也"记得昨晚念头"=生命感 / #11 surfaced log 可追溯。

**验证（build 过 ✅ / 待实跑）**：`cargo test --lib` **199 passed**（0 failed）/ `cargo check --tests` 全 harness 编译 ✅（17.99s）。**待实跑**：`npm run tauri dev`，等 reflection 产 thought（或手动插一条 next_interaction thought 到 DB）→ 对话观察她自然带出。

**Scope 边界**（follow-up，避免过度）：① thought 浮现无频率门控（每次对话 surface_thoughts 取 1）——现 thought 产出稀疏（reflection 每日≤1），不构成问题，多了再加"每 N 轮最多 1 次"门控 ② converse 无 thought 专用 harness（需完整 AppState 构造，重），靠 surface_thoughts 既有单测 + 实跑覆盖 ③ ConversationResult 未加 has_thought 字段（log 已够 #11，减少改动面）。

---

## §历史 (2026-07-29)：Foley 音效 — 真实素材接入（实跑通过 ✅，Tier1 #3 完成）

**起因**：用户提供 11 个真实 Foley 素材（桌面/新建文件夹，中文 mp3）。此前合成柔软占位在跑（见本节末历史）。

**素材映射（原名 → public/audio 语义名 → 角色）**：
- voice/surprise-soft（ow：轻微意外/疑惑）｜voice/startle-short（啊1 短促：戳受惊）｜voice/soft-ah（啊 稍长：摸头满足）
- voice/annoyed（生气：连戳3+）｜voice/laugh（笑：开心/亲近摸头）
- foley/cloth（布料声）｜foley/land（落地声）｜foley/lift（跳：抓起）｜ui/send（UI音效：发送）
- voice/greeting（hi：启动打招呼）｜voice/sleep（睡觉声：**仍预留**，Sleeping 入睡机制未做）

**核心设计（#10 宁少勿突兀 + #11 集中可调）**：
- `soundManager.ts` 重写为**纯 wav 加载**（fetch public/audio + decodeAudioData + AudioBufferSourceNode），**移除原 Web Audio 合成**。
- **权重随机 + 静默优先**：每 trigger 最大权重是"静默"（摸头陌生 静默70/布料20/ow10；亲近 静默45/啊25/笑15/布料15）。出声=惊喜非常态。
- **cooldown 出声/静默都计时**：rapid tap 不能叠声。摸头3s / 戳5s / 拖动0.8s / 落地0.6s / 发送0.4s / 双击1s。
- **亲密度分档**：摸头按 `closenessRef`（前端现成，15s 拉）≥ `INTIMATE_THRESHOLD`(40) 选 pet-intimate（啊/笑）vs pet-stranger（布料/ow）。声音=关系成长。
- **戳递进**：复用已有 `pokeCountRef` n=1/2/3+，音效跟随（poke1 ow/啊1 → poke2 啊1 → poke3 生气）。
- `preload()`：App 启动 mount effect 预热所有 buffer，首交无 fetch 延迟。

**触发点（App.tsx 9 处）**：摸头(773 按 closeness 分档) / 戳(801 按 n 分档) / 拖动(596) / 落地(609) / 发送(649 click→send) / 双击开对话(888 新接 dblclick) / **启动招呼（preload effect 调 `sound.greet()`）**。drag/land id 不变。ContextMenu 静音项不变（#6）。

**启动招呼 `greet()`（autoplay 处理）**：启动无用户手势 → AudioContext `suspended` 直接播不出。`greet()` 启动即尝试播 hi；若 ctx 挂起，挂一次性 `pointerdown` listener，首次互动 resume ctx 后补播（保证总能听到招呼）。`greeted`/`greetArmed` 双 flag 防 StrictMode 双 mount 重复。抽出 `playSample(key,ctx)` 给 play/greet 共用。**素材现状**：11 个接 10（含 greeting），仅 sleep 未接（Sleeping 未做）。

**验证（实跑通过 ✅ 2026-07-29）**：tsc ✅ / build ✅（2.52s）/ dist/audio 11 文件全打包 ✅。**CSP 排查**：`connect-src 'self'` 含同源 → fetch public 资源 release 不拦（同 Live2DCanvas `/live2d/` 模式），无 release-only 坑。`npm run tauri dev` 实跑：用户确认 9 触发点 + 启动 hi + 亲密度档效果不错。

**踩坑**：`import.meta.env.BASE_URL` 无类型（项目 tsconfig 无 vite/client），改硬编码 `/audio/`（同 Live2DCanvas 既有模式）。

**follow-up**：hi/sleeping 预留未接；走路脚步声 loop、Sleeping 入睡机制同期待办；权重/cooldown/阈值全集中 `soundManager.ts` 的 `TRIGGERS`，实跑后按手感调。

---

### 历史（已被真实素材取代）：合成柔软占位

**起因**：用户选 §下一步 Tier1 #3（北极星 #10）。审计 P11 标记"Foley 音效未做"，此前零音频代码。

**素材试错（关键教训）**：
1. 下载 CC0：从 [Kenney Interface Sounds](https://github.com/Calinou/kenney-interface-sounds)（GitHub raw 可 curl，避 Freesound 需登录）挑 5 个 UI 音（pluck/tick/maximize/drop/select）→ **实跑用户反馈"太难听、都很尖锐"**。根因：UI 音为界面反馈设计，高频瞬态、清脆冰冷，放柔软桌宠上刺耳。**下载素材风格不可控、无法预听筛选**——这正是当初推荐合成的核心理由。
2. 改 Web Audio 合成：sine（几乎无谐波）+ 低频基音 + 低通滤波 + 慢 attack（无瞬态）+ 长 release = 物理上不可能尖锐。每音参数集中可调（`SPECS`）。
3. 用户决定**自找真实 Foley 素材** → 本项 ⏸️ 列为待办。

**已落地（保留，不再折腾）**：
- `src/audio/soundManager.ts`：纯 Web Audio 合成版（`SPECS` 5 音参数 + `synth()` + `muted`/`toggleMuted`）。**已删 `public/sounds` wav**（不再用）。结构预留：未来加 wav 只需 `play` 里加"wav 优先 fallback 合成"分支，`synth` 已独立
- `App.tsx`：5 触发点接线（`handleHeadClick`→pet / `handleBodyClick`→poke / `handleDragStart` onMove→drag + onUp→land / `handleSend`→click）+ `soundMuted` state + `toggleSound` + ContextMenu 传参
- `ContextMenu.tsx`："静音/开启声音"项（#6）

**架构契合**：#5 Body 层零 LLM 依赖（断网照响）/ #6 右键静音，muted 时 no-op / #8 本地 DSP 零成本 / #10 交互有声=活物感。

**验证**：`tsc` ✅ / `build` ✅（481 modules）/ release exe 重建（PID 98956）。合成柔软占位在跑。

**⏸️ 待办（用户自找素材后接入）**：用户提供 pet/poke/drag/land/click 真实 Foley wav → soundManager 加 wav 加载分支（play 优先 wav、fallback 合成）+ drop `public/sounds/` + 重建。走路脚步声 loop（需 isWalking 周期触发）同期待办。

## §历史 (2026-07-29)：circadian 接入微行为（Tier3 #7）

**起因**：用户选 §下一步 Tier3 #7（北极星 #10 生命感）。审计 P10 标记"circadian 未接入微行为选择"——`getCircadianState()` 每帧算出 `sleepiness`（Morning 0.1 / DeepNight 0.9）但 `fsm.tick(App.tsx:199)` 根本没接收；`microBehavior` 的 `IDLE_BEHAVIORS` 权重写死、与时段无关 → 深夜桌宠和白天一样精神。顺带发现 `Sleeping` 状态渲染/参数全就绪（behaviorDriver 闭眼慢呼吸 / Live2D f05 / PetCharacter Zz）但**从未被自动触发**（无人 forceState，微行为池无 sleep 项）。

**实现**（3 文件，纯前端零后端，原则 #1/#9/#10/#11）：
- `microBehavior.ts`：`IdleBehavior` 加可选 `sleepy?: number`（困倦权重倍数，JSDoc 固化公式语义 + 数学预期）；`IDLE_BEHAVIORS` 8 项填值；`PickOptions` 加 `sleepiness`；权重循环加 `w *= 1 + (sleepy-1)*sleepiness`（`Math.max(0.01,w)` 兜底非负）
- `fsm.ts`：`tick` 签名加 `sleepiness: number`（closeness 后 now 前），透传进 `pickBehavior` 回调 opts
- `App.tsx`：`fsm.tick(...)` 调用喂入 `circadianRef.current.sleepiness`（ref 每帧已在 :542 更新）

**架构契合**：#1 纯规则无 LLM / #9 MVP 单标量 sleepiness 线性插值刚好够用 / #10 深夜她真的犯困=生命感 / #11 sleepy 字段 + 公式 JSDoc 可追溯。

**验证（build 过 ✅ / 待实跑）**：`tsc --noEmit` ✅ / `npm run build` ✅（480 modules）。**数学验证**（全池可见、closeness 足够）：白天 sleepiness=0.1 → yawn 占比 ~10.7%、look_around ~17.8%；深夜 sleepiness=0.9 → yawn ~32.2%（3×↑）、look_around ~6.5%（↓）。sleepiness=0 乘子=1 白天不变。**待实跑**：改系统时间到 2-6 点（DeepNight）观察 yawn 频率上升。

**Scope 边界**（follow-up，避免过度）：① `speedModifier`/`energyModifier` 未接动画速度/能量（circadian.ts 注释声称输出 speed，但 behaviorDriver 未消费）② **Sleeping 自动入睡/唤醒机制未做**——权重调制让深夜多 yawn，但不会真正 forceState(Sleeping)；Sleeping 是持续态（fsm tick 不自动退出），完整入睡需配"用户交互（戳/摸/对话）唤醒"机制，是更大设计 ③ idle_weights 仍硬编码（非 JSON 配置），可调但非数据驱动。

## §历史 (2026-07-29)：气泡生命力 — 打字节奏随情绪 + 无文字气泡（Tier1 #2）

**起因**：用户选 §下一步 Tier1 #2（北极星 #10 生命感，对话是核心交互）。审计发现气泡形态动画已有 5 种 keyframes（styles.css:445-501），但缺两块：①打字节奏固定 30ms 无论情绪 ②无文字气泡完全缺（#12 沉默表达未落地）。

**实现**（3 文件，纯前端零后端，原则 #1/#9/#10/#11/#12）：
- 新增 `src/animation/bubblePacing.ts`：`typewriterPacing(moodLabel)->{intervalMs,catchDiv,hesitate}` 纯函数，6 档映射（开心 22ms 快流畅 / 调皮 26ms / 平静 32ms baseline / 担心 42ms+20%停顿 / 难过 50ms+10% / 疲惫 55ms+15%喘），未知标签 fallback 平静
- `App.tsx` 流式打字机参数化：首 chunk 时按 `moodLabel` 取 pacing，interval/step/hesitate 全由 pacing 决定；hesitate 仅 `!streamEnded` 时生效（收尾必完成）。与气泡形态 class 用同一 moodLabel → 语气与表情一致
- `App.tsx` 无文字气泡两触发：①空回复 fallback「（……）」→ glyph「…」②`emotionTimer` idle 叹气（疲惫/难过 + 空闲 + 8% →「呼…」）。**stale-closure 守卫**：新增 `bubbleVisibleRef`/`isThinkingRef` + mirror effect（emotionTimer 是 setInterval，闭包捕获 stale state；ref-mirror 是现有 propsRef 模式），用刚获取的 `emo.mood_label`（实时）。已有 `onboardingActiveRef`/`awayMode` 守卫
- `styles.css`：`.bubble-glyph`（大字号 / 无尾巴 `::after{display:none}` / 淡 opacity:0.8 / `bubble-glyph-soft` 轻动画）

**架构契合**：#1 节奏纯规则不走 LLM / #9 MVP 6 档节奏+glyph 刚够用 / #10 说话语气+叹气=生命感 / #11 pacing 纯函数可测+JSDoc / #12 沉默省略号/叹气是表达。

**范围边界**（follow-up）：「害羞慢现」形态未做——后端 `label_for_mood_full`（emotion/state.rs:39）只产 6 标签无「害羞」，强加需改后端标签逻辑（破坏性）。叹气只接前端 emotionTimer（未接后端 life_loop 事件源）。

**验证（实跑通过 ✅）**：`tsc --noEmit` ✅ / `npm run build` ✅（480 modules）/ release exe 重建 / 用户实跑确认节奏差 + glyph + 叹气。

**后续修复（同日，`ebc1082`）：pacing 改用本轮输入关键词驱动。** 实跑发现「难过」与「讲故事」同速——根因 moodLabel 是**慢变量**双重滞后：①前端 `onChunk` 闭包 moodLabel 是 invoke 时捕获的旧值（stale closure）②后端 emotion 在 converse Step 12（LLM 之后）才更新，首 chunk 时后端 emotion 仍是旧的。改 `inferPacingMood(text, fallback)` 前端关键词启发式（难过/去世/哭→难过档），首 chunk 即时生效；moodLabel 降为无情绪词时的 fallback。零后端、即时。follow-up：后端流式前传统一情绪节奏信号，消除与 react.rs 的重复。

**release 重建踩坑（写入避免重复）**：`npx tauri build --no-bundle` 覆盖 exe 时，若桌宠**正在运行**，exe 文件被 Windows 锁 → cargo `failed to remove file ... 拒绝访问 (os error 5)`。**构建前必须 `taskkill //IM desktop-pet.exe //F`**，杀后 sleep ~3s 等 OS 释放句柄再 build，且**构建完成前不要重开快捷方式**（一次因构建中重开 → 新进程锁 exe → 失败）。

## §历史 (2026-07-28)：流式回复 — emit/listen → ipc::Channel（根因定位 + 修复）

**根因（对比定位，非瞎改）**：`download_embedding_model` 命令体内 `app.emit("download-progress")` **工作正常**（SettingsPanel listen 收到进度），因其 listener 在 `useEffect` 里**组件挂载时注册、长期存活**（命令返回后不立即 unlisten）→ 即使命令体内 emit 投递有延迟，listener 仍在 → 收到。而 chat-chunk 的 listener 在事件处理函数里**紧贴 invoke 注册**，`finally { unlisten() }` 在 invoke resolve 后**立即移除** → 命令体内 emit 的事件投递延迟到命令返回附近、被抢先 unlisten → **全部丢失** → `firstChunk` 恒 true → 走 `showBubble(res.reply)` 一次性 fallback。Tauri 官方文档明确：emit/listen「不适合低延迟/高吞吐」，`ipc::Channel` 才是命令体内流式正解（内部用于 download progress，命令期间实时投递、有序、不经全局事件总线）。上一轮 Channel「用法对但 onmessage 不触发」≈ 踩 v2 经典坑：后端 `on_event`（snake_case）要前端传 **camelCase `onEvent`**，传错则 Channel 不注入、onmessage 静默不触发。

**修复**（2 文件，原则 #1/#5/#10/#11）：
- `commands.rs`：`send_message` 去 `app: AppHandle`，加 `on_chunk: tauri::ipc::Channel<String>`；闭包 `move |delta| on_chunk.send(delta.to_string())`（Channel move 进 FnMut）。诊断：首 chunk 一次 `[chat-stream] first content chunk forwarded to channel` + send 失败 warn（不刷屏）。`send_message` 无 Rust 测试调用方，改签名不触发踩坑#4。
- `App.tsx`：去 `listen("chat-chunk")`/`unlisten`/try-finally；`const onChunk = new Channel<string>(); onChunk.onmessage = (delta)=>{...}; invoke("send_message",{text, onChunk})`（**camelCase 严格**）。打字机（`streamBufRef` buffer + 30ms interval reveal）保留——仍需绕开 React 同 tick 批处理。`firstChunk` fallback 逻辑保留（沉默/空回复走 showBubble）。

**验证（端到端确认 ✅）**：`npx tsc --noEmit` ✅ / `npm run build` ✅ / `cargo check` ✅。**① Channel 投递探针**（临时 `stream_test` 命令 send 10 chunk + 前端 `pong` 回调，验证后删）：`sent 0..9 ↔ received 0..9` 一一对应、同秒 → Channel 投递实时工作（隔离 LLM，不受 DeepSeek 速率干扰）。**② 真实 send_message**：dev.log 见 `[chat-stream] first content chunk forwarded to channel`（Step 9 + Channel.send 被调）+ chat_stream 7s 流式（长回复）。**③ 用户实跑确认**：长回复（"讲个稍微长一点的"）气泡逐字浮现 ✅。短回复（"你好啊"）content 仅 1-2 chunk 瞬间发完看不出逐字（DeepSeek v4 reasoning content 占比小，非 bug）。

**清理**：删 `tests/stream_debug.rs`、删 client.rs `[stream]` SSE 调试 log（start/DONE/ended，保留 `[llm-stream-empty]`/`[llm-stream] skip malformed` warn）、删 App.tsx 自动触发 effect、`[send-token]` 改为首 chunk + warn。

**踩坑总结（写入避免重复）**：
- **emit/listen 不适合命令体内流式**：命令体内 emit 的事件投递延迟到命令返回；若 listener 紧贴 invoke 生命周期（invoke 后立即 unlisten），全部丢失。对比：listener 长期存活（useEffect 注册）的命令体内 emit 能工作（download-progress）。**命令体内流式一律用 `ipc::Channel`**。
- **Tauri v2 Channel 参数 camelCase**：后端 `on_event` ↔ 前端 `onEvent`，传 snake_case 则 Channel 不注入、onmessage 不触发（静默）——上一轮 Channel 失败的疑似根因。
- **dev 实跑验证流式受 DeepSeek 速率制约**：reasoning 模型 gate/extractor 分类慢（接近 60s timeout），叠加 catch-up（系统睡眠触发 reflection+consolidate）多路并发易 rate-limit/超时，send_message 卡 Step 1 不到流式。验证流式前确保 DeepSeek 可用 + 无 catch-up 干扰。
- DeepSeek-v4-pro 是 reasoning 模型：短回复（"你好"）content 仅 1 个 chunk（一次性发完，看不出逐字），**测试流式必须用长回复**（如"讲个故事"→83 chunk）；reasoning_content 占 ~70% completion token（content 只剩 30%）
- **React StrictMode 双 mount 陷阱（诊断坑，曾误导整轮）**：dev 下 effect mount→cleanup→mount。自动触发 effect 若加 `flag` 守卫防重复，第一次 mount 设 flag+timer → cleanup 清 timer → 第二次 mount 见 flag 跳过 → **timer 永不执行**。上一轮自主 dev 实跑 dev.log 总无 `[stream]`/`[chat-stream]` 的真因即此（自动触发没跑，非后端问题）。教训：dev 一次性自动触发 effect **别加 flag 守卫**，让 React cleanup 天然去重（第二次 mount 的 timer 触发）。

## §历史 (2026-07-28)：情绪外显·连续表情插值（P10 emotionBridge）

**起因**：用户选 §下一步候选 #2（北极星 #10 生命感）。codegraph 调研发现 `emotion/state.rs` 注释明写"continuous vector for Live2D parameter interpolation"、设计文档（implementation-plan.md:809-810）点名 eye_open/mouth_form/motion_speed——但实际链路断裂：emotion 向量被 `label_for_mood` 压成 6 桶离散表情（f00-f05）经 `model.expression` 硬跳，energy/stress/loneliness 几乎不影响视觉。后端 `emotion-update` 事件早 emit 完整向量（loop_runner.rs:96-108），前端 App.tsx 监听却只取 mood_label。顺带发现离散映射疑似语义错位（`MOOD_EXPRESSION_NAME.happy="f00"`→F01.exp3 的 `ParamMouthForm=-1.76` 哭脸）。

**实现**（3 文件，纯前端零后端改动，原则 #1/#5/#9/#10/#11）：
- 新增 `src/animation/emotionDriver.ts`：`EmotionVector` 接口 + `DEFAULT_EMOTION`（对齐 Rust `EmotionState::default`）+ `getEmotionParams(e)` 纯函数（eye_open=energy/rest_need 疲惫半眯 / mouth_form=mood↗+stress↘ / eye_form=mood 笑眼 / brow=mood 下垂）+ `boostForTransientExpression(expr,base)`（f00→mood↑ / f04→stress↑mood↓，让对话强情绪也走连续路径）。参数 id 全部经 Haru expression 文件（F01.exp3）验证存在；brow 方向经 F01（难过 -0.56）确认。常量集中可调 + JSDoc。
- `Live2DCanvas.tsx`：props moodLabel→emotionVector；移除 `MOOD_EXPRESSION_NAME`/`moodToExpressionName` 离散映射 + 移除 mood→expression useEffect；`beforeModelUpdateFn` 里 `{...getEmotionParams(emoVector), ...getBehaviorParams(beh,elapsed)}`（emotion 基线叠 behavior overlay 之下，behavior 优先）；transient 用 boost 而非 `model.expression`（避免预设表情残留 brow/form 参数）。
- `App.tsx`：emotionVector state（DEFAULT_EMOTION）+ `toEmotionVector` 辅助；4 处填充（emotion-update 事件 / 5s emotionTimer / 启动 / send_message 后）；传 Live2DCanvas。moodLabel 保留给 `bubbleClassForMood`。rest_need 后端未暴露→0（energy 已覆盖疲惫，follow-up）。

**架构契合**：#1 参数纯函数无 LLM / #5 连续参数每帧写不依赖 LLM（断网仍跑）/ #9 MVP 4 维刚好够用（browForm/Angle 等细分残留留 follow-up）/ #10 表情连续流动=生命感 / #11 每维映射有 JSDoc + 参数来源可追溯。Talking 时 lipsync 管 `ParamMouthOpenY`、emotion 管 `ParamMouthForm`（嘴角随心情）不冲突。

**验证**：`npx tsc --noEmit` ✅ / `npm run build` ✅（479 modules）。**待实跑**：npm run tauri dev 观察情绪变化（戳身体 stress↑→嘴角下垂、心情好→微笑+笑眼、累→半眯）。

**Scope 边界**（follow-up）：rest_need 后端暴露（EmotionResponse + emit 加字段）/ `applyBehaviorToModel` 的 `model.expression`（Yawn→f05 等）仍走预设，其 browForm/Angle 细分参数残留未被 emotion 覆盖（主干 browLY/RY 已 cover）/ ParamAngleY 头部俯仰未映射（loneliness/energy 表达）。

## §历史 (2026-07-27)：Soul 慢循环闭环（Reflection 自动调度 + thought 融入回来招呼 + Consolidation 调度）

**起因**：用户"严格按 plan 执行"。对照 `implementation-plan.md` 验证标准逐项核对，P13 #1（Reflection 自动触发）/ #3（thought 融入下次交互）+ P15.1 慢循环清单**未达成**——Soul 三块逻辑（Reflection / Monologue / Consolidation）实现完整但**调度链断**：Reflection 仅启动 IPC 触发（常开不重启 → 永不跑）、thought 仅 `App.tsx` 启动独立气泡（没融入招呼）、Consolidation 未被 `slow_tick` 调用（只有 `lifecycle_cleanup` 被调）。

**实现**（4 文件，原则 #1/#5/#6/#8/#11）：
- `soul/reflection.rs`：新增同步纯函数 `should_run_reflection(db)->bool`（20h 冷却判断，可单测）+ async `maybe_run_if_due(db,llm)->Result<bool>`（调 `run_reflection(Daily)`，Err 传播）。+3 单测（无记录 true / 1h 前 false / 25h 前 true）
- `commands.rs`：`trigger_reflection_if_due` 瘦身为调 `maybe_run_if_due`（**IPC 签名不变**，规避踩坑 #4，前端 `App.tsx:387` 无感；Err 吞为 Ok(false) 保前端契约）
- `lifecycle/loop_runner.rs`：`slow_tick` 末尾加 `tauri::async_runtime::block_on` 块（slow_tick 在 std::thread，`block_on` 进入 runtime 不 panic；cadence 1h 阻塞可接受）→ `maybe_run_if_due`（Gap1）+ `consolidate`（Gap4）。`use crate::commands::AppState`。LLM 未配 → 跳过（#6 优雅退化）
- `pending/proactive.rs`：`generate_welcome_back` 调 `surface_thoughts`，thought 拼进同一条 user prompt（**不增 LLM 调用** #8）；`surface_thoughts` 消费性（mark surfaced）保证只浮现一次。降级路径（`welcome_back_bubble` canned 分支）不 surface——thought 留给下次 LLM 招呼，不白白消费。log 加 `has_thought`（#11）

**架构契合**：#1 thought 由 Rust surface、LLM 只配音 / #5 slow_tick 后台不阻塞 Body（Body 走自己的 medium loop）/ #6 LLM 未配优雅退化 / #8 reflection 每天≤1 次、thought 并入已有那次 LLM 调用 / #10「隔夜回来她说出昨晚念头」是设计文档点名场景 / #11 has_thought + source_reflection 时间戳可追溯。

**验证**：cargo test --lib **197 passed**（194+3）/ cargo test --no-run 全 harness 编译 ✅ / npx tsc --noEmit ✅ / **`cargo test --test soul_loop_harness` 2 passed（真实 LLM 端到端，新增 harness）**：①`maybe_run_if_due` 无历史→跑通，reflections 0→1，产出 3 traits+2 thoughts；②`welcome_back_consumes_surfaced_thought` 预插 thought「他今天好像有点累，希望他早点休息」→ `generate_welcome_back` 消费（surfaced_at 标记 ✓）→ 回复「你回来啦。刚才休息了一下，现在还累吗？要不要喝杯奶茶缓缓？」——**昨晚念头被自然融入招呼**（has_thought=true）+ 记忆锚定（奶茶）。**仅 slow_tick periodic 调用接线本身待常开 >1h 实跑确认**（编译 + 代码审查已高度可信）。

**Scope 边界**（留 follow-up，避免过度）：Gap3 Consolidation 反向更新 Facts（plan #9 V2）/ TurnThreshold·MajorEvent 触发器（本轮只做 Daily，满足 plan「自动触发」）/ converse 注入 thought（本轮只 welcome-back，最自然的「回来」时机）。

## §历史：welcome-back 回来主动招呼 (2026-07-27 早些)
**起因**：用户选 §下一步候选 #2（新增生命感，北极星 #10）。诊断发现 `perception/presence.rs`（`idle_seconds`/`classify`/`current_presence`）**完全空转、零调用方**；现有"回来招呼"只在 ①首次启动 welcome（`lib.rs:119`）②系统睡眠唤醒（`loop_runner` app-status resumed）触发——**缺失核心场景**：用户离开>5min 回来动鼠标她不察觉。

**实现**（6 文件；决策：LLM 记忆锚定+降级 / 仅 LongAway 触发）：
- `perception/presence.rs`：新增 `Transition::ReturnedBack{away_secs}` + `classify_transition(prev,now,away_secs)` 纯函数（仅 LongAway→Active 触发，BriefAway 不触发）+ 5 单测
- `pending/proactive.rs`：新增 `generate_welcome_back(away_secs)`（**不改 `generate` 签名**，规避踩坑 #4，现有 3 调用方零改动）。区别于 generate：不查 pending_due（回来是连接非跟进）、anchor 可选（无 anchor 仍说话，不像 generate 沉默）、welcome 语境 prompt（"用户离开了 N 分钟刚回来"）。复用 retrieve/budget 管道，tone 随 mood
- `emotion/react.rs`：新增 `welcome_back_canned(mood,away_secs) -> &'static str` 降级纯函数 + 4 单测（随 mood 桶+时长桶选文案，纯规则 #8）
- `commands.rs` + `lib.rs`：`welcome_back_bubble(away_secs)` 命令（LLM 优先/None 降级）+ invoke_handler 注册。**踩非 Send 坑**：`if let` 条件中的 MutexGuard 跨 `.await` 使 future 非 Send——改独立 `let` 绑定（同 `proactive_bubble` 模式）
- `lifecycle/loop_runner.rs`：medium 线程闭包持有 `last_presence`+`away_since`（线程局部无锁）；`check_presence_transition` 检测转换 → emit "welcome-back"；`recent_interaction_secs` 30s 守卫防与正在进行的对话撞
- `src/App.tsx`：`listen("welcome-back")` → `invoke welcome_back_bubble` → `showBubble`（onboarding/awayMode 守卫，与 proactive-prompt 一致）

**验证**：cargo build ✅ / `cargo test --lib` 194 passed（185+9）/ cargo test --no-run 全 harness 编译 ✅ / `npx tsc --noEmit` ✅。**实跑通过**：离开 5.5min（away_secs=330）回来 → 单气泡"回来啦，刚刚想到星际穿越了，怪想你的"（记忆锚定 + 情感连接到位）。

**实跑修的 2 个 bug**：
1. **StrictMode 双气泡**：`main.tsx` 用 `<React.StrictMode>`，dev 下双挂载 effect；`listen()` 异步注册，第一次 mount 的 listener 在 cleanup 之后才 resolve → 泄漏成第二个 listener → 同一 welcome-back 事件双触发、冒两个气泡（现有 6 个 listener 都有此竞态，welcome-back 是首个显著一次性事件才暴露）。修：事件 effect 加 `cancelled` 标志，late-resolving listener 自我 unlisten。**坑**：HMR 清不掉已泄漏的 listener，必须重启/刷新 webview 才能验证修复。
2. **启动误触 away_secs=0**：dev 用脚本启动 app 时用户可能已 idle>300s，`last_presence` 初始=LongAway、`away_since`=None，第一次 tick 动鼠标 → LongAway→Active 误判"回来"、away_secs=0。修：`loop_runner` 初始 `last_presence=Active`（生产用户双击启动本就 Active），只对真实 Active→away→Active 循环反应。

**架构契合**：#1 Rust 选 anchor+拼 prompt、LLM 只配音 / #5 后端 life_loop 持续检测不依赖前端 / #8 最多 1 次 LLM + 降级零成本 / #3 anchor 只来自检索不编造 / #6 onboarding/awayMode 可关 / #10 生命感优先。

## §历史：docs 治理 — 去 superpowers 嵌套 + 归档过期 (2026-07-26)
**起因**：用户选 §下一步候选 #1（docs 治理）。`docs/superpowers/` 是无意义嵌套层；docs 根堆了多份过期文档，混淆"当前有效"与"历史快照"。

**执行**（6 git mv + 8 引用）：
- design/plan：`docs/superpowers/{specs,plans}/` → `docs/{specs,plans}/`（与 `decisions/` 子目录风格一致）
- 归档：`bug-audit` / `fix-plan` / `feature-checklist` / `known-issues` → `docs/archive/`
- 引用同步 8 处：CLAUDE.md ×5（项目一句话 + 文档导航 design/plan + known-issues 描述）、Architecture-Principles.md ×2、implementation-plan.md ×1（自引 design）
- `known-issues` 主体 P1 视线已修复（86ca465），归档并改 CLAUDE.md 描述为"历史问题诊断"；`test-checklist-v2.md` 保留原位（仍有效）

**验证**：grep `superpowers` 路径引用 0 命中；git status 6 rename + 引用修改。

## §历史：清技术债 — proactive_harness 简化（2026-07-26）
`tests/proactive_harness.rs`（Codex 写）复刻了 `generate` 的 emotion/retrieval/anchor/budget/LLM 管道。简化为直接调 `proactive::generate`；顺带 `generate` 返回值升级为 `Option<BubbleOutcome{reply, anchor}>`（暴露 anchor 让 harness S1 复用 + 满足原则 #11）。3 调用方同步（commands.rs / closed_loop2_harness / proactive_harness）。lib 185 + 全 tests 编译通过。已提交 `fc8bcfe`。

## §历史：提醒功能修复（2026-07-26 早些）
**起因**：用户实测"3分钟后提醒喝水"失败——她说"没办法定闹钟，只能现在提醒"。读 5 处源码诊断出断点链。

根因链（"提醒我X在Y分钟后"为什么失败）：
1. `gate.txt` 的 pending_event 定义只含"未来计划"，不含"提醒请求" → gate 不路由到 PendingEvent
2. `PendingInput` 只有 `event_date`（绝对日期），无相对时间
3. `extractor.txt` 没引导短期提醒
4. `compute_remind_date` 只解析 `%Y-%m-%d`，不支持相对分钟
5. `converse` 不读 `outcome.extraction`，她不知这轮设了提醒 → 据"常识"答"没办法"

修复（7 文件，原则 #1：时间由 Rust 算，不交 LLM）：
- `PendingInput` 加 `offset_minutes`、`event_date` 改 `Option`（extractor.rs；serde 向后兼容旧 JSON）
- `compute_remind_date(&PendingInput, now)` 支持 `offset_minutes` → `now + offset`（store.rs）；`store()`/`mod.rs` PendingEvent 分支适配 event_date 回填
- `gate.txt` / `extractor.txt`：纳入"提醒请求"，区分短期(`offset_minutes`)/远期(`event_date`)，带中英文例子
- `converse` 读 `outcome.extraction.pending_event`，注入 system 提示让她自然确认"好的，N分钟后提醒你"
- 适配调用方 `store.rs`/`mod.rs`/`tests/golden_conversations.rs` + 更新单测（踩坑 #4：grep 全部 `PendingInput`/`compute_remind_date` 用法，抓到 golden_conversations 漏改）

**验证**：`cargo test --lib` 185 passed、闭环2 harness 1 passed；**用户实跑"3分钟后提醒喝水"全链路通过**——她回"好的"+ Pending 区有记录 + 3分钟后主动冒泡。闭环2 真实运行 ✅。

## §部署：桌面启动方式（2026-07-28）
release exe 构建一次，桌面快捷方式双击启动（无需终端 / `npm run tauri dev`）。
- 构建：`npx tauri build --no-bundle`（**勿用** `cargo build --release`——后者产物 embed 不全、webview 加载异常）。`--no-bundle` 跳过 msi 打包（wix 可能失败，非必需），只出 exe。
- 产物：`D:\cargo-target\desktop-pet\release\desktop-pet.exe`（CARGO_TARGET_DIR 重定向到 D 盘，**非** `src-tauri/target/`；bin 名 `desktop-pet`，非 productName `DesktopPet`）。
- 桌面快捷方式：`C:\Users\SunJialei\Desktop\DesktopPet.lnk`（Target=exe, Icon=src-tauri/icons/icon.ico）。
- 踩坑1（已修）：`open_devtools` 是 Tauri **debug-only** API（release 下该方法不存在 → E0599）。commands.rs 已加 `cfg(debug_assertions)` 守卫，release no-op。
- 踩坑2（已修，release-only 隐蔽）：PIXI ShaderSystem 需 CSP `unsafe-eval`，但 tauri.conf.json CSP 原本只有 `wasm-unsafe-eval`（给 Live2D Core）→ PIXI Application 创建即崩 → 桌宠空白不显示。dev 模式 tauri 自动放宽 CSP（dev 正常），release 用配置 CSP 才暴露。诊断法：WebView2 设 `--remote-debugging-port=9222` + CDP `Runtime.evaluate` 抓异常。已加 `'unsafe-eval'` 到 `script-src`。
- 重建：改 Rust/tauri.conf.json → `npx tauri build --no-bundle`；改前端 → 先 `npm run build`。快捷方式自动指向新 exe（同路径覆盖）。

## §审计：P0-P17 + 架构原则完成度（2026-07-28，对照 implementation-plan v1.1 + design v2 + Architecture-Principles）

Kill List 三闭环全部端到端跑通（Body→Memory→Soul）。逐项审计（✅ 完整 / ⚠️ 有缺口 / ❌ 未做）：

| 阶段 | 状态 | 说明 / 缺口 |
|---|---|---|
| P0 脚手架/配置 | ✅ | AppData config（踩坑#1）|
| P1 数据库 | ✅ | schema v2，8 层记忆全 |
| P2 Embedding | ✅ | BGE-M3，AppData 引导下载 |
| P3 LLM 客户端 | ⚠️ | 非流式；**流式 chat_stream 未做**（client `stream:false`）|
| P4 Emotion | ✅ | state/homeostasis/needs/pace 全 |
| P5 摄入管道 | ✅ | gate/extractor/store/correction/working |
| P6 检索管道 | ✅ | trigger/retrieval/budget/grounding，score breakdown |
| P7 Planner | ⚠️ | director+actor 闭环；**流式逐字渲染未做** |
| P8 Pending | ✅ | 闭环2 实跑 |
| P9 Body 窗口 | ✅ | Live2D/透明/点击穿透 |
| P10 FSM | ⚠️→✅ circadian | fsm+emotionDriver(连续表情)+microBehavior+circadian sleepiness 接入 ✅；idle_weights 硬编码(非 JSON，可调) |
| P11 交互 | ⚠️ | 摸头/戳/注意力三态 ✅；气泡生命力(节奏+glyph) ✅；Foley 音效 5 音 ✅；**走路脚步声 loop、Alt+Space 全局键未做** |
| P12 物理 | ⚠️ | 空间(窝/回巢)/昼夜 ✅；**自由落体/任务栏弹跳简化(松手停原地)** |
| P13 Soul | ⚠️ | reflection/monologue/consolidation+慢循环闭环 ✅；**TurnThreshold/MajorEvent 触发器、Consolidation 反向更新 Facts 未做** |
| P14 感知 | ✅ | time/presence/window 模块全 |
| P15 Life Loop | ✅ | 三循环+recovery(前端catch)+firstrun 访谈 |
| P16 Debug Panel | ⚠️ | Brain/Counts/Facts/Episodes/Pending/Timeline ✅；**Prompt token/Retrieved score/Reflect/AnimFSM/Cost 分区缺** |
| P17 Golden | ⚠️ | golden_conversations 测试数据有；**evaluation 框架+人格漂移 score+CI 未完整** |
| A1 BrainState 快照 | ❌ | converse 多参数，未统一 BrainState（架构债）|
| A2 统一 Scheduler | ❌ | loop_runner 线程+sleep，非 Scheduler trait（架构债）|
| A3-A6 | ✅ | 直接调用+事件 / Change Log / Suspend-Resume / schema_version |

## §未解决问题
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。现在 `BubbleOutcome.anchor` 已暴露，Debug Panel 可顺手显示"当前冒泡锚定的记忆"。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选（按优先级重排，基于 §审计 + 北极星 #10 + Kill List 已完成）

> ⚠️ **本节为 07-28 快照，已过时**（Tier1 三项全完成、Tier2 #4/#5 已做）。最新统一优先级 backlog 见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选)。保留下方作历史对照。

Kill List 三闭环已完成，现按"提升体验/生命感"→"闭环深度"→"Body 完善"→"开发者基建"→"架构债"→"二期"排序。

**Tier 1 — 生命感/体验（#10 北极星，对话是核心交互）**
1. ✅ **流式回复**（已完成并实跑确认）：ipc::Channel 逐字渲染（短回复看不出逐字是 DeepSeek-v4 reasoning content 占比小，非 bug）。详见 §最近一轮。
2. ✅ **气泡生命力**（P11.3，已完成 `abb9d49`，待实跑）：打字节奏随情绪（`bubblePacing` 6 档）+ 无文字气泡（glyph 省略号/叹气，#12）。形态动画本就有 5 种 keyframes。「害羞慢现」缺后端 mood 标签未做（follow-up）。
3. ✅ **Foley 音效**（P11.5，已完成实跑通过）：真实素材 10 接入（ow/啊/啊1/生气/笑/布料/落地/跳/UI/hi）+ 权重静默优先 + cooldown + 亲密度分档 + 启动招呼(autoplay 补播)；sleep 预留（Sleeping 未做）。详见 §最近一轮。

**Tier 2 — Soul/对话深度（闭环增强）**
4. ✅ **converse 注入 surfaced thought**（已完成，build 过 / 待实跑）：正常对话也带出昨晚念头。converse Step 8 后注入克制措辞的 thought_clause（#8 零额外 LLM、消费性与 welcome-back 自洽）。详见 §最近一轮。
5. **Reflection TurnThreshold/MajorEvent 触发器**：每 30 轮 / importance>0.85 自动反思（现只 Daily）。
6. **Consolidation 反向更新 Facts**（#9 V2）：压缩总结中的事实回写 Facts。

**Tier 3 — Body 完善**
7. ✅ **circadian 接入微行为**（已完成，build 过 / 待实跑）：sleepiness 调制 idle 权重（深夜 yawn↑/look_around↓）。详见 §最近一轮。follow-up：speedModifier 未接动画速度；Sleeping 自动入睡/唤醒机制（现只调权重，未真正入睡）。
8. **完整物理**（P12.1）：自由落体 + 任务栏弹跳（现简化松手停原地）。

**Tier 4 — 开发者基建（#11 Explainability）**
9. **P16 Debug Panel 补全**：Prompt token / Retrieved score breakdown / Reflect(has_thought/unsurfaced) / Cost 分区。
10. **P17 Golden 评估框架**：人格漂移 score + CI 自动跑（现 golden 数据有，框架不完整）。

**Tier 5 — 架构债务（重构，功能已在跑）**
11. **A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表。
12. **A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14 二期清单）**
13. Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

---

## §下一步总清单（2026-07-31，统一优先级 · 取代上方 §下一步候选）

> **权威 backlog。** 上方 §下一步候选 是 07-28 快照（Tier1 已全完成、Tier2 #4/#5 已做），仅作历史对照。
> Kill List 三闭环全部端到端跑通（活着 Body → 记住你 Memory → 懂你 Soul）。
> 排序驱动：北极星 #10（优先生命感不优先功能）+ 优先级阶梯（活着→记住→懂你→工具砍）+ 实施计划 P0-P17 / A1-A2。
> 两类工作：**① 待验收**（已编码、收尾即闭环，最高 ROI）→ **② 待开发**（按 Tier 优先级）。

### ① 待验收（代码层已全部验收 ✅ 2026-07-31 18:01；GUI 实跑待用户）

> **代码层闭环**：`cargo test --lib` **207 passed** / `cargo check --tests` 全 harness 编译 ✅ / `tsc --noEmit` ✅ / `npm run build` ✅（2.12s）。**全部已 rebuild 进 release exe**（`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 07-31 18:01，含工作树未提交的 A1/A2/A4 Rust 改动；桌面快捷方式自动指向）。A1-A6 代码层验收通过，余下仅 GUI 运行时实跑（见"运行时实跑"列）。

| # | 项 | 代码层验收 | 运行时实跑（用户） |
|---|---|---|---|
| A1 | consolidation max_tokens 修复 | ✅ `consolidation.rs:89` `Some(4096)` + `:97-103` 空 content 防御 | 需攒 ≥100 低 importance episodes 自然触发，难快速复现（不必强测） |
| A2 | Reflection TurnThreshold/MajorEvent 触发器 | ✅ 优先级 Daily→MajorEvent→TurnThreshold + 12 单测全过 | 需攒 30 条对话记忆 或 importance>0.85 事件 |
| A3 | converse 注入 surfaced thought | ✅ `converse.rs:202-221` 注入 + 消费性 | 需 reflection 先产 thought（一日以上），下次对话观察带出 |
| A4 | Sleeping 入睡/唤醒 | ✅ `App.tsx:216-222` 入睡 + `:604-607` 唤醒 | **可立即验证**：改系统时间 2-6 点 + 不交互 10min→入睡；戳/摸/对话→唤醒 |
| A5 | circadian sleepiness 调权重 | ✅ `microBehavior.ts` sleepy 公式 + `App.tsx:226` 喂入 fsm.tick | **可立即验证**：深夜 yawn↑ / look_around↓（对比白天） |
| A6 | emotionBridge 连续表情 | ✅ `App.tsx:56` toEmotionVector + `:934` 传 Live2DCanvas | **可立即验证**：戳→嘴角下垂；开心→微笑笑眼；久运行→半眯 |
| A7 | ~~多气泡堆叠~~ | ❌ **未实现** | 降级为 ③ follow-up（见下） |

> **A7 勘误**：原 backlog 把"多气泡堆叠"列为待验收，核验发现 `App.tsx:75-77` 气泡是单气泡状态（`bubbleText/Visible/Style/Pos` 均单一 useState）、`showBubble`(:159) 是覆盖语义（新气泡直接覆盖旧的 + 重置 timer），从未实现堆叠。降级为 follow-up；若用户确认需要"堆叠/排队"再开。

### ② 待开发（按优先级）

**Tier 2 — Soul/对话深度（懂你 · 闭环增强）**
- ~~**B1. Consolidation 反向更新 Facts**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：`consolidate` 成功后调 `backfill_facts`（LLM 从摘要提取 JSON 事实 → category 白名单+confidence clamp → `expire_old` 冲突过期 + `dedup_insert`）。失败隔离（只 warn）。+8 单测 + 新 `consolidation_harness`（真实 LLM 端到端）。详见 §最近一轮 (2026-08-03)。
- **B1b. Grounding 运行时阻断（B 档 · ⏳ 条件触发）**：A 档（prompt 收紧）实跑若仍偶发主动开口幻觉则升级——`check_groundedness` 补中文 claim 模式（现全英文、中文漏检）+ 在 proactive/welcome_back 输出端挂检测、发现编造就丢弃/降级。根因+修复详见 §最近一轮 (07-31 19:10)。

**Tier 3 — Body 完善（活着 · 生命感）**
- ~~**B2. 完整物理**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：自由落体 + 任务栏弹跳（P12.1）。新 `gravity.ts`（GRAVITY/BOUNCE 常量 + `stepGravity` 纯函数）。**关键**：发现 `startDragging` 吞 webview 鼠标事件（旧 `onUp` 死代码）→ 改 `onMoved`+静止检测；petPos useState→ref 重构修卡顿。用户偏好"1/3 飘落悬停"（不真触底，bounce 当前是死代码，待确认）。详见 §最近一轮 (2026-08-03)。
- ~~**B3. Sleeping 配套收尾**~~ ✅ **已完成（2026-08-03 续，纯前端）**：① 睡着抑制 DeepNight/LateNight nudge（`App.tsx` nudge effect 加 `fsmRef.state===Sleeping` 守卫，不再梦话）② 接 sleep 音效（`soundManager.ts` 加 `"sleep"` AssetKey + `sleep()` 方法 mirroring `greet()`；入睡时 `sound.sleep()`，mute 尊重 #6）③ LateNight 不入睡只 yawn（**已满足、零改动**：auto-sleep 本就 DeepNight-only）。详见 §最近一轮 (2026-08-03 续)。**待实跑**。

**Tier 4 — 开发者基建（#11 Explainability · ⭐ 当前最高 ROI 且未受阻）**
- ~~**B4b. conversations 死表修复**~~ ✅ **本轮完成（2026-08-03 续③）**：审计确认真 Bug——生产路径从未调 `conversations::insert`（grep 0 / callers 仅测试）。`commands.rs::send_message` 镜像 working_memory push 写 user+assistant turn。详见 §最近一轮 (2026-08-03 续③)。
- ~~**B4-MVP. Debug Panel 决策链分区（Retrieved+Intent+Reflect）**~~ ✅ **本轮完成（2026-08-03 续③）**：服务"她为什么这么说"诊断链。详见 §最近一轮 (2026-08-03 续③)。
- **B4-余. Debug Panel 补全（follow-up）**：~~Cost~~ ✅ 续③；~~AnimFSM（当前态+history）~~ ✅ **续⑧**（fsm.getHistory + DebugPanel AnimFSM 分区）；~~Prompt（动态 token）~~ ✅ **续⑧**（PromptTokenDebug → DecisionTrace → Last Turn "sys N/budget M"）。**Debug Panel 9 分区全补齐**（Brain/Counts/Cost/Facts/Episodes/Pending/Timeline/Last Turn/Retrieved/Reflect/AnimFSM）。待 dev 实跑确认 AnimFSM/Prompt 渲染。
- **B5. P17 Golden 评估框架**：✅ **三层完成**——① 规则启发式层（2026-03 续⑧，`personality_drift_score` 抓 GROSS 话痨/卖萌/依赖）+ ② 语义 cosine 层（2026-08-08 Item6，`semantic_drift_score` 抓语气漂移）+ ③ **LLM-as-judge 层（2026-08-08 续，`tests/personality_judge_harness.rs`：30 条 golden 集 + persona_fit 0-10 + 漂移维度命名 + 3 次退避重试）**。规则/cosine 是廉价 CI 线（合成向量 + 规则单测），judge 是重手动线（同 prompt_quality/embedding_ab 模式）。三层交叉验证各覆盖边界：规则层对 Subtle(cold/客服腔/鸡汤/动作描写) **0/10 盲**、judge 是唯一抓这些的线。详见 §最近一轮 (2026-08-08 续)。

**Tier 5 — 架构债务（重构 · 功能已在跑）**
- **B6. A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表（架构债）。
- **B7. A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14）**
- **B8.** Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

### ③ 散落 follow-up（低优先 · 可并入相关 Tier）
Alt+Space 全局键（P11.4）/ 走路脚步声 loop（P11.5）/ 害羞慢现气泡形态（缺后端 mood 标签）/ ~~rest_need 后端暴露（P10）~~ ✅ **2026-08-04**（含激活生产 homeostasis + 恢复项；详见 §最近一轮 2026-08-04）/ ~~speedModifier 接动画速度（circadian）~~ ✅ **2026-08-04**（PIXI ticker.speed；energyModifier 仍未消费——能量已是情绪维度，speed 够用）/ idle_weights JSON 化（数据驱动）/ ~~选择性遗忘（用户请求"忘掉..."，P13 lifecycle_cleanup）~~ ✅ **2026-08-04 续 episode MVP + 2026-08-05 fact/pending 扩展**（gate Forget + `forget_best_match` 三路调度 episode/fact/pending + converse 确认；详见 §最近一轮 2026-08-05。**仍留 follow-up**：多轮消歧义、fact/pending 语义级匹配需加向量）/ **loneliness 生产未激活**（apply_homeostasis_time_aware 不更新；tick_needs 死代码；影响检索/planner，激活属行为变更需评估）/ ~~**FTS5 全历史检索**~~ ❌ **2026-08-05 证伪**（bundled SQLite 三分词器对中文 MATCH 全 0——无 CJK 分词；"fts5_cjk"旧记错误；除非引入 jieba 扩展/Rust 分词否则不可行，已从 backlog 移除）。

### ③ Hermes 记忆优化 follow-up（续③ 立项，按 ROI）
~~FTS5 全历史检索~~ ❌ **2026-08-05 证伪移除**（见上，CJK 不兼容）/ ~~"关系进展摘要"（后台每 N 次对话异步总结，对应 Hermes 后台 review）~~ ✅ **2026-08-07**（relationship_reviews 表 + soul/review.rs + [Relationship] 注入 + slow_tick 调度；详见 §最近一轮）/ 记忆可视化编辑（Debug Panel 只读→可改）。

> **建议下一会话起点**：先清 ① 待验收（A1-A7 逐项 rebuild+实跑，零新代码、闭环既有成果），再按 B1→B8 推进。实跑前提：`%APPDATA%\DesktopPet\config.toml` 配好 DeepSeek key + 桌面快捷方式（或 `npm run tauri dev`）。
