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
| 闭环2 到期主动提起 | ✅ | `cargo test --test closed_loop2_harness` |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 闭环3 "她记得我"体感 | ⏳ | 主观，实跑 `npm run tauri dev` 验收 |
| 库单测 | ✅ 184 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ❌ P1 | 见 `known-issues-2026-07-18.md` |

**阶段**：Soul 层（P13）刚激活并通过端到端验证。按 Kill List，闭环 1+2 已稳，可推进闭环 3（体感）或收尾项。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。

## §当前任务（接手者先看这）
**无进行中任务。** 上一轮（2026-07-26）完成交接验证 + 阻塞 bug 修复，全部落地、测试通过。改动**未提交**（等你定 commit 粒度）。下一步候选见底部。

## §最近一轮 (2026-07-26)：交接验证 + DeepSeek v4 兼容
**起因**：审查 Codex 交接汇报，发现"闭环1已验证"**不实**——模型名 `deepseek-chat` 已失效 + v4 reasoning 爆预算，对话管道近期根本没跑通过真实 LLM。

做了：
- 新增 `soul_harness` / `closed_loop2_harness`（可重复验证 Soul + 闭环2，内存 DB + 真实 LLM）
- 修模型名 `deepseek-chat`→`v4-pro/flash`：`config.rs` 默认值+单测 / 项目根 `config.toml` / `config.example.toml` / AppData 运行时
- 修 v4 reasoning 爆预算：7 处 `max_tokens` 增大（gate/correction/consolidation→2048；extractor/reflection/converse/proactive→4096）
- 重构 `pending::proactive::generate`（从 `proactive_bubble` 命令层抽出，闭环2 可测，原则 #1 命令薄）
- 修 `memory_recall` harness（Codex 加 pacing 时漏更新，编译挂）
- `llm/client.rs` 加 `[llm-empty-content]` warn 监控（content 空→reasoning 爆预算的永久告警）
- 确认 `config.toml` gitignore，API key 未泄入 git

**改动文件**：`tests/soul_harness.rs`、`tests/closed_loop2_harness.rs`（新）；`tests/memory_recall.rs`、`mind/{gate,correction,extractor,converse}.rs`、`soul/{reflection,consolidation}.rs`、`pending/{proactive,mod}.rs`、`commands.rs`、`llm/client.rs`、`config.rs`、`config.toml`、`config.example.toml`（改）。

**审查纠错**：Codex 说"P16 Debug Panel 未实现"实际已部分在（`DebugPanel.tsx` 六分区可用）；"闭环1已验证"此前是假的。

## §未解决问题
- **P1 视线 360°（上下卡死）**：库 `pixi-live2d-display-lipsyncpatch` 的 `focus()` 用 atan2 耦合 x/y。已绕过仍不全通。诊断日志 `[gaze]`/`[ct]` 留在 `Live2DCanvas.tsx`/`App.tsx` 待 revisit。详见 `known-issues-2026-07-18.md`。
- **Codex 技术债**：`tests/proactive_harness.rs`（Codex 写的）复刻了 `generate` 的旧逻辑，现可简化为调 `proactive::generate`。
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选
1. **提交这一轮**（建议 `fix: DeepSeek v4 兼容 + 三闭环验证 harness`，或拆分）
2. 清 Codex 技术债（`proactive_harness.rs` 简化为调 generate）
3. 闭环3 体感验收（实跑 app，主观感受"她记得我"）
4. P1 视线修复
5. P16 Debug Panel 补全
6. docs 治理（去 `superpowers/` 嵌套、归档过期 `bug-audit`/`fix-plan`/`feature-checklist` 到 `archive/`）
