# 主动冒泡治理方案（Proactive Bubble Governance）

> 2026-08-14。用户反馈：① 主动消息频率太高；② 不需要那么多消息都带记忆；③ 带出的记忆多半是糯米/实习、且多为问句；④ 同一条记忆绝对不能多次浮现；⑤ 时间错位（记忆记的是"今天在找实习"，隔天冒泡却说"你说今天在找实习"）。
> 本文档 = 盘点 + 根因（代码/数据证据）+ 解决方案 + 实施清单。对齐 Architecture-Principles（#1 LLM只表达 / #6 可关可调 / #8 成本 / #11 可观测 / #12 沉默也是表达）。

---

## 一、主动消息路径盘点（8 条独立触发）

| # | 路径 | 触发条件 | 频率门控 | 带记忆？ |
|---|---|---|---|---|
| 1 | 前端 5min 轮询 → `check_proactive` → `proactive_bubble` | 每 5min；deep-focus 抑制、closeness≥20 | `min_interval_secs`=1800（内存态） | 70% lively / 30% 记忆（有 pending 时 100% 记忆） |
| 2 | `loop_runner` medium_tick → `proactive-prompt` | pending 到期，30s 内必发 | **无**（不查 last_bubble） | 记忆（pending 锚） |
| 3 | presence 转换 → `welcome-back` | 离开 >5min 后回来 | **无**（自身转换条件 + 30s 内刚聊过守卫） | 记忆（可选锚，几乎总是挂） |
| 4 | lonely-nudge | loneliness>0.6 + closeness≥20 + 未聊 120s | 自身 30min 线程本地 cooldown | 记忆（可选锚，几乎总是挂） |
| 5 | ritual 早安 | 每天 Morning/Afternoon + Active | 一天一次（日期持久化） | 记忆（可选锚，几乎总是挂） |
| 6 | idle sigh（前端 5s 轮询） | mood 疲惫/难过 + 8% 随机 | 无 | 否（"呼…"字形） |
| 7 | 启动欢迎（lib.rs 2s 后）+ FIX-J 2s fallback | 每次启动 | 无 | 否 |
| 8 | `app-status` resumed / recovery | 挂起恢复 / 崩溃恢复 | 罕见 | 否 |

**关键结论**：只有路径 1 受 30min 全局门控，且 `last_proactive_bubble` 是**内存态**（重启清零 → 每次启动后 5min 内必冒一次）；路径 2/3/4/5 各自独立门控、互不感知，且**4/5 条路径都带记忆锚**。这就是"频率不低 + 带记忆的消息多"的结构性原因。

---

## 二、根因（代码 + 实库数据证据）

### R1 无统一冒泡预算
- `commands.rs:525-544`：`last_proactive_bubble` 只被 `check_proactive` 更新（占位在 greenlight 时）；`proactive_bubble` / `welcome_back_bubble` / `lonely_bubble` / `ritual_bubble` 四个命令**都不检查也不更新**它。
- `lib.rs:116`：初始化为 `Mutex::new(None)`（内存态，重启即失）。
- `loop_runner.rs`：medium_tick 的 pending 检查（:97-128）、presence 转换（:187-227）、lonely（:249-305）、goodmorning（:311-363）全部独立 emit，互不感知。
- 叠加后果：回来（welcome）→ 5min 后轮询（lively/记忆）→ loneliness 高时再 lonely → 疲惫时每 ~62s 一次"呼…"。用户体感"频率并不低"属实。
- 另有并发竞态：pending 到期时 medium_tick emit `proactive-prompt` **和** 5min 轮询的 `check_proactive`（Rule 4 返回 followup）**双路都调 `proactive_bubble`**，`generate()` 里 `pending_due.first()` 在任一路 mark_triggered 前都读得到 → 同一条 followup 两次 LLM 生成（气泡互相覆盖）。

### R2 记忆锚定路径过多
- `proactive.rs:202-216`：lively 70 / 记忆 30。
- `generate_welcome_back` / `generate_lonely_bubble` / `ritual.rs generate_goodmorning`：**锚几乎总是挂**（`sample_anchorable_fact` → `sample_surface_anchor`，只在两者皆空才无锚）。
- 实测：4/5 冒泡路径带记忆锚 → 用户"不需要那么多消息带记忆"。

### R3 同一条记忆可反复浮现（无硬去重）
- **facts 零 surfaced 追踪**：`facts` 表无 `surfaced_count` / `last_surfaced_at`；`sample_anchorable_fact` 权重 `1/(1+mention_count)` 只在用户**再次提及**时变化，冒泡浮现过也不降权（`proactive.rs:664-689`）。实库 24 条 active fact：糯米系 5 条（pet_name/pet_type/pet_age/pet_dog/pet_dog_food_preference）+ 实习 2 条（internship_preparation/emotion_anxiety）+ 面试 1 条，全部 mc=1 → 权重 0.5，合计约占抽取池 ~40% → "多半是糯米/实习"。
- **episodes 冷却被 reinforce_top 批量打穿**：`generate`/`generate_welcome_back`/`generate_lonely_bubble`/`ritual` 都在抽样前调 `reinforce_top(db, &retrieval.episodes)` —— 对**全部 top-8** 写 `recall_count+1, last_recalled_at=now`，而非只对抽中的锚（`retrieval.rs:194-201` 被四处调用）。结果：一次冒泡把 8 条 episode 全部标记"刚被召回"→ `sample_surface_anchor` 的 12h 冷却因"全部在冷却 → 放宽"分支（`retrieval.rs:243-246`）永远失效 → softmax 仍偏向最强记忆。实库证据：多条 episode `recall_count` 20~124、`last_recalled_at` 集中在同分钟（如 13:46:23 四条同时）。
- 内存池小（24 fact + ~30 episode）+ 无"已浮现"记录 → 同一记忆反复被抽。**"绝对不能多次浮现"在现状下无任何机制保证。**

### R4 时间词错位（"你说今天在找实习"）
- 抽取端：`extractor.txt:12` 有 deictic 软规则（"我今天面试了"→"去面试了"），但 LLM 遵守不稳定，且**存量记忆早于该规则**。实库现存：`用户今天读了一本好书。`（今天）、`User completed animations for two actions today`（today）、`用户昨天带宠物狗糯米去看了流浪狗`（昨天）。
- 浮现端：锚**原文**喂进 prompt——`proactive.rs:269-272` `"你想起来的只有这一件：{}"`、`welcome_back :491` `"你想起 ta 之前跟你提过的事：{}"` → LLM 原样复述"你说今天在找实习"。即使记忆是 7 月 26 日记的，"今天"两字照抄 → 时间对不上。
- grounding_guard 盲区：claim patterns（`grounding.rs:399-408`）只有"你说过/你之前说"，匹配不到"你说今天…"；且即使匹配上，锚本身在 retrieval 里，window 包含检查照样判 grounded → 这类错误永远漏网。

### R5 问句偏多
- 记忆锚 prompt 的"围绕它原意来聊 / 轻轻关心一句 / 轻轻带一句"天然诱导问句；`system.txt:14` 人格"你常把现在和过去连起来（你上次说…我记得你…）"也鼓励追问；lively prompt 允许"一个突然冒出来的小疑问"。无系统化的"可不问"约束注入冒泡路径（对比对话路径的 engage 已有续⁶ 改法）。

---

## 三、解决方案

### A. 全局冒泡预算（唯一共享门控）⭐ 治频率
1. **`last_bubble_at` 持久化**：写入 `app_config`（KV，仿 `last_goodmorning_date` 模式），启动读回。重启不再清零 → "每次启动 5min 后必冒"消失。
2. **共享预算助手** `bubble::try_occupy_budget(state/db, now) -> bool`：原子 check-and-set（距上次 < `min_interval_secs` → false 不占位；通过 → 写 `last_bubble_at=now` 返回 true）。**所有 4 个后端 emit 点**（pending / welcome-back / lonely / ritual）和 **4 个 bubble 命令入口**（`proactive_bubble`/`welcome_back_bubble`/`lonely_bubble`/`ritual_bubble`）统一过它——emit 端拦一次、命令端兜底一次（堵住并发双路竞态：先占者胜，后到者返回 None 不生成）。
3. **默认间隔 1800 → 3600s**（60min，`config [proactive] min_interval_secs`，可调）。
4. 早安 ritual：日期仪式一天一次，**豁免** interval 检查但仍**占位**（早安后 60min 内其它冒泡静默，最像真人）。

### B. 记忆浮现：硬去重 + 轮转（Round-Robin）⭐ 治"重复浮现"
1. **migration v5**：`facts` 加 `surfaced_count INTEGER NOT NULL DEFAULT 0` + `last_surfaced_at TEXT`。
2. **锚选择改为确定性轮转**（替代加权抽样）：
   - facts：按 `(surfaced_count ASC, last_surfaced_at ASC)` 排序，取**最久没浮现的**可锚 fact（`mention_count` 只做同序 tiebreak；confidence≥0.7 过滤保留）。
   - episodes：复用 `last_recalled_at`——取 `last_recalled_at IS NULL`（从未浮现）优先，其次最久远的；12h 冷却从"放宽重抽全部"改为"放宽 = 按最久未浮现轮转"（`sample_surface_anchor` 的 relax 分支改语义）。
   - **硬窗口**：同一锚 7 天内绝不重复；池子轮完一轮后才允许回到最早的一条（隔周再提一次 = 真人行为，可接受）。
   - **锚池空了 → 降级 lively，绝不重复旧记忆**（generate 已有无锚→lively fallback，保留）。
3. **修 reinforce_top 滥用**：proactive 四处（generate / welcome_back / lonely / ritual）改为**只 reinforce 被抽中的那条锚**（抽中后写 `surfaced_count+1, last_surfaced_at=now` / episode 走既有 `last_recalled_at`）。`converse.rs:302` 的对话召回保留现状（用户正在聊相关话题，top-k 均相关）。
4. 每次冒泡成功 = 该记忆 `surfaced_count+1` → 权重/轮序自然衰减 → 实库里"糯米/实习"每轮只浮现一次。

### C. 记忆比例下调 + 问句克制 ⭐ 治"带记忆多、全是问句"
1. lively/记忆 **70/30 → 85/15**（`config [proactive] memory_bubble_ratio: 15`）。
2. welcome-back / lonely / goodmorning 的锚改为**概率挂**：`P(anchor) ≈ 0.25` 且只从"从未浮现 / 7 天以上未浮现"的 fresh 池取；否则纯情感招呼（无锚不调 retrieve，还省 embedding 成本）。
3. 四份冒泡 prompt 统一加"可不问"指令：`这条不一定要问问题——大多数时候就是一句带着温度的陈述；真的好奇最多一个问句，别追问。`（镜像对话路径续⁶ 的 engage 改法）。lively prompt 的"小疑问"降级为"极偶尔"。
4. idle sigh 8% → 3% + 5min cooldown（疲惫时"呼…"也算气泡噪音，可选）。

### D. 时间词中和 ⭐ 治"你说今天在找实习"
1. **浮现端（关键、零 LLM 成本）**：新增纯函数 `neutralize_deictic(text) -> String`（Rust 正则剥离 `今天/昨天/明天/今早/今晚/上周/这周/下周/最近/刚刚/前天/后天` 等相对时间词：`"今天在找实习"→"在找实习"`），所有锚进 prompt 前调用；同时把锚的记忆日期以参照形式注入：`（这是 ta {date} 提到的事）`——LLM 有了正确时间参照，可自然说"你之前说在找实习"，不再照抄错词。
2. **抽取端**：`extractor.txt` 加硬例子 `"今天在找实习" → episode.summary = "在找实习"`（正例强化，非新增机制）。
3. **grounding_guard 补模式**：claim patterns 加 `"你说"` 变体（防 LLM 自造"你说今天…"）。
4. **一次性数据治理**：脚本仿 `scripts/migrate_memory_hygiene.py`，扫描现存含 deictic 词的 fact.value / episode.summary，剥离相对时间词（备份先），并顺手把"用户今天读了一本好书"这类旧机器味 summary 归一。治理后"今天在找实习"类记忆永不再现错词。

### E. 可观测（#11）
- DebugPanel 新增「冒泡预算」分区：`上次冒泡 {type} @ {time}（{elapsed} 前）` + `min_interval`；「记忆浮现」列：`surfaced_count / last_surfaced_at`——用户能自查"为什么这条冒了 / 为什么还没冒"。

---

## 四、实施清单

| 文件 | 改动 |
|---|---|
| `src-tauri/migrations/005_fact_surfacing.sql` | 新：facts 两列 + v5 登记 |
| `src-tauri/src/db/schema.rs` | v5 分支（接在未提交的 v4 之后） |
| `src-tauri/src/db/facts.rs` | struct 加两字段 + SELECT/INSERT/UPDATE 同步 + `bump_surfaced` |
| `src-tauri/src/pending/budget.rs`（新） | `try_occupy_budget`（app_config 持久化 + interval 判定 + 占位） |
| `src-tauri/src/mind/deictic.rs`（新） | `neutralize_deictic` 纯函数 + 单测 |
| `src-tauri/src/pending/proactive.rs` | ① 锚选择改轮转（fact/episode）；② 只 reinforce 抽中的锚 + bump_surfaced；③ prompt 加"可不问"；④ 锚进 prompt 前 neutralize + 注入日期参照；⑤ 记忆比例 85/15 可配 |
| `src-tauri/src/commands.rs` | 4 个 bubble 命令入口过 budget；`check_proactive` 用共享 budget（替代内存 last_bubble）；`proactive_bubble` 内 neutralize 兜底 |
| `src-tauri/src/lifecycle/loop_runner.rs` | pending / welcome-back / lonely 三个 emit 点过 budget（ritual 豁免但占位） |
| `src-tauri/src/config.rs` | `ProactiveConfig` 加 `memory_bubble_ratio`；默认 `min_interval_secs` 1800→3600 |
| `src-tauri/src/soul/ritual.rs` | 锚概率 0.25 + fresh 池过滤 + "可不问" + neutralize |
| `src-tauri/src/mind/grounding.rs` | claim patterns 加 "你说" |
| `src-tauri/resources/prompts/extractor.txt` | deictic 硬正例 |
| `src-tauri/resources/prompts/*`（lively 等 4 处） | "可不问"指令 |
| `src/App.tsx` | idle sigh 0.08→0.03 + cooldown（可选） |
| `src-tauri/src/.../DebugPanel.tsx` 对应后端 | 冒泡预算 / 记忆浮现分区 |
| `scripts/migrate_deictic.py`（新） | 存量 deictic 记忆治理 |
| 测试 | budget 原子占位 / 轮转不重复（7 天窗口）/ neutralize / 只 reinforce 锚 / 记忆比例默认值 / 既有 harness 适配（`BubbleOutcome` 无变化则不破） |

## 五、参数默认值（都可配）

| 参数 | 现状 | 新默认 | 位置 |
|---|---|---|---|
| `min_interval_secs` | 1800 | **3600** | `[proactive]` |
| `memory_bubble_ratio`（lively 占比） | 30（记忆） | **15** | `[proactive]` 新 |
| welcome/lonely/goodmorning 挂锚概率 | ~100% | **25%**（仅 fresh 池） | 代码常量 |
| 同记忆重复硬窗口 | 无 | **7 天** + 轮完一轮才回 | 代码常量 |
| idle sigh 概率 | 8% | **3%** + 5min cooldown | `App.tsx` |

## 六、验证
1. `cargo test --lib`（新增单测全绿）+ `cargo check --tests` + `tsc`。
2. 单测级：轮转 100 次模拟 10 条记忆 → 每条恰好一次、7 天内零重复；`neutralize_deictic` 全词表；budget 并发双路先占者胜；reinforce 只写锚。
3. 实跑（dev）：回办公室触发 welcome-back 后 60min 内无其它冒泡；记忆气泡内容出现非糯米/实习项；同一条记忆两周内不重复；冒泡不再出现"今天/昨天"错词；DebugPanel 看到 surfaced_count 增长。
4. release rebuild（先 `taskkill //IM desktop-pet.exe //F`）+ 桌面快捷方式重启。

## 七、实施记录（2026-08-14 已落地）

**实现偏差（比计划更保守的两处）**：
1. **budget 只在 emit 端 check-and-occupy，不在 4 个 bubble 命令入口再拦**——若命令端也占位，welcome-back 流程（loop_runner 先占位 → 前端 listener → 命令）会二次占位失败 → 欢迎气泡被吞。所有命令入口都只由唯一 emit 路径驱动，emit 端原子占位已堵住并发双路竞态（`Mutex<Connection>` 使 check-and-write 单闭包原子）。
2. **启动欢迎（lib.rs 2s 问候）不占位**：重启后第一个 5min 轮询冒泡仍会出现（新会话第一泡，自然），之后严格 60min 间隔——避免"重启后一小时全静默"。

**落地清单**（commit 见 git log，lib 384 passed / check --tests ✅ / tsc ✅）：
- 005 迁移（`facts.surfaced_count` + `last_surfaced_at`）+ schema v5 + facts.rs `bump_surfaced` + 全 Fact 构造点（含 4 个 harness 种子）。
- `pending/budget.rs`：`LAST_BUBBLE_KEY`（app_config 持久化）+ `try_occupy_budget`（原子 check-and-occupy）+ `occupy_budget_always`（ritual）+ 3 单测。
- `mind/deictic.rs`：`neutralize_deictic`（31 词）+ `format_memory_date` + 4 单测。
- `proactive.rs`：`sample_anchorable_fact` 改确定性轮转（fewest-surfaced → oldest → least-mentioned，7 天硬排除，全窗口内 → None 降级 lively）；`sample_surface_anchor` 冷却 12h→168h + 全冷却时按 last_recalled_at 最老优先（绝不重复最新）；四路径只 `record_anchor_surfaced` 抽中的锚（fact bump / episode reinforce 单条）；`present_anchor` neutralize + 注入"（这是 ta X月X日 提到的事）"；due/欢迎/早安 prompt 加"可不问"；welcome/lonely/goodmorning 挂锚概率 25% 且仅 fresh 池；`generate` 第 5 参数 `memory_ratio`（config 默认 15，85% lively 碎碎念）。
- `commands.rs`：AppState 删内存 `last_proactive_bubble`；`check_proactive` 走 `try_occupy_budget`；`proactive_bubble` 传 `memory_bubble_ratio`；DebugSnapshot 加 `last_bubble_at`/`next_bubble_in_secs`；DebugFact 加 `surfaced_count`/`last_surfaced_at`。
- `loop_runner.rs`：pending / welcome-back / lonely 三个 emit 点过 `bubble_budget_ok`；早安豁免但 `occupy_budget_always`。
- `config.rs`：`memory_bubble_ratio` 默认 15；`min_interval_secs` 默认 3600。
- `ritual.rs` / `grounding.rs`（"你说今天/昨天/明天/你说你"）/ `extractor.txt`（deictic 硬规则）/ `App.tsx`（叹气 0.03 + 5min cooldown）/ `DebugPanel.tsx` / `scripts/migrate_deictic.py`（存量治理，dry-run 默认）。
- 运行时 AppData config 无 `[proactive]` 段 → 新默认直接生效；可加 `[proactive] min_interval_secs=…` / `memory_bubble_ratio=…` 调。

**实跑待办**：① dev 实跑 60min 间隔 + 内容多样性 + 同记忆 7 天不重复；② `python scripts/migrate_deictic.py --apply` 清存量"今天/昨天"；③ release rebuild（`npx tauri build --no-bundle`，先 taskkill）。
