# 念头流 v2：三层决策冒泡架构方案

> 2026-08-27。前置阅读：[`docs/research/2026-08-27-proactive-bubble-research.md`](../research/2026-08-27-proactive-bubble-research.md)（双调研整合与勘误）。
> 目标：彻底解决冒泡死板——内容像预定好的、时间错位（下午说"今晚"）、第二人称问句突兀、节奏是节拍器。**效果优先，允许颠覆现有管线**；同时守住 Architecture-Principles（#1 LLM 只表达 / #6 可关可调 / #8 成本 / #11 可观测 / #12 沉默也是表达）。

---

## 一、总架构：念头流（持续内心状态）+ 三层决策（评估与发声解耦）

```
                      ┌──────────────── 念头流 thought_stream（SQLite）────────────────┐
                      │  origin: environment / body / memory / self_state / temporal  │
                      │          ritual / pending / reflection                        │
                      │  字段: content(第一人称陈述种子,无时间词) salience 衰减 状态     │
                      └──────▲───────────────────────────────────────────┬───────────┘
                             │ 喂流(零LLM)                                │ top-K 候选
  环境 diff 事件环 ──────────┤                                            ▼
  时段边界/情绪越阈 ──────────┤   ┌── 第一层 Rust 硬门(30s tick, 零成本) ──────────────┐
  记忆 selector(保留) ────────┘   │ 全局预算│硬静音时│skipRecent│interrupt│退避│深专注  │
  时间触发器(扫日期)            └────────────────────┬────────────────────────────┘
  反思(夜间整理)                                     │ 门开才评估(省掉全部无效窗口)
  空闲成形(flash批量,P2)                             ▼
                                    ┌── 第二层 flash 静默评估(reasoning开,~2K tok) ──┐
                                    │ in: 候选念头+此刻真实时间/时段/星期+环境快照     │
                                    │     +最近气泡(embedding查重>0.75先降权)         │
                                    │ out:{speak,intent,hook,reason}                 │
                                    │ speak=false 是一等结果 → Debug Panel 沉默透明   │
                                    └────────────────────┬───────────────────────────┘
                                                         │ speak=true
                                                         ▼
                                    ┌── 第三层 主模型渲染(thinking关,每气泡1次) ──────┐
                                    │ persona + 选中念头+intent/hook + 发声时刻       │
                                    │ 真实时间锚定 + 弱场合标签 + 最近气泡             │
                                    │ → 1句,第一人称为主,陈述句默认,问句稀少          │
                                    │ grounding_guard 保留                            │
                                    └─────────────────────────────────────────────────┘
```

与现状的根本区别：现在的 6 条管线是"**场合枚举 × 手写模板**"，气泡=对既定场合的应答；新架构是"**内心状态持续存在，评估后可能说也可能不说**"，气泡=此刻念头的自然溢出。场合（早安/晚安/到期/欢迎回来）不再拥有独立模板，只是高 salience 的念头来源。

### 设计依据（调研 → 决策映射）
- 评估/发声解耦：Inner Thoughts 五段循环、LettaBot silent envelope、ProCoT、ProactiveEval 规划-引导分离。
- 沉默增益 1.02^t、念头储池演化：Inner Thoughts。
- 退避/静音时/skipRecent/interrupt：Kindroid 实证、Nomi 官方、LettaBot 配置。
- 渲染关 thinking：ProactiveEval 定量结论（thinking 伤引导：一轮倾倒、元数据泄漏）。
- 时间触发器：Zep 双时间线 + Kindroid 日历采样的 SQLite 平替。
- 内容锚定 noticing：bark 系统动词锚定 + Inner Thoughts stimulus 标注。

## 二、数据模型

### 2.1 新表 `thought_stream`
```sql
CREATE TABLE thought_stream (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL,          -- 念头种子:第一人称陈述句,无时间指示词
  origin TEXT NOT NULL,           -- environment|body|memory|self_state|temporal|ritual|pending|reflection
  origin_hint TEXT,               -- 素材上下文(如"连续编辑 main.rs 40分钟")
  salience REAL NOT NULL DEFAULT 0.5,
  created_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',  -- pending|voiced|expired|absorbed
  voiced_at TEXT,
  evolved_from TEXT               -- 演化链(惦记感)
);
```
入库守卫（Rust，单测覆盖）：`deictic::neutralize` + 时间词黑名单（今晚/昨晚/深夜/今天下午…一律拒收，念头必须时间中性——时间只在第三层渲染时注入）。

### 2.2 `bubble_log` 增列 `user_responded INTEGER`（30min 窗口内用户是否回应，Rust 判定）——退避与 P3 回应启发式的数据源。

## 三、三层细节

### 第一层：Rust 硬门（`soul/stream.rs::gate()`，挂 medium loop，零 LLM）
按序检查，任一不过→本窗口静默：
1. 全局气泡预算（现有 `bubble_budget`，间隔加 **lognormal 抖动**去节拍器）；
2. **硬静音时**：晚安仪式已说过→次日 06:00 前全静默（早安仪式除外）；
3. **skipRecent**：用户 30min 内有任何交互→跳过（LettaBot skipRecentFraction 同款思想）；
4. **退避状态机**：连续 unacked 次气泡未获回应→有效间隔=base×2^min(unacked,8)；任意用户交互→清零；仪式念头不受退避影响（日期驱动，Kindroid 同款豁免）；
5. 深专注抑制（现有）。

### 第二层：flash 静默评估（`soul/stream.rs::evaluate()`，reasoning 开，≥10min 节流）
- 输入：top-K 候选（salience×新鲜度 λ=0.95/tick×情绪放大[loneliness/rest_need]×沉默增益 1.02^t_silence，Rust 先算并排序）+ 剔除与 bubble_log 最近 5 条 embedding>0.75 的候选 + 此刻时间/时段/星期 + 环境快照（复用 environment sanitize 管道）+ 最近气泡原文。
- 输出 JSON：`{speak: bool, pick: id, intent: "随口说/惦记/提醒/陪伴", hook: "从什么切口说起", reason: "一句话"}`。
- **speak=false 写入 Debug Panel**（Kindroid Thought Bubbles 式："她决定先不打扰你——刚聊过没多久"）。
- ProCoT 轻量版：intent+hook 即"目标+策略"，渲染层消费——弱模型尤其受益（ProactiveEval 消融：拿掉目标弱模型引导分跌 25.8%）。

### 第三层：统一渲染器（`soul/stream.rs::voice()`，thinking 关，每气泡 1 次主模型）
- prompt = persona（system.txt 精简）+ 选中念头+intent/hook + **发声时刻真实时间**（星期/时段/距今）+ 弱场合标签（"ta 刚回来"/"清晨"/"这是你答应过的"）+ 最近气泡。
- 输出约束：1 句、第一人称视角为主、**陈述句默认**（"我今天比较安静"式碎碎念）、问句稀少且需 intent 许可；无禁令堆叠——人格棱角靠 persona 保，不靠禁词清单。
- grounding_guard 保留；渲染产物 log 进 bubble_log。

### 喂流来源（全部零 LLM，除注明）
| 来源 | 触发 | 实现 |
|---|---|---|
| environment | App 切换/文件切换/项目切换/回来 | 复用 `perception/environment.rs` 环形缓冲，新增订阅接口，diff 事件→素材行念头 |
| body/self_state | 时段边界、情绪越阈、久坐/长静默 | medium loop 检测 |
| memory | 候选池有值得提的记忆 | **selector 整体保留**，产出投流（origin=memory） |
| temporal | 今天==事件日/临近 N 天 | slow tick Rust 扫 pending.event_date + facts.valid_from/valid_to + episodes 时间（"你说的交稿日是不是今天"） |
| ritual/pending | 早安/晚安窗口、到期事件 | 折叠为高 salience 免预算念头 |
| reflection | 夜间整理（P3） | 反思产出改写入流 |
| 空闲成形 | 流中可说念头<2 且有新素材（P2） | flash 批量把 3-5 条原料合成 2-3 条种子，≥15min 节流 |

## 四、实施计划

### P0 立即修复（当天，独立提交——不等重构）
1. **reflection.txt 重写**：注入真实时间（`run_reflection` 组装时 replace `{now_local}`，按实际时段措辞，删除"深夜安静的时刻"）；念头改**第一人称自述默认、禁问句、禁时间指示词**；Rust 入库校验+单测。
2. **启动念头不再直出**（App.tsx:833-850）：`get_pending_thoughts` 返回内容改走新命令 `voice_thought`（LLM 再表达+当前时间锚定；LLM 不可用→宁可不显示）。
3. **welcome thought_clause "你昨晚"→实际相对时间**（proactive.rs:1110；converse.rs:905 同查）。
验收：`cargo test --lib` 新单测过；下午手跑 trigger_reflection，产物无时间错位、无第二人称问句。

### P1 念头流+三层决策 MVP（1-2 天）
- thought_stream 表 + `db/thoughts.rs` + `soul/stream.rs`（ingest/评分/硬门/退避/静音时/skipRecent/interrupt/evaluate/voice）。
- 替换 `generate_lively`/`generate_lonely_bubble`/`generate_welcome_back`/ritual 生成路径的模板分支（函数保留为 legacy，config `[proactive] engine="stream"|"legacy"` 回滚，#6）。
- Debug Panel 新增"念头流"分区：候选+评分+评估决策+理由（含 speak=false）。
验收：`cargo test --lib` + 三 harness 编译跑通；实跑观察 Debug Panel 决策链。

### P2 质量与节奏
时间触发器；语义查重调参；间隔 lognormal 抖动；多样性配额（origin 四类近 10 条占比约束，Rust 计数）；念头演化（同 origin_hint 未说念头被后续事件强化→salience↑，允许 flash 演化内容）；空闲成形。
验收：模拟 24h 冒泡序列（harness）：问句率<20%、无两条相似度>0.75、origin 分布不塌缩。

### P3 Sleep-time 整理 + 回应启发式
反思升级为夜间念头整理（合并/衰减/沉淀过夜念头）；bubble_log.user_responded 挖掘"什么 origin/时段被回应"→一行策略启发式注入评估 prompt（轻量 PRINCIPLES）。internal_thoughts 直出闭环下线（表兼容保留，converse/welcome 注入改读流）。

### P4 评估线
`bubble_nature_harness`：连续 N 次冒泡 LLM-as-judge——时间一致性（对照注入时间）、第一人称占比、问句率、语义重复度、origin 多样性；评分维度借 ProactiveEval 五维裁剪为陪伴版（循序渐进/个性化/语气/简洁/自然）。

### Kill list
六份场合模板 prompt、`lively_prompt` 禁令清单、启动念头直出逻辑。保留：selector（记忆念头来源）、greetings.ts（重启兜底，零成本）、bubble_budget、grounding_guard、deep-focus/presence 感知。

## 五、成本核算（#8）

| 项 | 频率/日 | 成本 |
|---|---|---|
| 第一层硬门 | 2880 tick | 0（纯 Rust） |
| 第二层 flash 静默评估 | 硬门过滤后估 5-15 次 | ~2K tok/次，v4-flash 单价 |
| 空闲成形（P2） | ≤1 次/15min 且需新素材 | flash ~2K tok |
| 第三层渲染 | =气泡数（不变） | 每气泡 1 次主模型，**thinking 关**（还略省） |
| 语义查重 | 每候选 | 0（本地 BGE-M3） |
| 时间触发器/退避/静音时 | — | 0 |
| 夜间整理（P3） | 1 次/日 | 并入现有反思调用，0 增量 |

净增：~10-35 次 flash 小调用/日。对比收益：硬门同时省掉大量"到点必发"的主模型渲染（现状每小时必发 1 次主模型调用；新架构说不上话就不渲染）——**总成本大概率持平或下降**。

## 六、总验收标准
1. 下午/上午反思产物零"今晚/昨晚/深夜"（单测+实跑）。
2. 连续 1 天实跑：陈述句为主、第一人称为主、无语义重复气泡、晚安后硬静默、未回应退避生效、深专注静默、`engine=legacy` 可回滚。
3. 用户主观标准："像她突然想到什么随口说一句"，而不是"定时汇报"。

## 七、风险与对策
- flash 评估误判（该说不说/不该说说说）：评估 prompt 内置正反论证（Inner Thoughts 防分数膨胀技巧）；Debug Panel 可观测便于调参；退避与静音时是硬规则不依赖 LLM 判断。
- 念头池枯竭：空闲成形兜底 + 沉默本身合法（#12）——宁可少说。
- 环境素材触发隐私顾虑：沿用 environment 的 sanitize+untrusted 管道，念头种子只存脱敏后的素材行，不存原始窗口标题。
