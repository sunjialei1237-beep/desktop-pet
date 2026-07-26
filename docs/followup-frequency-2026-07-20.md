# 反问频率控制（Follow-up Question Frequency Control）

## 背景

用户实测确认 engage 反问已生效（“我最近在练卧推”“今天也好热”都会被反问），但**每一次**分享都被反问，太像审问。本机制把反问频率降下来，且**绝不连续两轮反问**。

## 设计：信用桶 + 冷却 + 轻随机

状态 `QuestionPacing`（纯内存，重启重置——冷启动小七略带好奇，符合预期）：

- `credit: u8`：每次“分享”+1，上限 `CREDIT_CAP`；每次真的发问扣 `ASK_COST`。
- `last_turn_was_question: bool`：上一轮是否触发了反问。

决策（在 `converse.rs` 第 7.5 步执行，planner 保持纯函数，架构原则 #8）：

1. planner 给出 `intent.goal` 后，掷一次 `roll = rand::thread_rng().gen::<f64>()`。
2. 若 `goal == "engage"`：
   - `allow = credit >= ASK_THRESHOLD && !last_turn_was_question && roll < FOLLOWUP_PROB`。
   - 允许 → 保持 `engage`，扣 `ASK_COST` 信用，`last_turn_was_question = true`。
   - 不允许 → 降级为 **`react`**（新增 intent），信用 +1（封顶），`last_turn_was_question = false`。
3. 若 `goal != "engage"`：不动 credit，仅 `last_turn_was_question = false`。
4. `react` 在 `grounding.rs` 注入 cue：“温暖具体地回应，但本轮不许提问，只是陪伴。”

`proactive_bubble`（主动气泡）是独立路径，**本机制不覆盖**它（它本身低频）。

## 常量（集中在 `src-tauri/src/mind/pacing.rs`）

| 常量 | 值 | 含义 |
|---|---|---|
| `FOLLOWUP_PROB` | `0.6` | 在可用窗口内的发问概率（调密度只改这一个） |
| `CREDIT_CAP` | `3` | 信用桶上限 |
| `ASK_COST` | `1` | 每次发问消耗的信用（=1 才能命中 ~40% 目标，见下） |
| `ASK_THRESHOLD` | `2` | 发问所需的最低信用 |

## 频率数学（稳态）

把状态机当成“每轮都是分享”的马尔可夫链。`ASK_COST = 1` 时，唯一的硬封顶来自“绝不连续两轮发问”（理论上限 ~50%）。由 pacing.rs 的 `steady_state_rate_measured` 单测用 30 万轮确定性 LCG 实测：`p = 0.6` 时稳态 ≈ **37.5%**，命中用户要求的 ~40% 目标。

- 取 `ASK_COST = 1`（而非协调派单最初给的 2）正是为了够到 40%：`ASK_COST = 2` 会在每个发问周期里多塞一个 credit 重建的“死轮”，把封顶压到 ~33%、p=0.6 时只有 ~30.6%，低于目标。
- 要更密就调高 `FOLLOWUP_PROB`（向 ~50% 上限靠）；要更稀就调低它。

## 触点

- `src-tauri/src/mind/pacing.rs`（新增）：`QuestionPacing` + `throttle()` + 常量 + 单测。
- `src-tauri/src/mind/mod.rs`：挂 `pub mod pacing;`。
- `src-tauri/src/commands.rs`：`AppState` 加 `question_pacing` 字段；`send_message` 传给 `converse`。
- `src-tauri/src/lib.rs`：`.manage(AppState{...})` 初始化 `question_pacing: Default::default()`。
- `src-tauri/src/mind/converse.rs`：签名加 `pacing` 参数；第 7.5 步节流覆盖（silence 分支早退后才执行）。
- `src-tauri/src/mind/grounding.rs`：`format_intent` 新增 `react` 分支 cue。
- `src-tauri/resources/prompts/system.txt`：规则 2 改为“由 [Intent] 驱动”（engage 才问、react 不问）。
- `Cargo.toml`：加 `rand = "0.8"`。
- `src-tauri/tests/conversation_harness.rs` / `questioning_harness.rs`：适配。

## 如何调参

手感太密 → 调低 `FOLLOWUP_PROB`（如 0.45，约 30%）；太稀 → 调高（如 0.8，接近 ~48% 上限）。
想放宽“连续两轮”硬规则 → 改 `throttle` 的 `last_turn_was_question` 判断（当前架构不推荐）。
想让冷启动更克制 → 把 `QuestionPacing::default` 的初始 credit 设低（当前 0，已经偏低）。

## 已知边界

状态不持久化：重启后 reset。这是有意为之（冷启动小七略带好奇）。
只节流**反应式聊天**的 engage；主动气泡（`proactive_bubble`）不在此列。
诊断日志：`[pacing] roll=... credit=...->... last=...->... goal=...`。
