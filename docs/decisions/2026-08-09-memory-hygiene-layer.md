# ADR: 记忆卫生层（Memory Hygiene Layer）

> 2026-08-09 续⁹。结构性治理记忆三类易复发缺陷（非一次性补丁）。
> 调研 mem0 / Zep(Graphiti) / MemGPT(Letta) 后设计；三次多视角复盘见文末。
> **本文档与实现一致**（复盘后定稿）。

## 背景：三类结构性缺陷（读码定位）

| 缺陷 | 根因（代码） | 表现 |
|---|---|---|
| **A 抽取无校验** | `store_fact` 全盘信任 extractor 输出 + LLM 自打 confidence；extractor prompt 写对了但 LLM 违规 10-20% | "太阳东升西落"conf0.98、"user is asking about my dreams"、知识问答入库 |
| **B 读路径强化** | `retrieve()` 每次**读**都副作用**写** `reinforce()`（+strength、+recall_count）；forget/proactive/**测试** 都触发 | recall_count 刷爆(382/445/446)、strength 饱和钉在 1.0、富者愈富 |
| **C 去重视区** | `converse.rs:94` known_facts 只拉 `preference` 类 | 糯米跨 relationship/preference/profile 碎片化、extractor 看不到 → 重抽 |

> ⚠️ 复盘纠正：原以为 strength"只升不降"。**错**——`db::episodes::decay_strength`（×0.998/天）已在 `loop_runner.rs:309` 每日运行。所以衰减**早就存在**，B 的真正根因是"读路径也强化"，不是"无衰减"。

## 调研结论（决定不造什么）

- **不建知识图谱**（Zep/Graphiti）：39 facts/单用户/成本#8 规模 overkill。我们已有 `valid_from/valid_to/source_episode` = Zep 的 bi-temporal 形状。
- **不上 LLM judge 二次校验**：mem0 V1/V2 的 extract→verdict 引起回归+成本，V3 已砍。业界收敛到**确定性规则闸门**。
- **复用**：mem0 的 REJECT 闸 + 负向规则、ADD-only + 软废弃（我们 `expire` 机制已是）、MemGPT 的"维护关进后台 worker"（我们 consolidation 已是）。

## 设计：两层确定性卫生（LLM 只提议，Rust 校验，#1）

### Part 1 — 写入校验闸门（治 A，新模块 `mind/memory_gate.rs`）

`admits(fact) -> bool` / `filter_facts(facts) -> Vec<FactInput>`，无 LLM、可单测，在 `store()` 写库前调用。三条独立 deny（任一命中即丢弃）：

1. **category 白名单**：仅 `preference/relationship/goal/profile/school/work/health`（与 `extractor.txt:33` 一致）。越界（`pet_dog`/`current_reading`/`geography`）丢弃。
2. **噪声 key 模式**：key 结尾 `_question`/`_gap`/`_knowledge`，或 `belief_in_*` 前缀。
   - **中文反例靠 key 抓**："太阳东升西落" 的 key 是 `knowledge_question` → 命中 `_question`。（value 黑名单是英文，纯中文 trivia 配干净 key 是已知残留缺口，见"不做"。）
3. **噪声 value 模式**（英文 + 对齐 proactive `is_anchorable_fact`）：`asked about`/`asking about`/`user asked`/`user is asking`/`is asking about my`/`does not know`/`doesn't know`/`curious about`/`user is busy`/`busy with work`。

> 安全性：糯米在 `relationship`(pet_name/pet_type/pet_age) + `preference`(pet_dog) 已有副本，丢弃越界类那条冗余不丢信息。

### Part 2 — 检索纯化（治 B，根因修复，**零签名变更**）

1. **`retrieve()` 删除 reinforce 副作用 → 纯读**。reinforce 块（原 retrieval.rs:130-138）移除。
2. **新增 `reinforce_top(db, episodes)` 辅助**（retrieval.rs），仅 genuine-recall 调用方使用：
   - `converse()`（对话召回，非 QA 模式）—— 显式调用。
   - `proactive`（记忆锚定气泡 / 欢迎回家 / 孤独关怀，3 处 retrieve 后）—— 显式调用。
   - **forget / 测试 / embedding_ab / questioning → 纯读，不强化**（retrieve 已无副作用，自动满足）。
3. **不新增衰减**：`decay_strength`（0.998/天）已在 `loop_runner:309` 每日运行。strength 饱和由"retrieve 纯化（止源头膨胀）+ 一次性迁移重置（解现有饱和）+ 现有日衰减（自然回落）"共同解决。

> **为什么不加 `reinforce: bool` flag？** 复盘选了"纯读 + 调用方显式 reinforce"：retrieve 回归纯函数（无副作用语义更清晰），且签名零变更 → forget/tests/embedding_ab 等多个调用点无需改（避踩坑#4）。genuine-recall 的判断权留在调用方（它最清楚这次检索算不算"真实回忆"）。

### Part 3 — 去重视野（治 C）

`converse.rs` known_facts 从 `get_by_category("preference")` take(20) 改为 `get_all_active(30)`（按 `mention_count DESC, confidence DESC` 排序）。extractor 看得到跨类的糯米/目标/画像，不再重抽。

## 数据治理（一次性，用户 #2）

用 Python sqlite3 对 `%APPDATA%/DesktopPet/desktop_pet.db` 跑（非 blanket 重放 gate，**显式** expire 已知噪声，避免误杀 `current_reading`）：

1. **expire 噪声 facts**：value/key 命中上述模式 → `valid_to=now`。预期清掉 ~8 行（知识问答/自我语境/越界类）。
2. **strength 重置**：`UPDATE episodes SET memory_strength = importance WHERE is_landmark = 0`——解除测试期累积的饱和，回到 importance 基线；之后由 genuine recall 重新强化、由日衰减自然回落。recall_count 不动（不参与评分，仅诊断）。
3. **保留** `current_reading`（genuine 但瞬时，gate 未来会拦其类别——可接受，episode 已记录）+ 糯米 relationship/preference 副本。

## 改动清单（已实施，surgical）

- 新 `mind/memory_gate.rs`（`admits`/`filter_facts` + 6 单测）。
- `mind/mod.rs`：注册 `pub mod memory_gate`。
- `mind/store.rs`：fact 写库前过 `memory_gate::admits`，记录 reject 数。
- `mind/retrieval.rs`：删 reinforce 块 → 纯读；新增 `reinforce_top`；`test_strength_reinforcement` → `test_retrieve_is_pure_read` + 新增 `test_reinforce_top_strengthens`。
- `mind/converse.rs`：known_facts 全类(30)；retrieval 后 `reinforce_top`（非 QA）。
- `pending/proactive.rs`：3 处 retrieve 后 `reinforce_top`。
- `mind/forget.rs`：更新 stale 注释（retrieve 已纯读）。
- `tests/golden_conversations.rs`：`gc_008` → 纯读契约。
- `tests/embedding_ab_harness.rs`：更新 stale 注释（fresh-DB 不再为防 strength 串扰所必需，留作防御性隔离）。

**验证**：`cargo test --lib`（287 passed）+ `--test golden_conversations`（29 passed）+ `--no-run`（17 个测试二进制全编译，零签名破坏）。

## 不做（复盘收敛）

- **知识图谱 / NLI 模块**：overkill（#8 成本）。
- **LLM judge 二次校验**：翻车 + 成本（mem0 V3 已砍）。
- **新衰减子系统 / importance 地板**：`decay_strength` 已存在且有效，无过衰减证据；地板是治未病且可能保噪音。YAGNI。
- **`enable_memory_gate` kill-switch**：gate 与 `dedup_insert`/`expire_old` 同属"零成本确定性 ingest 闸门"（后者也无 toggle）；#6 的 kill-switch 专给**昂贵/LLM 能力**（reflection 等）省成本，gate 无成本可省。threading config 进 `store()` 是踩坑#4 级签名动荡，不值。
- **纯中文 trivia 配干净 key 的残留缺口**：观测到的噪音均被 key 抓住；若未来出现更隐蔽误抽，再加可选 LLM judge flag（#6）。

---

## 三次多视角复盘（设计定稿前）

**复盘 1（架构/正确性）**——关键纠正：Defect B"无衰减"为**假**，`decay_strength` 已每日运行；据此砍掉新衰减子系统。另指出 value 黑名单全英文漏中文反例 → 由 key 黑名单兜住（`knowledge_question`）；`test_strength_reinforcement` 会挂 → 已改纯读断言；缺 kill-switch 违 #6 → 评估后以 rationale 裁掉（见"不做"）。

**复盘 2（回归/副作用）**——签名零变更确认（仅删 retrieve 内部副作用 + converse 内部加 reinforce）。两个固定测试断言会失败（`retrieval.rs:485` + `gc_008`）→ 已改纯读契约 + 新增 `reinforce_top` 单测保覆盖。迁移误杀 `current_reading` → 改显式 expire（非 blanket）。stale 注释（forget/embedding_ab）→ 已更新。proactive 三处 genuine-recall → 加 reinforce_top。

**复盘 3（小马尾/更优解）**——砍 ~40% 代码：filter_facts 内联点确定、`reinforce_top` 替代 flag、衰减子系统全砍（F4：retrieve 纯化已从源头切断非对话膨胀，衰减是测试侧问题 YAGNI）。known_facts 全类保留。
