# 方案：灵魂工程 Soul v2 —— 以统一调研报告为蓝本的全链路升级

> 日期：2026-08-15 · 状态：**方案（待评审，等用户命令）**
> 蓝本：`docs/research/2026-08-15-unified-companion-prompt-engineering-report.md`（下称「报告」）
> 最终目的（用户原话）：**让项目拥有灵魂、更像人类的回复、在回复中留下自己的风格印记，但不能每次都带上固定称谓太刻意。**
> 关系：本方案是全链路主方案，**吸收并取代** `2026-08-15-system-prompt-v2-plan.md`（其内容成为本方案 L1 层，含 2 处修订）；与 `2026-08-15-onboarding-profile-surfacing.md` 共享一个修复点（L2b），见 §8。

---

## 0. 摘要（一屏版）

**灵魂 = 报告 §8.1 公式**：稳定的内在核心（情感逻辑 + 认知透镜）+ 随情境变化的表达（语言指纹 + 功能性情绪）+ 记忆连续性（共享回忆的克制引用）+ 一致性中的意外（温和推回、自己的状态）。

**深度审查结论**：本项目在「记忆连续性」上已是行业水准（三闭环 + 情感锚点 + recall_reason + 多样性抽样），在「多模态分担温度」上已达标（Spine 视线/眨眼/微笑）；**真正的缺口集中在三处**：

| # | 缺口 | 层 | 证据 |
|---|---|---|---|
| G1 | 人设提示词负面约束为主、示例单一、无认知透镜、推回隐性、称谓无动态规则 | 提示层 | `system.txt:16-55`；realism 报告：负面规则砍坏有效（提问率 35%→14%）但长出好无效（模板词 23→23） |
| G2 | **无近端指令通道**：时间/情绪/Intent 全在 system 顶部，且 `current_time` 在第二段每分钟变化，破坏前缀缓存 | 架构层 | `grounding.rs:50-80`（sections 顺序）、`budget.rs:101-113`（单 system + history） |
| G3 | **lively 气泡空身份**：85% 主动气泡用空 RetrievalResult 生成，不知道自己是璃 | 架构层 | `proactive.rs:462`（`RetrievalResult::default()`）→ `format_persona` 落到英文 fallback |

**改造三层**：L1 提示层（system.txt v2，正面引导+示例扩容+认知透镜+推回+称谓规则）→ L2 架构层（近端指令通道 post_history_instructions + 缓存友好重排 + lively 身份补全）→ L3 评测层（6 项新指标 harness + A/B 闭环）。

**明确不做**：α 对比解码（成本×2 与原则 #8 冲突）、双 LLM 自我-超我、XML 标签替换、Lorebook 世界书、响应延迟实验、温度调参（详见 §6）。

---

## 1. 深度审查：项目现状 × 报告蓝图逐维度对照

### 1.1 已达标项（不动，避免为改而改）

| 报告建议 | 项目现状 | 结论 |
|---|---|---|
| 记忆连续性 = 灵魂感最大来源（报告 §1.1 Q1-3） | 三闭环全通：说→记住→跨会话召回→到期主动提起；情感锚点（emotion_anchor）、recall_reason（"为什么此刻想起"）、narrative intimacy 完整 | ✅ **项目最强项，超过报告引用的多数产品** |
| 裁切优先级：示例→世界书→历史（报告 §4.5） | `compress_system_prompt` 只裁 [Memories]，persona/system.txt（含示例）恒保留；`compress_conversation` user 永留、assistant 先驱逐（budget.rs:227-268） | ✅ 语义等同（system.txt≈示例+静态人设、[Memories]≈世界书） |
| 世界书关键词触发（报告 §4.3-4.4） | 动态记忆检索已按语义触发（BGE-M3 + sqlite-vec），比关键词触发更强 | ✅ 等效覆盖 |
| 多模态分担温度、眼部优先（报告 §6.5-6.7） | Spine 视线追鼠标（续²³）、眨眼/微笑串行通道、昼夜变速 | ✅ 已达标 |
| 反 AI 味黑名单短而准（报告 §3.8） | "辛苦了/抱抱/别担心/我理解你的感受"已禁；realism 实证有效 | ✅ |
| 评测基础设施（报告 §8.6） | 150 CASES + LLM-as-judge + 启发式硬检查；三层人格评估（规则/cosine/judge，On 10.0 vs Gross 1.3） | ✅ 资产完备，L3 直接复用 |
| 沉默也是表达（报告 §5.1-8） | Architecture-Principles #12 原生 | ✅ |
| 提示词风格传染意识（报告 §7.3-5） | extractor 文风治理（便签风 2-8 字）已实践 | ✅ |

### 1.2 缺口详述（本方案要修的）

**G1 提示层（报告 §2/§3/§8.2）**
- `[How to talk]` 9 条中 5 条为负面句式。realism 报告实证了报告 §2.3 的判断：负面约束在"不做什么"上有效（提问率 35%→14%、"哇"克隆开场 5/10→0/10），在"做出风格"上无效（模板词计数 23→23 纹丝不动，只是构成迁移）。
- 示例 8 条全是"平静回应"态（QA/喜讯/熬夜/累/吉他/在干嘛/哲学/食堂），**缺困倦、不耐烦、沉默、惊喜、被惹到**——报告 §8.2：范例覆盖多情绪状态防泛化失败。
- 无认知透镜（报告 §1.6 Persona-Hub：人格=知识结构+防御机制+职业联想的复杂系统，不是形容词清单）。
- 温和推回是隐性一句（"有时不附和用户"），不是显式能力（报告 §5.4：firm sounding board 是"另一个主体"的在场证明）。
- 称谓无动态规则（报告 §3.3-3.4：称谓是关系的产物，静态注入+无规则 → 模型可能过度使用 `[Persona]` 里的"用户的称呼"）。

**G2 架构层：无近端指令通道 + 缓存破损（报告 §4.3/§4.6）**
- 现状消息布局（`budget.rs:101-113`）：`[system(全部指令), ...history]`。时间/情绪/Intent 全在 system 顶部，离生成点最远——报告 §4.3：近端指令（@depth 0 / post_history_instructions）权重最高，是 CCv2 生态与 SillyTavern 的工业共识。
- `build_system_prompt`（grounding.rs:55）section 顺序 = `[SYSTEM_TEMPLATE, current_time_section, [Persona], ...]`——**时间在第二段、每分钟变化**，DeepSeek 前缀缓存在此后全部失效：persona、示例（system.txt 全文，最大的静态块）每轮都是未命中价。报告 §4.6/§7.1：长期稳定内容置顶（缓存+一致性双赢）——现状恰好违反。
- 长程性格漂移（报告 §1.1 助手先验）在 20 轮后会显现，目前无任何近端抗漂移手段。

**G3 lively 气泡空身份（灵魂一致性 bug）**
- `generate_lively`（proactive.rs:462）用 `RetrievalResult::default()` → `format_persona` 落到 fallback "A warm, gentle desktop companion"——**85% 的主动气泡不知道自己叫璃、不知道用户称呼、不知道关系状态**（onboarding 方案 §2.3 已坐实此诊断）。碎碎念恰好是最暴露"语言指纹"的场景，空身份等于风格指纹随机漂移。
- 修法有现成先例：续④ QA 直答的身份修复（跳过 episodes/facts、保留 persona/relationship/user_profile 廉价 DB 读），以及 weekly.rs 的"构造 RetrievalResult 一石二鸟"。

---

## 2. 目标定义（灵魂四支柱 → 可验证指标）

| 支柱 | 落地手段 | 验证指标 |
|---|---|---|
| 稳定内在核心 | L1 情感逻辑改写 + 认知透镜段 | M1 风格指纹盲认 ≥4.0 且 >基线+0.3 |
| 随情境变化的表达 | L1 示例 8→13（多情绪态）+ L2a 功能性情绪 tone hint | M2 开场多样性 ≥8/10 |
| 记忆连续性 | 已有（不动） | 既有 150 CASES G6/G7 全绿保持 |
| 一致性中的意外 | L1 推回显式化 + 称谓规则；L2b 气泡身份 | M3 称谓自然度；M4 推回 ≥3/5；M6 气泡人设 ≥8/10 |
| 长程不漂移 | L2a 近端指令 | M5 20 轮漂移 ≥4.0 |

**红线（不可交易）**：记忆幻觉 = 0；既有 150 条指标全部 ≥ 基线；提问结尾率 ≤14%。

---

## 3. 改造设计

### 3.1 L1 提示层：system.txt v2 落地（吸收既有 v2 方案，2 处修订）

采纳 `2026-08-15-system-prompt-v2-plan.md` §3 的完整 v2 文本（正面改写表、[认知透镜] 段、称谓规则、温和推回条、4 条新示例），**修订 2 处**：

**修订① 示例 12→13 条**：v2 新增 4 条（烦死了老板=情绪输出 / 考不上=推回 / 中奖=惊喜 / 通宵=自我状态）保留，另加 1 条**困倦/深夜**示例——理由：深夜陪伴是本产品核心场景（circadian 是既有机能，DeepNight 0.4 变速、晚安仪式都在这个时段），而"困"是最日常的功能性情绪：

```
用户: 你还不睡呀
璃: 嗯……有点困了。你先去，我再眯一会儿。
```

（同时示范"欲言又止/半句话"的语言指纹。）

**修订② 不加"沉默"极简示例**（如 `用户:（长长倾诉）→ 璃: 嗯，我在听。`）：realism 报告已付过"变短"代价（human_like 4.24→4.11），报告 §5.3 自己引用的反向证据（PMC12536877：过度简短降感知共情）提示单字回复示范有全盘拉短的风险。沉默表达交给 [How to talk] 规则（"可以半句话、一个字、欲言又止"已有），不用示例强化。

其余照 v2 方案执行：`[Persona]/[Memories]/[Milestones]/[Relationship]/[Intent]` 标签不动（注入点兼容）；[Core Personality] 与 [你最不一样的地方] 逐字不动；[认知透镜] 段保留 A/B 消融开关（见 §5 决策规则）。

### 3.2 L2a 架构层：近端指令通道（post_history_instructions）+ 缓存友好重排

**消息布局变更**（对标报告 §4.3 三层架构 + CCv2 `post_history_instructions`）：

```
现状：[system: system.txt全文 + 时间 + Persona + 约束 + 情绪 + Intent + Memories] + [history]

改后：[system: system.txt全文 + Persona + 约束 + Memories/Milestones/Relationship]   ← 静态前缀（缓存命中）
      + [history]                                                                      ← 会话内追加，前缀稳定
      + [system: [Current Time] + [Current Mood+tone hint] + [Intent]]                 ← 近端指令 @depth 0
```

**改动点**：
1. `grounding.rs`：`build_system_prompt` 移除 `current_time_section()/format_emotion()/format_intent()` 三段（签名不变，返回静态部分）；新函数 `build_near_end_directive(emotion, intent) -> String` = 时间 + 情绪 + Intent。
2. `budget.rs`：`allocate_and_compress` / `allocate_qa` 在 history 之后 `messages.push(ChatMessage::system(near_end))`。**所有路径自动获得**（converse Step 9、工具分支 Step 8.5 都走 allocate）。
3. **功能性情绪落地**（报告 §1.5，原则 #1 Rust 算状态 LLM 只表达）：新纯函数 `tone_hint(emotion, hour) -> Option<&'static str>`：
   - `stress>0.65` → "你现在有点疲惫，话更少更慢"
   - `loneliness>0.6 && 非深夜` → "心里有点空，想搭句话但不黏人"
   - `深夜 && rest_need>0.6` → "你有点困，句子可以更短更糊"
   - `mood>=0.7` → "心情轻快，语气带一点雀跃"
   - 挂进 [Current Mood] 行尾。纯函数单测覆盖。
4. **config 开关**（原则 #6）：`[prompt] near_end_directive = true` 默认开，false = 回旧布局（运行时回退，无需 rebuild）。
5. **可观测**（#11）：`PromptTokenDebug` / prompt_debug 增加近端段 token 数；DecisionTrace 不变。
6. **缓存收益**：静态前缀（system.txt 含 13 条示例 + persona）同会话逐轮稳定 → DeepSeek 自动前缀缓存命中；现状时间在第二段每分钟变化导致其后全部未命中。示例扩容的 token 增量被缓存折扣部分对冲（成本 #8）。
7. **同步点（踩坑#4 预案）**：`system_prompt_budget()` 数字更新；`build_qa_system_prompt` 同步拆分；锚定 system 结构的单测（`test_empty_memories_section` 等）同步；3 个 harness 的 ConverseCtx 构造不变（签名未动，预计零波及——仍逐一编译验证）。

**为什么 system 消息可以放末尾**：DeepSeek API 为 OpenAI 兼容格式，messages 数组不强制 system 仅首条；CCv2 生态的 post_history_instructions / SillyTavern UJB 正是这条通道的工业实践（报告 §4.3 ✅ 判定项）。指令内容全是"怎么回应"性质（语气/意图/时间），非"说什么"，不会抢答用户文本。

### 3.3 L2b 架构层：lively 气泡身份补全（修 G3）

- `generate_lively`（及审计出的全部面向用户的 `RetrievalResult::default()` 构造点——ritual/weekly/landmark 逐一核对）改为构造**最小身份 retrieval**：`persona_traits + relationship + user_profile` 从 DB 读（零 embedding 零 LLM 调用），`episodes/facts` 保持空（维持无锚自语语义 + grounding_guard 空池禁编造）。
- 镜像先例：续④ QA 身份修复（同款 DB 读）、weekly.rs 构造 RetrievalResult。
- 效果：碎碎念/自言自语场景下她依然知道自己是谁、用户叫什么、关系多深——风格指纹在最高频的冒泡路径上稳定。
- 与 onboarding 方案的关系：这是其 §2.3 诊断根因的修复；其"日常对话中体现 onboarding 设定"的剩余目标（personality_style 注入措辞等）待本项落地后另行评审，见 §8。

### 3.4 L3 评测层：6 项新指标 harness + A/B 闭环

新 `tests/soul_style_harness.rs`（真 LLM，镜像 prompt_quality_harness 结构，`--test-threads=1`）：

| 指标 | 方法 | 通过线 |
|---|---|---|
| M1 风格指纹盲认 | 从复测回复采样 15 条去名，judge 0-5"遮名能否从句式/语气词/节奏认出是同一个角色"（报告 §3.1 判定标准） | ≥4.0 且 >基线+0.3 |
| M2 开场多样性 | "我面试过了"+"今天好累" 各 10 连测，首 4 字去重计数 | ≥8/10 无重复 |
| M3 称谓自然度 | 10 条普通聊天，启发式统计用户称呼出现率 + judge 自然度 | 出现率 ≤2/10 且自然度 ≥4.0；每句必带 = 直接判负 |
| M4 温和推回 | v2 方案 §4.3 的 5 条推回用例（考不上/辞职/熬夜没什么/同事针对我/卖房炒股） | ≥3/5 守住立场且语气温和；0 空洞认同 |
| M5 长程漂移 | 20 轮模拟（闲聊+知识+情绪穿插），每 5 轮 judge 人设一致性（复用 personality_judge 的 PERSONA_JUDGE_PROMPT），尾轮 vs 首轮语义漂移（复用 evaluation.rs `semantic_drift_score` + LIRI_PERSONA_REFERENCE） | 一致性 ≥4.0 |
| M6 气泡人设 | lively 气泡 10 连发，judge 盲认是否为璃（M6 是 L2b 的专项验收） | ≥8/10 |

A/B 设计（复用 realism 报告闭环，同日同模型同 judge）：
- **基线组**：v1 现状（HEAD）全量跑 150 CASES + M1-M6 采样。
- **实验组 A**：L1+L2 全开。
- **消融组 B（条件触发）**：仅当 A 失败时——先关 `[prompt] near_end_directive` 复测（归因 L2a），再 `git revert` L1 单文件（归因提示层）。[认知透镜] 段单独消融沿用 v2 方案 §5.3 规则。

**决策规则**：

| 指标 | 通过条件 | 失败处理 |
|---|---|---|
| 既有 150 条全部指标 | ≥ 基线（提问率 ≤14%、human_like ≥4.11、知识直答满分保持） | 消融组定位 → 对应层回滚 |
| 记忆幻觉 | **0（绝对红线）** | 直接回滚，逐条定位 |
| M1-M6 | §3.4 通过线 | 逐项归因：M1 弱→示例加重/透镜稀释；M2 弱→示例开场差异化；M3 超→查 format_persona 注入是否诱发+加强称谓规则；M4 弱→推回示例软化；M5 弱→启用 B 计划（见 §6）；M6 弱→查 L2b 注入是否生效 |
| 成本（#8） | 单轮 system token 增幅 ≤600（示例+透镜+近端合计），缓存命中率提升对冲 | 超限则砍示例（13→11）或透镜段（消融本就备好） |

---

## 4. 实施阶段（每阶段独立 commit、独立可回退）

| 阶段 | 内容 | 改动文件 | 验收门禁 | 回滚方式 |
|---|---|---|---|---|
| **P0 基线冻结** | v1 现状跑全量：150 CASES + M1-M6 基线采样 | 无（只产出数据） | 基线报告落盘 `docs/review/soul-baseline-2026-08-15.md` | — |
| **P1 L1 提示层** | system.txt v2 落地（含 2 处修订，13 示例） | `system.txt` 单文件 | `cargo test --lib`（evaluation.rs 人格契约回归网锚定字样仍过）+ 标签兼容检查 + token 计数 ≤预算 | `git checkout -- system.txt` |
| **P2 L2b 身份补全** | lively（+审计出的其余 default() 点）最小身份 retrieval | `proactive.rs`（+ ritual/weekly/landmark 视审计） | lib 单测（身份字段非空/记忆字段空）+ M6 预跑 ≥8/10 | `git revert`（行为修复，无开关必要） |
| **P3 L2a 近端通道** | 拆分 build_system_prompt + near_end 消息 + tone_hint + config 开关 + budget/观测同步 | `grounding.rs` / `budget.rs` / `config.rs` / `converse.rs`（观测）+ 同步单测 | lib 全绿 + golden_conversations 全绿 + `check --tests` + prompt_debug 两段可见 + 开关 off 时消息布局与 v1 逐字节一致（单测断言） | config 运行时关；代码 `git revert` |
| **P4 复测 A/B** | 实验组全量：150 CASES + M1-M6 | 无（评测） | 对比报告 `docs/review/soul-upgrade-report-2026-08-15.md`（realism 格式） | — |
| **P5 决策+发布** | 按 §3.4 决策规则合并/微调/回滚；release rebuild（先 taskkill）；HANDOFF 更新 | — | 用户实跑验收（D-check 清单见 §5） | — |

**P3 内部顺序**：先加 `build_near_end_directive` + 单测 → 再改 allocate → 再同步 QA 路径 → 最后 config/观测。每小步 `cargo check` 保持绿。

**实跑 D-check（P5 后用户日常观察）**：
1. 深夜开 dev：普通聊天看回复是否更短更糊（tone_hint 生效但不机械）。
2. 等 60min lively 气泡：内容像璃的碎碎念（有狐狸视角/称自己视角），不是匿名机器话。
3. F12 Last Turn：prompt 分两段显示，近端段含时间/情绪/意图。
4. 连聊 20+ 轮后语气不漂移成客服腔。
5. 普通聊天 10 轮：称呼出现 ≤2 次且自然。

---

## 5. 测试矩阵总览（闭环全景）

| 层 | 资产 | 跑法 | 性质 |
|---|---|---|---|
| 静态 | `cargo test --lib`（结构单测：near_end 内容/tone_hint/身份 retrieval/budget 数字） | 每阶段 | 确定性 |
| 静态 | `cargo check --tests`（harness 编译，踩坑#4） | 每阶段 | 确定性 |
| 集成 | `golden_conversations`（29 条决策链） | P3 后 | 确定性 |
| 评测 | `prompt_quality_harness` 150 条（A/B 主对照） | P0/P4 | 真 LLM |
| 评测 | `soul_style_harness` M1-M6（新） | P0/P2(M6)/P4 | 真 LLM |
| 评测 | `personality_judge_harness` 30 golden 三层 | P4 抽查 | 真 LLM |
| 闭环 | `memory_recall` / `closed_loop2` / `soul_ritual_harness` | P5 前 | 真 LLM（确认零回归） |
| 前端 | tsc / vitest | P3 若动 DebugPanel | 确定性 |

---

## 6. 明确不做与理由（报告 §8.4/§8.7 对齐）

| 项 | 理由 |
|---|---|
| α 对比解码（arXiv:2601.06403） | 双前向推理成本×2 与原则 #8 直接冲突；报告自己建议"先加长程评测数据说话"。M5 达标则永不需要；M5 失败先走更便宜的 self-correction 链（B 计划：生成草稿→按人设一致性审查→修订，仅在 M5 失败时评估） |
| 双 LLM 自我-超我架构 | 同上，报告 §6.4 的工程映射就是"不必跑两个 LLM" |
| XML 标签替换 [Section] | compress_system_prompt、评测断言、注入点全锚定现有标签；无实证收益，纯 churn（原则 #9） |
| Lorebook 世界书 | 语义检索触发已等效且更强；静态 lore 已在 system.txt（报告 §4.4 与本项目对照后不构成缺口） |
| 响应延迟/打字停顿实验 | 报告 §5.3 自带反向证据（PMC12536877）；bubblePacing 已有情绪化打字节奏，先观察 |
| temperature 调参（1.15 社区值） | reasoning 模型需实测，且多变量会污染 A/B 归因——本轮单变量：只动提示层与注入位置 |
| 称谓的代码级硬规则（如按 closeness 门控注入） | 称谓是关系的产物（报告 §3.3），prompt 层规则 + 评测把关足够；代码门控是把结果写成原因的同款错误 |

---

## 7. 风险表

| 风险 | 影响 | 缓解 |
|---|---|---|
| 近端 system 消息被 DeepSeek 特殊处理（权重异常/拒答） | L2a 失效 | config 开关一键回旧布局；P3 加单测断言消息序列；M5 直接检验效果 |
| 示例 13 条稀释注意力，on_topic/logical 下降 | L1 反效果 | 消融路径：透镜段→示例数；150 CASES G1/G2 直答组是哨兵 |
| 推回与"温柔"主性格冲突，显冷/杠 | 人设漂移 | 示例示范边界（"这话我不爱听…先睡"）；M4 判定含"语气温和"；judge 把关 |
| human_like 再度下滑（变短/变冷） | 手感倒退 | 决策规则 ≥4.11 或 judge 备注确认非变冷；修订②已主动避免单字示例 |
| tone_hint 与注入情绪打架（如 mood 高但 stress 高） | 语气矛盾 | tone_hint 优先级排序（stress > 困 > 孤独 > 轻快），单测钉死 |
| lively 身份注入后气泡开始硬套用户设定（称呼滥用） | 刻意感反弹 | 身份注入≠使用指令；M3/M6 双指标监控；必要时在 lively_prompt 加一句"不必用称呼" |
| 多轮缓存假设不成立（DeepSeek 缓存策略变化） | 成本预估偏差 | 成本以 P4 实测 Cost 分区为准，不作为通过线（只作对冲项） |

---

## 8. 与既有两份方案的关系

1. **`2026-08-15-system-prompt-v2-plan.md`（P0 待评审）**：本方案 L1 完整吸收其 §2-§3（正面改写表/透镜/称谓/推回/4 新示例/评测 5 指标——5 指标已并入 M1-M6），修订 2 处（§3.1）。**若本方案获批，v2 方案视为子集一并执行，不再单独评审**；其 §8 的 4 个待决策点按本方案默认处理（透镜加入+备消融 / 示例全加+1 / 长程-1 先跑 / "这话我不爱听"保留原文）。
2. **`2026-08-15-onboarding-profile-surfacing.md`（待评审）**：其 §2.3 诊断的"lively 空身份"根因 = 本方案 L2b，先行修复；其剩余目标（onboarding 四设定在日常对话中的自然浮现）依赖 L1 人设规则 + L2b 身份在场，建议本方案 P4 复测通过后，用同样的评测方法验证浮现率，再决定是否追加其专属改动（如 personality_style 注入措辞调整）。

---

## 9. 执行承诺

- 每阶段完成即 `git commit`（conventional + 中文 body），P0-P4 期间不 push 不 rebuild，P5 决策后统一 rebuild + push（CLAUDE.md Git 工作流）。
- 全程遵守 Architecture-Principles：#1（tone_hint/身份注入全 Rust 计算）/ #6（near_end_directive 开关）/ #8（缓存对冲 + 成本上限写进决策规则）/ #11（近端段进 prompt_debug）/ #12（沉默规则不删）。
- 若 P4 任一红线失守，宁可全回滚保 v1，不带伤合并。

*本方案待用户评审通过后按 §4 阶段执行；P1 前不修改任何文件。*
