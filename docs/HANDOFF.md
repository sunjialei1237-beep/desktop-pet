# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-07-26**

## 项目一句话
见 [`CLAUDE.md`](../CLAUDE.md)。Kill List 三闭环驱动开发：活着 Body → 记住你 Memory → 懂你 Soul。

## 当前进度（以测试为准）

| 闭环 / 层 | 状态 | 锚定测试 |
|---|---|---|
| 闭环1 说→记住→跨会话召回 | ✅ | `cargo test --test memory_recall` |
| 闭环2 到期主动提起 | ✅ | harness + 实跑：3分钟后主动冒泡提醒 |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 闭环3 "她记得我"体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| 库单测 | ✅ 185 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ✅ | 实跑验收通过（autoFocus:false + ny 取反） |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）
**技术债清理完成（2026-07-26）：`proactive_harness` 不再复刻 `generate` 旧逻辑，改为直接调 `proactive::generate`。** 顺带把 `generate` 返回值从 `Option<String>` 升级为 `Option<BubbleOutcome{reply, anchor}>`——暴露 anchor 让 harness 的 S1 锚定检查能复用，且满足原则 #11（Debug Panel 可显示"她锚定在哪条记忆"）。3 个调用方同步改（commands.rs / closed_loop2_harness / proactive_harness）。lib 185 passed + 全 tests 编译通过。未提交，工作区脏（4 文件改 + HANDOFF）。等用户决定是否提交 + 选下一步。

## §最近一轮 (2026-07-26)：清技术债 — proactive_harness 简化
**起因**：用户选 HANDOFF §下一步候选 #1（清 Codex 技术债）。`tests/proactive_harness.rs`（Codex 写）复刻了 `generate` 的 emotion/retrieval/anchor-pick/budget/LLM 管道，与 `pending/proactive.rs::generate` 逻辑重复。

**关键障碍**：harness 的 `check_standards` S1（锚定）需要 `keyword`，而 `generate` 只返回 reply、丢弃 anchor。分析 S1 逻辑后发现：用完整 `memory_anchor` 当 keyword 语义不变——fact 走 synonym_hit（anchor 是 "key: value" 仍含英文 value），episode 走字符重叠（中文摘要天然重叠）。"前 4 字"技巧对 overlap≥2 判断无实质影响。

**修复（4 文件，原则 #1/#11）**：
- `proactive.rs`：加 `BubbleOutcome{reply, anchor}` 结构体；`generate` 返回 `Result<Option<BubbleOutcome>, String>`；构造处填 `anchor: memory_anchor`
- `commands.rs`：`proactive_bubble` 命令把 `BubbleOutcome` 映射回 `.reply`，**IPC 契约 `Option<String>` 不变，前端无感**
- `closed_loop2_harness.rs`：`outcome.reply` + 多打一行 anchor（可解释性）
- `proactive_harness.rs`：删复刻管道（emotion/retrieval/Intent/budget/LLM 全去），改调 `proactive::generate`，用 `outcome.anchor` 跑 `check_standards`；imports 瘦身（去掉 budget/Intent/retrieval/ChatMessage）

**验证**：`cargo test --lib` 185 passed；`cargo test --no-run` 全 8 个 test crate 编译通过（含两个 harness）。未跑真实 LLM harness（慢、需关 dev server）——编译通过即证明调用链正确。

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

## §未解决问题
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。现在 `BubbleOutcome.anchor` 已暴露，Debug Panel 可顺手显示"当前冒泡锚定的记忆"。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选（按优先级，等用户定）
1. **docs 治理**（agent 做）— 去 `superpowers/` 嵌套 + 归档过期 `bug-audit`/`fix-plan`/`feature-checklist` 到 `archive/`，更新 CLAUDE.md/HANDOFF.md 导航路径。
2. **P16 Debug Panel 补全** — Prompt token 预算 / Retrieved score breakdown / Reflect 分区（核心状态面板已在；`BubbleOutcome.anchor` 可补"冒泡锚定"展示）。
