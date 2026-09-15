# 交互「不死板」：分支核对后的结论（原方案降级为增量提案）

> 日期：2026-09-15（**二次修订**：发现 `念头` 分支后重写）
> 状态：**原方案的机制 ①③④ 作废——`念头` 分支已实现且更完整**；本文件现在只主张 §D 的两件事，并保留 §A 的评审核实与 §C 的自我纠错
> 权威来源（①③④ 请直接读它，不要读本文）：`念头` 分支 `docs/plans/2026-08-27-thought-stream-plan.md`（v3，已吸收一轮 GPT 评审）
> 关联：`docs/plans/2026-08-27-companionship-gap-and-belief-layer.md`（根因层）、`docs/Architecture-Principles.md`

---

## A. GLM 评审逐条核实（2026-09-15）

| GLM 断言 | 核实 | 证据 |
|---|---|---|
| `last_bubbles_clause` 把防重复与防延续混在一条禁令里 | ✅ 成立 | `proactive.rs:195` 逐字命中 |
| 1h 硬间隔 + 记忆气泡 15% | ✅ 成立 | `config.rs` 默认值 |
| 续³² 把记忆气泡 30%→15% | ✅ 成立 | HANDOFF |
| 新机制均未实施（当时） | ✅ 成立（master 上） | grep 无 `pick_motivation`/`pet_events`/`open_thread`/`mood_referent` |
| `Intent` 21 处构造点 | ⚠️ 口径差异，非分歧 | 原始 grep 21 命中 = 18 处字面量构造（另 3 处是结构体定义/impl 行/fn 签名）+ 3 处测试。"别动它"的结论不变 |
| 索引里的 `soul/stream.rs` 是幽灵、可能是幻影 | ❌ **GLM 错** | 它是 **`念头` 分支上的真实文件**（1156 行），见 §B。GLM 查证时我的重建**已经跑完**，所以它"查无此文件"——顺序造成的假否证 |
| "零新增 LLM 调用"有水分（retrieve = embedding API） | ⚠️ 半对 | embedding 是**本地 BGE-M3 ONNX**（无 API token，只有 CPU/内存）；但分支方案诚实标了净增 ~10-35 次 flash/日，我原先那句确实过满 |
| "她除时间外没有任何输入" | ❌ **我错**（GLM 对） | 见 §C.1。`perception/` 有完整输入流 |
| CV≥0.8 是坏指标 / 现状日均远低于 8 | ✅ 成立（GLM 对） | 见 §C.2：CV 方法论有误，且"现状"根本测不到 |

**GLM 最有价值的一条**是"真实感知流才是金矿，不要虚构 `楼下的车声`"。这条不仅对，而且**分支已经照这个思路做了**（environment 环形缓冲 → 素材行念头），所以我原方案里"percept 用池子虚构"是**双重错误**（前提错 + 有更好的现成源）。

---

## B. 关键发现：`念头` 分支 = 已完成的第二条第开发线

| 事实 | 值 |
|---|---|
| 分支 | 本地 `念头` + `remotes/origin/念头`（另有 `qa/fresh-user-onboarding`） |
| merge-base | `b5ad4c3` **2026-08-27 11:41**（此后分叉） |
| 提交数 | **念头 +25 / master +8** |
| 规模 | 51 文件、**+7350 / −1362** |
| 核心文件 | `src-tauri/src/soul/stream.rs`（**1156 行**）、`db/thoughts.rs`、`migrations/008_thought_stream.sql`、`tests/thought_stream_harness.rs`(256) / `bubble_nature_harness.rs` / `env_bubble_harness.rs` |
| 一并携带 | **应用内更新推送**（检查/下载/签名校验/覆盖安装）、动画与 UI 一批（`spineIntent.ts` +433、`liriAssetPatch.ts`、气泡锚定、输入框）、Debug Panel 念头流分区、"她用小 LLM 调用给自己起名" |
| 最后一条提交 | `chore: checkpoint before switching to master` ← **有人主动把它停在分支上** |

**架构（v3 三层）**：`ingest`(Rust, 0 LLM) → **硬门**(Rust, 零 LLM) → **动机评分**(Rust, 分量可展开) → **flash 静默评估** → **统一渲染器**(主模型, thinking 关)；种子是结构化心理状态 `{stimulus, emotion_tone, relation_hint}` 而非台词；`state=unspoken`（想说但忍住，salience 保留）+ `evolved_from`（演化链）+ **未回应退避**（2^n 封顶 8×）+ 安静时段 + skipRecent 30min + 48h TTL + 池容量 12 + `engine=legacy` 回滚。

**它的喂流来源（8 条，全部零 LLM）**：environment（App/文件/项目切换、回来）· body/self_state（时段边界、情绪越阈、久坐长静默）· memory（selector 保留）· relationship（"他通常晚上进入深度开发"——熟悉感来源）· temporal（事件日、facts 有效期）· ritual/pending · reflection（夜间整理写入流）· 空闲成形（flash 把 3-5 条原料合成 2-3 条种子，≥15min 节流）。

> ⚠️ **这 8 条里没有一条是"她自己今天做了什么"**——她的念头永远关于你、环境或时间。这就是 §D1 的立足点。

**运行证据（它真的跑过）**：`%APPDATA%\DesktopPet\desktop_pet.db` 的 `schema_migrations` 有 **8 行**（第 8 条写入于 **08-28 01:42**），且 `thought_stream` 表存在——而 **master 没有 008 迁移文件**。

**索引"幽灵"真相（修正我 09-15 早些时候的错误结论）**：那些文件**既不是索引 bug，也不是代码丢失**——CodeGraph 索引是在 `念头` 分支被检出时建立的，切回 master 后没重建（209 文件 vs master 实际 163）。旧索引里另外 28 个 JS / 12 个 Python 文件是那时删掉的调试脚本。

**新风险**：`thought_stream` 表 + 第 8 条迁移记录留在你的库里，而 master 代码没有 008 → **master 构建跑在这个库上属于 schema 未定义状态**，需实测确认迁移校验是否会报错（本次未验证）。

---

## C. 我原方案的两处事实错误（已作废 / 已修）

### C.1 "她除时间外没有任何输入" —— 错

真实输入流（GLM 指出的，已逐条验证）：

- `src-tauri/src/lib.rs:237-246`：`perception::environment::start(enable_window)`——**每 3 秒**采前台 App/标题，快照 diff 合成语义事件，进程内**环形缓冲**。
- `perception/environment.rs`：`EnvSample` / `diff()` / `ring()` / `recent_events()` / `recent_summary()`；事件含 `AppChanged` / `FileHintChanged` / `ProjectHintChanged` / `PresenceReturned`。
- 另有 `perception/focus.rs`（深专注检测）、`cursor.rs`、`presence.rs`、`time.rs`、`window.rs`、`title.rs`。
- **已进对话 prompt**：`mind/converse.rs:702` `build_environment_section()`；`commands.rs:1918-1923` 暴露 hints/summary。
- **隐私边界（既定决策，不得违反）**：`docs/plans/2026-08-17-environment-filesystem-plan.md:91`"进程内存环形缓冲，**永不落盘**（窗口标题历史是敏感数据）"；窗口感知关闭时**标题完全不采集**（原则 #6）；进 LLM 的只有脱敏概括（`sanitize_env_text`）。

⇒ 因此机制二的 percept 类**应来自这条真实流**（"你今天开电脑比平时晚""连着三小时没动"），不是虚构池子。

### C.2 判据 4（CV ≥ 0.8）—— 方法论错误且无法测量

- 夜间/离开的长间隔天然抬高 CV；把它当**目标**会鼓励奇怪调度。GLM 对。
- 更糟的是我写的"现状 ≈0"**从未测量**：你当前库 `bubble_log` **0 行**（`episodes`/`facts` 也是 0，`conversations` 仅 10 条）——这是重装（续⁵⁷ E:\Liri）后的**新库**，测不出日常频率。
- **替代指标见 §D2**：直接数"相邻间隔 <10min 的次数/周"（burst 计数），CV 只作参考。

---

## D. `念头` 分支未覆盖、本文档仍主张的两件事

### D1 ⭐ `pet_events`：她自己的生活（唯一真正的空白）

**为什么这仍是空白**：分支的 8 条喂流全以用户/环境/时间为对象；Her 的成长弧线（她读书、写曲子、出书）恰恰来自"他不在时她也在活着"。companionship 文档的缺口 1-5 也指向这一层。

**设计（吸收 GLM 的素材规则）**：

| 类 | 来源 | 说明 |
|---|---|---|
| `activity`（她做了件小事） | 池子 JSON | 无害虚构，她自己的事 |
| `realization`（想通一件事） | 池子 JSON | ≤1/3 且必须具体到事 |
| `percept`（注意到的细节） | **真实 environment 流** | 不虚构；沿用 sanitize + untrusted 管道 |

- **素材编辑规则**（防止 LLM 写池子必然同质化 + 伪深沉）：① 只写动作与感官细节，**禁止心情结论**（"有点想你"绝不进池子——表达是 LLM 的事）；② `realization` ≤1/3 且必须具体；③ 每条 ≤12 字；④ `percept` 类不写池子。
- **落库**：新迁移（**合并 `念头` 后应为 `009_*`**）+ 上限裁剪 + 插入时 prune；`kind/tag/used_at` 供由头选取与防复用。
- **消费**：① 作为念头流的种子源之一（`origin=pet_life`，与分支 ingest 同构，**不新增 LLM 调用**）；② 对话上下文尾部动态段落 ≤2 条；③ 7 天内可被回忆引用。
- **开关**：`[pet_life] enable / events_per_day / quiet_hours`（原则 #6）。
- **不做**：池子不写用户信息；不产生"需要用户配合的事"（那是索取）。

### D2 客观验收（替代我原先的主观"跑一天看看"）

1. **burst 计数**（主指标）：`bubble_log` 相邻间隔 <10min 的次数 ≥N 次/周。
2. **7 天生成文本质量（自动、便宜）**：去重率 + bigram 多样性阈值——给"内容像不像模板"一个客观判据，而不是感觉过关。
3. **人工盲评**：混入旧气泡，判断"像她突然想到什么随口说一句"（与分支方案 §六.3 一致）。
4. `pet_events` 专有：**问她"你刚才在干嘛？"必须答出库里真实存在的那条**（这一句同时验了 D1 与反幻觉）。

---

## E. 待决策（阻塞本文档定稿与 D1 落地）

1. ⭐ **`念头` 分支怎么处理**：合并进 master / 继续在分支上开发 / 弃用？——这一条决定了 D1 用哪个迁移号、在哪个基线上实现，也决定"应用内更新推送"和那批动画改动是否入主线。
2. 是否采纳分支已实现的 ①③④（**建议：采纳**，它的"未回应退避 + 安静时段 + legacy 回滚 + 成本核算"比我原方案完整）。
3. `pet_life` 池子：由我起草 30-40 条供你改？还是等你/美术给风格？（若起草，按 D1 规则）
4. 真实感知进气泡由头的隐私边界复核（沿用现状即可，但这是你的产品决策）。

---

## F. 不做（精简 Kill List）

- ❌ 不在 master 上重造 `thought_stream` / 三层决策（分支已有）。
- ❌ 不改 `Intent` 结构（构造点众多 + harness 同步成本）。
- ❌ 不用虚构池子产 `percept`（改用真实 environment 流）。
- ❌ 不写 CV 门槛（改 burst 计数）。
- ❌ 不动身体层（Spine 姿态）与 TTS（各自独立工单）。
