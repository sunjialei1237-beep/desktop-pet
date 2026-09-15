# 念头流 v3：三层决策冒泡架构方案

> 2026-08-27。前置阅读：[`docs/research/2026-08-27-proactive-bubble-research.md`](../research/2026-08-27-proactive-bubble-research.md)（双调研整合与勘误）。
> 目标：彻底解决冒泡死板——内容像预定好的、时间错位（下午说"今晚"）、第二人称问句突兀、节奏是节拍器。**效果优先，允许颠覆现有管线**；同时守住 Architecture-Principles（#1 LLM 只表达 / #6 可关可调 / #8 成本 / #11 可观测 / #12 沉默也是表达）。

## v3 变更记录（吸收 GPT 评审后修订）
1. **种子去台词化（本次最重要修订）**：thought_stream 从"文字池"改为"心理状态池"——存 `{stimulus, emotion_tone, relation_hint}` 结构化心理状态，**不存成句台词**；`possible_expressions` 不落库，评估时由 flash 惰性生成。链路补全为 `事件 → 心理状态 → 评估 → 语言`（v2 缺"心理状态"层，存句子仍是高级模板池）。
2. **动机评分命名化 + 低分硬静默**：Rust 评分采用命名分量公式（interest/emotional_need/relationship_value/timing_bonus − annoyance_cost/recent_contact_penalty），score<0.3 直接沉默（省 flash）。**不采纳">0.7 跳过 flash"**——高分时刻恰恰最需要语境把关，flash 对所有开口路径保留终审。
3. **表达类型配额取代"第一人称陈述默认"**：全"我今天…"会变成 AI 日记；改为五类表达轮换配额（自言自语/观察/回应环境/关心/提问），用户指向类表达必须由种子里的真实 stimulus 授权（防监控感）。
4. **新增 origin=relationship**：用户习惯/互动模式/共同经历（"他通常晚上进入深度开发"），由夜间反思与 relationship_review 挖掘——熟悉感的来源（Kindroid Learned Context 同思路）。
5. **双轨降级修正**：流素材枯竭时优先"降级素材"（时间流逝本身就是 stimulus，Rust 零成本可造种子），legacy 仅作故障应急不作常规 fallback——常规回退会把旧死板带回来。
6. **unspoken（未说出口的念头）成为一等状态**：评估拒说但保留 salience 的念头，次日经时间触发器强化后轻轻带出（"昨天很晚才睡吧"）——"她记得"的最高形式。
7. **人格惯性以派生量实现**：不新建 LLM 自由书写的 persona_state（避免与 persona_traits/emotion 三重状态打架，违反 #2 BrainState 统一快照精神），改为 Rust 从既有数据滚动计算的"近日表达倾向"注入 prompt。
8. **"她在想什么"列入用户功能路线图**：长按角色看她此刻/忍住没说的念头（第一人称转述，不暴露分数），P4 后试点。

---

## 一、总架构：念头流（持续内心状态）+ 三层决策（评估与发声解耦）

```
                      ┌──────────────── 念头流 thought_stream（SQLite）────────────────┐
                      │  origin: environment / body / memory / self_state / temporal  │
                      │          ritual / pending / reflection                        │
                      │  字段: stimulus/emotion/relation(心理状态,非台词) salience 状态 │
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

### 2.1 新表 `thought_stream`——心理状态池，不是台词池
种子是**结构化心理状态**（Rust 组装，零 LLM）；台词只在第三层渲染时诞生。链路：`事件 → 心理状态(种子) → 评估 → 语言`。
```sql
CREATE TABLE thought_stream (
  id TEXT PRIMARY KEY,
  stimulus TEXT NOT NULL,      -- 触发素材(脱敏事实,Rust组装):"用户连续编辑 main.rs 40分钟"
  emotion_tone TEXT,           -- 种子产生时的情绪色调(EmotionState 映射):好奇/惦记/困倦…
  relation_hint TEXT,          -- 它对"我们"意味着什么:"关注项目进展""他最近压力大""上周说好今天交稿"
  origin TEXT NOT NULL,        -- environment|body|memory|self_state|relationship|temporal|ritual|pending|reflection
  salience REAL NOT NULL DEFAULT 0.5,
  created_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',  -- pending|voiced|unspoken|expired|absorbed
  unspoken_reason TEXT,        -- 评估拒说的理由——次日"她记得"的原料
  voiced_at TEXT,
  evolved_from TEXT            -- 演化链(惦记感)
);
```
- **不存成句台词**（v2 的 content 字段删除）：存台词=高级模板池，迟早回潮成"AI 日记"。
- **`possible_expressions`（可选表达）不落库**，评估时由 flash 惰性生成——省成本，且表达跟着渲染时刻的语境走。
- 入库守卫：stimulus/relation_hint 走 `deictic::neutralize`（只允许相对事实如"40分钟"；**今晚/昨天这类绝对时间指示词拒收**——时间只在渲染时注入真实值）。
- `unspoken` 与 `expired` 的区别：前者是"想说但忍住了"（salience 保留，可被次日触发强化），后者是自然遗忘。

### 2.2 `bubble_log` 增列 `user_responded INTEGER`（30min 窗口内用户是否回应，Rust 判定）——退避与 P3 回应启发式的数据源。

## 三、三层细节

### 第一层：Rust 硬门（`soul/stream.rs::gate()`，挂 medium loop，零 LLM）
按序检查，任一不过→本窗口静默：
1. 全局气泡预算（现有 `bubble_budget`，间隔加 **lognormal 抖动**去节拍器）；
2. **硬静音时**：晚安仪式已说过→次日 06:00 前全静默（早安仪式除外）；
3. **skipRecent**：用户 30min 内有任何交互→跳过（LettaBot skipRecentFraction 同款思想）；
4. **退避状态机**：连续 unacked 次气泡未获回应→有效间隔=base×2^min(unacked,8)；任意用户交互→清零；仪式念头不受退避影响（日期驱动，Kindroid 同款豁免）；
5. 深专注抑制（现有）。

### 第二层之一：Rust 动机评分（命名分量，Debug Panel 可展开各项）
```
motivation = interest + emotional_need + relationship_value + timing_bonus
           − annoyance_cost − recent_contact_penalty        （各 0~1，权重 config 可调）
```
- interest=种子 salience；emotional_need=loneliness/rest_need 放大；relationship_value=亲密度与 relation_hint 关联度；timing_bonus=沉默增益 1.02^t_silence（封顶）+ 时段合适度；annoyance_cost=deep-focus/刚聊过；recent_contact_penalty=与 bubble_log 最近 5 条的 embedding 相似度（>0.75 重罚）。
- **score<0.3 → 直接沉默**（省掉 flash；"忍住不说"的硬保证来自系统规则而非模型随机）。
- **0.3~1.0 → 全部进 flash 终审**。不设">0.7 跳过 flash"：高分时刻（惦记的事到期、情绪峰值）恰恰最需要语境把关——跳过它等于把"它想说就说"的裁判从 flash 换成 Rust，问题没变只是换了人。**Rust 管"想说的强度"（机械可算），flash 管"现在说合不合适"（语境判断）**，两者不互相替代。

### 第二层之二：flash 静默评估（`soul/stream.rs::evaluate()`，reasoning 开，≥10min 节流）
- 输入：过阈值候选（stimulus/emotion_tone/relation_hint 结构化呈现）+ 此刻时间/时段/星期 + 环境快照（复用 environment sanitize 管道）+ 最近气泡原文。
- 输出 JSON：`{speak: bool, pick: id, expression_type, intent, hook, reason}`——expression_type ∈ {自言自语, 观察, 回应环境, 关心, 提问}，受第三层配额约束。
- **speak=false → 种子转 unspoken（保留 salience），理由记入 Debug Panel**（Kindroid Thought Bubbles 式："她决定先不打扰你——刚聊过没多久"）。
- ProCoT 轻量版：intent+hook 即"目标+策略"，渲染层消费——弱模型尤其受益（ProactiveEval 消融：拿掉目标弱模型引导分跌 25.8%）。

### 第三层：统一渲染器（`soul/stream.rs::voice()`，thinking 关，每气泡 1 次主模型）
- prompt = persona（system.txt 精简）+ 选中种子（stimulus/emotion_tone/relation_hint）+ expression_type + intent/hook + **发声时刻真实时间**（星期/时段/距今；种子的年龄也交给它）+ 弱场合标签（"ta 刚回来"/"清晨"/"这是你答应过的"/"这是你昨天注意到但忍住没说的事"）+ 最近气泡 + 近日表达倾向（P3）。
- **表达类型配额**（Rust 对近 10 条气泡计数，短缺者优先）：自言自语 30% / 观察 25% / 回应环境 20% / 关心 15% / 提问 10%。第一人称陈述是常用形态但不是唯一形态——全"我今天…"会变成 AI 日记。
- **防监控感铁则**：观察/关心/调侃这类用户指向表达，必须由种子里的真实 stimulus（环境事件/记忆锚）授权，渲染只复述事实、不渲染凝视（反例教训："你喝雪碧的时候我都在看着"）；unspoken 念头次日带出时尤其如此（"昨天很晚才睡吧"是关心，"我昨晚看着你熬夜"是惊悚）。
- 1 句话；无禁令堆叠——人格棱角靠 persona 保，不靠禁词清单。
- grounding_guard 保留；渲染产物 log 进 bubble_log。

### 喂流来源（全部零 LLM，除注明）
| 来源 | 触发 | 实现 |
|---|---|---|
| environment | App 切换/文件切换/项目切换/回来 | 复用 `perception/environment.rs` 环形缓冲，新增订阅接口，diff 事件→素材行念头 |
| body/self_state | 时段边界、情绪越阈、久坐/长静默 | medium loop 检测 |
| memory | 候选池有值得提的记忆 | **selector 整体保留**，产出投流（origin=memory） |
| relationship | 夜间反思/关系回顾发现的互动模式 | "他通常晚上进入深度开发""最近回应变多了"——**熟悉感的来源**（Kindroid Learned Context 同思路），P3 夜间整理产出，零增量成本 |
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
- **流素材枯竭的降级顺序**：① 环境与时间永远可造种子（时间流逝本身就是 stimulus，Rust 零成本），流几乎不可能真空；② 长静默+用户在场 → "很久没说话"本身成为种子、允许降低开口阈值；③ legacy 引擎仅作**故障应急**（stream 模块报错兜底），不作常规 fallback——常规回退会把旧死板在最该有生命的日子带回来。
- Debug Panel 新增"念头流"分区：候选+评分+评估决策+理由（含 speak=false）。
验收：`cargo test --lib` + 三 harness 编译跑通；实跑观察 Debug Panel 决策链。

### P2 质量与节奏
时间触发器；语义查重调参；间隔 lognormal 抖动；多样性配额（origin 与表达类型双维度，Rust 计数）；念头演化（同 origin_hint 未说念头被后续事件强化→salience↑，允许 flash 演化内容）；空闲成形；**unspoken 跨日带出**（时间触发器扫描隔夜 unspoken 种子→强化→渲染带"你昨天注意到但忍住没说"标签，一句轻轻带过）。
验收：模拟 24h 冒泡序列（harness）：提问占比≤~10%、无两条相似度>0.75、origin/表达类型分布不塌缩；unspoken 带出场景人工评审无监控感。

### P3 Sleep-time 整理 + 回应启发式 + 人格惯性（派生量）
反思升级为夜间念头整理（合并/衰减/沉淀过夜念头 + 挖掘 relationship 模式种子）；bubble_log.user_responded 挖掘"什么 origin/时段被回应"→一行策略启发式注入评估 prompt（轻量 PRINCIPLES）。
**人格惯性**：不建 LLM 自由书写的 persona_state（会与 persona_traits/emotion 形成三重状态，违反 #2 BrainState 统一快照精神）——改为 Rust 从既有数据滚动计算的**近日表达倾向**（近 N 日开口率/被回应率/亲密度 → 一行摘要如"近来说得少、被回应多，今天可以稍微主动一点"），注入渲染与评估 prompt。效果等价于"昨天忍了很久→今天更倾向轻声开口"，但状态只有一份真相。
internal_thoughts 直出闭环下线（表兼容保留，converse/welcome 注入改读流）。

### P4 评估线 + 用户功能试点
`bubble_nature_harness`：连续 N 次冒泡 LLM-as-judge——时间一致性（对照注入时间）、表达类型分布、问句率、语义重复度、origin 多样性；评分维度借 ProactiveEval 五维裁剪为陪伴版（循序渐进/个性化/语气/简洁/自然）。
**"她在想什么"交互试点**：长按/摸头时以角色口吻展示当前种子与最近忍住没说的事（第一人称转述，不暴露分数/JSON）——Debug Panel 数据的用户态包装（Kindroid Thought Bubbles 桌宠化）。

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
2. 连续 1 天实跑：表达类型分布健康（提问≤~10%，无单一类型连续霸屏）、无语义重复气泡、晚安后硬静默、未回应退避生效、深专注静默、`engine=legacy` 可回滚、unspoken 带出无监控感。
3. 用户主观标准："像她突然想到什么随口说一句"，而不是"定时汇报"。

## 七、风险与对策
- flash 评估误判（该说不说/不该说说说）：评估 prompt 内置正反论证（Inner Thoughts 防分数膨胀技巧）；Debug Panel 可观测便于调参；退避与静音时是硬规则不依赖 LLM 判断。
- 念头池枯竭：空闲成形兜底 + 沉默本身合法（#12）——宁可少说。
- 环境素材触发隐私顾虑：沿用 environment 的 sanitize+untrusted 管道，念头种子只存脱敏后的素材行，不存原始窗口标题。
