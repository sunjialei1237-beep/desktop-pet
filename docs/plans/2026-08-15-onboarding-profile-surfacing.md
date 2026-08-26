# 方案：让 Onboarding 用户设定在日常聊天中被"想起"并自然带出

> 日期：2026-08-15 · 状态：方案（待评审） · 关联：闭环3「她记得我」体感
> 问题提出（用户）：初次访谈输入的"与用户的关系、性格特点"等信息，日常聊天中完全体现不出来。

---

## 1. 问题定义

首次见面访谈（onboarding）收集的 4 个用户设定——**称呼、期望性格、关系设定、桌宠名字**——存在且被注入 system prompt，但 107 轮真实对话中几乎从未被使用：

| 设定 | 存储值（app_config） | 注入位置 | 107 轮对话中 assistant 使用次数 |
|---|---|---|---|
| user_nickname | "你可以叫我爸爸" | `[Persona]` 用户的称呼 | **3**（其中 2 次是特殊提问触发） |
| personality_style | "温柔，又有点调皮" | `[Persona]` 你被期望的性格 | **0** |
| relationship_style | "伙伴" | `[Persona]` 与用户的关系设定 | **0** |
| pet_name | "小七" | `[Persona]` 你的名字 | **1** |

对照组：普通记忆（"糯米"——用户养的小狗，走 episode/fact 检索链路）被引用 **8 次**。

**结论：数据完好、注入存在，但"使用"为零。链路断在"注入 → 行为"这一环。**

---

## 2. 链路全景（实证诊断）

### 2.1 写入链路（一次性 onboarding）
```
src/App.tsx ONBOARD_QUESTIONS（4 问，注释明言"答案存入 app_config，注入 system prompt 的 [Persona]"）
  → startOnboarding() 气泡提问
  → handleSend() 访谈分流（App.tsx:1038）
  → commands.rs save_onboarding_answer
  → db/onboarding.rs::save → app_config KV（user_nickname/personality_style/relationship_style/pet_name/onboard_completed）
```
数据层完整，无 bug。

### 2.2 注入链路（每轮对话）
```
converse() → retrieve()（非 QA）/ QA 手动装载
  → RetrievalResult{ persona_traits, relationship, user_profile, ... }
  → budget::allocate_and_compress / allocate_qa
  → grounding::build_system_prompt / build_qa_system_prompt
  → format_persona → [Persona] 块：
       Core personality: caring, curious, gentle, patient, playful   ← 旧英文种子，未 reseed（HANDOFF 续² 遗留）
       Adaptive traits: ...（reflection 产生）
       Relationship: closeness 100/100, trust 0.0/100, known 0 days, 751 conversations  ← trust/days_known 死列
       用户的称呼: 你可以叫我爸爸
       你的名字: 小七
       你被期望的性格: 温柔，又有点调皮
       与用户的关系设定: 伙伴
  → compress_system_prompt：只裁 [Memories]，[Persona] 恒保留
  → chat_stream → 回复
```
QA 直答路由同样注入（build_qa_system_prompt 也调 format_persona）。

### 2.3 主动冒泡链路（proactive，被遗漏的一环）
| 冒泡类型 | 占比 | 检索装载 | 身份是否进 prompt |
|---|---|---|---|
| lively（碎碎念/自言自语） | 85%（默认） | `RetrievalResult::default()`（proactive.rs:462） | ❌ **空身份**——连"小七/爸爸/伙伴"都不知道 |
| memory 锚定 | 15% | 真实 retrieve | ✅ |
| welcome_back / lonely_nudge | - | 真实 retrieve | ✅ |

### 2.4 失败机制（7 条，按影响排序）
1. **注入是"声明"不是"指令"**：`[Persona]` 只陈述身份，`system.txt` 的 `[How to talk]` 没有任何一条指示模型使用称呼/关系/性格。模型把 4 行当惰性背景。
2. **身份冲突**：`system.txt` 首行硬编码 "You are 璃 (Liri)"（最强位置信号），`[Persona]` 才说"你的名字: 小七"。模型以首行为准，自称小七仅 1 次。
3. **零浮现机制**：4 值是常数 → 模型习惯化（habituation）忽略。对比糯米：走检索/浮现/grounding/锚定全链路，所以能被"想起"。
4. **lively 冒泡零身份**：85% 的主动说话用 `RetrievalResult::default()` 生成，prompt 里连用户画像都没有——主动场景永远说不出"伙伴/爸爸"。
5. **语气零耦合**：关系设定（伙伴/恋人/妹妹/助手）没有映射到说话语气；性格期望与 `[Core Personality]` 重复但无强化。
6. **示例零示范**：`system.txt` 8 个示例全部用"你"，从不叫称呼、不体现关系感 → 少样本反向强化。
7. **关系快照误导**：`trust`/`days_known` 是死列（record_interaction 只更新 conversations/last_interaction；landmark.rs 注释自认 dead column），渲染出 "closeness 100/100, trust 0.0/100, known 0 days" 自相矛盾。

---

## 3. 外部调研（GitHub + Firecrawl）

### 3.1 关键模式来源
- **mem0 "Build an AI Agent That Remembers Your Users"**：① 检索记忆 → 单独 system 消息，**紧贴用户消息**放置；② 指令式 prompt："Use the user's past preferences and facts from the MEMORY section when answering, but do not repeat them verbatim"；③ 同类别新事实覆盖旧事实（recency override）；④ 每次交互后更新记忆。
- **LangMem "How to Manage User Profiles"**：用户画像作为"活的槽位文档"，随对话自动抽取、更新——profile 是过程不是常量。
- **QwenPaw persona 文档**：SOUL.md（行为原则）+ PROFILE.md（身份+用户画像）加载进 system prompt；MEMORY.md **默认不加载**、按需检索——理由正是"避免上下文膨胀"。
- **ACL-2026 "Towards Proactive Personalization of LLMs through Profile Customization"（PersonalAgent）**：把对话拆成单轮交互，动态精炼统一用户画像。
- **arXiv 2304.05371 "Those Aren't Your Memories"**（警示）：长期记忆可被播种错误信息，且召回时 **328%** 更可能当作事实复述 → 记忆卫生是前提（本项目已有 memory_gate，采纳需保持）。
- **MIT News 2026（警示）**：个性化特征会让 LLM 更"讨好/镜像"用户观点 → 需克制、防油腻（呼应本项目"不黏人、不强行乐观"）。

### 3.2 来源清单（一手）
- mem0 官方博客 *Build an AI Agent That Remembers Your Users*：https://mem0.ai/blog/build-an-ai-agent-that-remembers-your-users
- mem0 *AI agent frameworks and how to choose a memory strategy*：https://mem0.ai/blog/ai-agent-frameworks-and-how-to-choose-a-memory-strategy
- LangMem *How to Manage User Profiles*：https://langchain-ai.github.io/langmem/guides/manage_user_profile/
- QwenPaw（agentscope）Persona/SOUL/PROFILE/MEMORY 分层文档：https://github.com/agentscope-ai/QwenPaw
- ACL-Findings-2026 *Towards Proactive Personalization of LLMs through Profile Customization*（PersonalAgent）：https://aclanthology.org/2026.findings-acl.159.pdf
- arXiv 2505.24697 *Towards a unified user modeling language*：https://arxiv.org/html/2505.24697v1
- arXiv 2504.14225 *Benchmarking LLMs for Dynamic User Profiling*（PersonaMem）：https://arxiv.org/html/2504.14225v1
- arXiv 2304.05371 *Those Aren't Your Memories, They're Somebody Else's*（记忆播种警示）：https://arxiv.org/abs/2304.05371
- MIT News 2026 *Personalization features can make LLMs more agreeable*（讨好化警示）：https://news.mit.edu/2026/personalization-features-can-make-llms-more-agreeable-0218
- GitHub 综述：TsinghuaC3I/Awesome-Memory-for-Agents、Applied-Machine-Learning-Lab/Awesome-Personalized-RAG-Agent、Neph0s/awesome-llm-role-playing-with-persona

### 3.3 对本项目的三点启示
1. **身份是基线、记忆是过程**：onboarding 4 值应保留稳定注入（身份），同时镜像为可检索记忆（过程）——两轨并存，与"糯米能浮现而伙伴不能"的对照完全吻合。
2. **近端注意力**：把"你记得X"这类提示放在用户消息附近，比埋在长 system prompt 里有效得多。
3. **反常数**：任何每轮恒定的东西都会被模型习惯化——低频轮转 + 内容轮换是对抗手段（项目已有先例：proactive 的 FACT_REPEAT_WINDOW_DAYS 轮转、memory governance 7 天硬排除）。

---

## 4. 方案（分层实施）

### L0 数据修复（渲染层，5 分钟，零 schema 变更）
- **4a** `format_persona` 关系快照行修正：`days_known` 用 `first_met_date` 实时计算（复用/抽取 landmark.rs::resolve_first_met 逻辑）；`trust` 死列不再渲染（或改为 closeness 推导值），消除 "closeness 100 / trust 0 / days 0" 矛盾。
- **4b**（可选，需用户确认）core persona traits 重种为 Liri 中文维度（`DELETE FROM persona_traits WHERE trait_type='core'` → 重启自动重种），让 `[Persona]` 的 Core personality 与 system.txt 一致。

### L1 身份与行为契约（prompt 层，治"模型愿意表现"）
- **1a 身份动态化**：`SYSTEM_TEMPLATE`（include_str!）首行 "You are 璃 (Liri)" → `build_system_prompt` 时字符串替换为 `你是{pet_name}（璃，一只小狐灵）`，pet_name 为空回退"璃"。示例保持"璃"不变。
- **1b 行为指令区块 `[与你相处]`**（Rust 静态文本，紧跟 [Persona]，非 LLM 生成）：
  - 称呼：{nickname}——在自然处偶尔用一次，不每句都用，不硬塞。
  - 关系设定：{relationship_style} → 语气映射（伙伴=平等老朋友 / 恋人=亲昵但克制 / 妹妹=宠但有分寸 / 助手=尊重距离）。
  - 性格期望：{personality_style} ——用户选的样子就是你最外层的性格，落到具体说话方式（话轻一点 / 偶尔开个小玩笑），别拧着来。
- **1c 示例示范**：system.txt 增 1-2 条带称呼/关系感的示例（克制用法，示范"偶尔用"）。

### L2 记忆化（核心，治"模型能想起"）
- **2a** `complete_onboarding` 时把 4 答案镜像为 active facts（Rust 直接写库，幂等 dedup，不过 extractor、无额外 LLM 成本）：
  | 设定 | category | key | confidence |
  |---|---|---|---|
  | user_nickname | relationship | user_address | 0.85 |
  | relationship_style | relationship | relationship_setting | 0.85 |
  | personality_style | profile | user_wished_personality | 0.85 |
  | pet_name | relationship | pet_self_name | 0.85 |
  - 效果：参与 retrieve 语义召回 → `[Memories]` 渲染 → grounding 锚定 → proactive 锚定轮转 → 可被"忘掉"。用户问"我们是什么关系"→ 召回 → 自然答"你当初说我们是伙伴"。**onboarding 答案从"配置"变成"她会想起的事"。**
- **2b** `soul/review.rs` 关系总结生成时把 relationship_setting 作为 seed 输入——动态关系理解（每 15 episode）从一开始就带着用户设定。

### L3 近端浮现（治"记得"可见、有节奏）
- **3a 轮转近端提示**：非 QA 轮次 ~15%（config 可调，默认关/开）在用户消息前插一条轻量 system 提示，内容四选一轮换：
  `（你记得：{称呼}。这轮可以在自然处用一次，不用每句都用。）` / 关系 / 性格 / 名字 同理。
  利用近端注意力 + 反常数（低频+轮换），零额外 LLM 成本（Rust 概率 + 静态文本）。
- **3b lively 冒泡补身份**：`generate_lively` 的 `RetrievalResult::default()` → 轻量装载（persona core + relationship + user_profile，episodes/facts 留空），~200 tokens 增量，让 85% 的主动说话也知道"小七/爸爸/伙伴"。
- **3c** `MEMORY_QUERIES` 增一条：`"the relationship the user set with you and how they want to be addressed"`——让记忆锚定冒泡偶尔带出设定本身。

### L4 验证（闭环）
- **单测**：format_persona 渲染（pet_name 替换、days_known 修正、`[与你相处]` 区块）；onboarding→fact 镜像幂等（重复 complete 不重复插）；lively 身份装载非空；轮转提示概率与四选一轮换。
- **golden case**：gc_onboarding_identity——设定"伙伴+爸爸+小七"后，golden 对话中回复体现称呼/关系。
- **实跑验收**：新装（或临时清 onboard_completed 重放 4 问）→ 聊 10-20 轮 → 统计 assistant 消息中"爸爸/小七/伙伴"出现率 vs 基线（0/107）。验收线：明显 >0 且不油腻（每 3-5 轮出现一次相关表达为佳）。

---

## 5. 三轮复盘记录

### 复盘 1（合理性：方案是否对症、符合架构）
- 对症：直接命中 7 条失败机制（L1→机制1/2/5/6，L2→机制3，L3→机制3/4，L0→机制7）。对照"糯米 8 次 vs 伙伴 0 次"的实证，L2 记忆化是核心解。
- 架构：L1/L2/L3 全部由 Rust 写状态、LLM 只表达（#1）；无新增 LLM 调用（#8）；轮转与冒泡频率可配（#6）；称呼低频不黏人（#12 沉默也是表达）。
- 唯一疑问（已解决）：fact 镜像与配置重复？——两轨有意并存：配置是身份基线（恒定），fact 是"她记得的"（可想起）。LLM 看到同一信息两次正是"真的记得"的体感来源。冲突时以配置为准，fact 可被 forget。

### 复盘 2（会不会引新问题）
- **extractor 重复抽取/污染**：converse Step 1 的 known_facts 防重复机制已覆盖；镜像 fact 走 Rust 直写 + dedup_insert，不过 extractor。memory_gate 白名单含 relationship/profile，天然放行。✅
- **[Memories] 被恒定 fact 占位**：4 条 fact confidence 0.85 排序靠前，但 compress_system_prompt 超预算时先裁低分——即便裁掉，[Persona] 里身份仍在，不丢信息。✅
- **用户改主意**（"别叫我爸爸了"）：现有事实演化机制处理（新 fact 覆盖 + 可 forget），与糯米/雪碧同类，不新增问题。远期补 DebugPanel 编辑。⚠️
- **近端提示变油腻**：15% 低频 + 提示词明写"不用每句都用" + 示例示范克制用法。可配开关。✅
- **身份替换破坏 system.txt 其他"璃"引用**：只替换首行；[Core Personality] 与示例保持"璃"，语义一致（pet_name 是昵称，璃是本名）。✅

### 复盘 3（有无更有效方式）
- **替代 A（纯 prompt 指令，不做记忆化）**：只加指令+示例，成本最低。但对照"糯米能浮现"的实证，无记忆化则设定永远只是背景，用户主动问"我们是什么关系"时无法自然召回 → 半截方案。❌
- **替代 B（profile 向量化参与检索，替代恒定注入）**：mem0/LangMem 模式。但身份是"基线"不是"情境记忆"——恒定性恰是身份特征；完全检索化会让身份依赖命中率，可能整轮缺失。❌ 不替代，2a 的 fact 镜像已提供检索参与，恒定注入保留。
- **替代 C（每轮都插近端提示）**：比埋在长 prompt 有效，但每轮恒定 = 新常数 → 又习惯化。**收敛为 15% 轮转 + 内容轮换**（3a）——兼得近端注意力与反常数。✅（已并入方案）
- **替代 D（关系引擎：relationship_style 随 closeness 自动演进）**：更接近真人（认识→朋友→亲密），但用户明确设定了"伙伴"，擅自演进违背设定；且是大工程。**列为远期**，可与 relationship_reviews 联动（2b 已为它埋 seed）。⏸
- **替代 E（把 B 链路冷启动访谈答案与 A 链路合并）**：B 链路（喜欢做什么/梦想/开心事）答案走 extractor 进记忆、能浮现（糯米案例）；A 链路走配置、不能浮现。**最有效的正是让 A 也走记忆**——即 2a。方向已确认。✅

**收敛结论**：L0+L1+L2+L3 组合拳，核心是 **L2 记忆化**（把配置变成会想起的记忆）+ **L1 行为契约**（让模型愿意表现）+ **L3 近端轮转**（让表现可见不油腻）。

---

## 6. 不做 / 远期
- **关系引擎**（设定随亲密度演进）：远期，需用户参与定义演进路径。
- **onboarding 编辑 UI**：远期；当前仅重装或改库。可在 DebugPanel 增加 4 值只读展示（低优先）。
- **profile 完全检索化**：不做（见复盘 3 替代 B）。

## 7. 落地顺序建议
1. L0（数据修复，含 4b 需用户确认）
2. L1（身份动态化 + 行为指令 + 示例）
3. L2（fact 镜像 + review seed）
4. L3（轮转提示 + lively 补身份 + query 池）
5. L4 验证（单测 → golden → 实跑）
每步独立可提交、可回退（#6 可关）。
