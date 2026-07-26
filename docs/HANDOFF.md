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
| 闭环3 "她记得我"体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| 库单测 | ✅ 184 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ✅ | 实跑验收通过（autoFocus:false + ny 取反） |

**阶段**：Soul 层（P13）刚激活并通过端到端验证。按 Kill List，闭环 1+2 已稳，可推进闭环 3（体感）或收尾项。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。

## §当前任务（接手者先看这）
**MVP 三闭环全通（2026-07-26）。** P1 视线修复已提交 `86ca465`；闭环3 体感验收通过——用户实跑 `npm run tauri dev`，**重启**后问"我最近忙啥"，她从持久记忆 recall 出"找实习"（闭环1 真实运行 ✅ + 闭环3 体感 ✅）。闭环2 真实运行待 pending 自然到期触发（harness 已 ✅）。无进行中任务，工作区干净。等选下一步（见 §下一步候选）。

## §最近一轮 (2026-07-26)：P1 视线修复（autoFocus + y 翻转）
**起因**：用户选 P1 视线修复方向。`known-issues-2026-07-18.md` 自 07-18 起卡住，诊断了 A/B/C 三假设但未坐实。

做了（读库 `pixi-live2d-display-lipsyncpatch/dist/cubism4.es.js` 源码定位）：
- **坐实 A（主因）**：`_Automator` 默认 `autoFocus=true`（:10149）→ 绑 `globalpointermove` → `model.focus`（:10272）→ atan2 耦合（:10495）把 target 锁单位圆 → 鼠标在右时 `targetY≈0` → 永不上下看 = "卡死"。每次指针移动覆盖我们 `focusTickerFn` 写的独立 x/y。
- **坐实 C（次要）**：库 `model.focus` 的 y 用 `-sin(radian)`（:10496）翻转（PIXI y 向下、`ParamAngleY` 正=看上），我们裸 `ny` 没翻 → 解锁后上下会颠倒。
- **排除 B**：`focusController` 结构正确（`FocusController` :7992，`focus()` 设 target / `update()` 加速度平滑）。但 `autoFocus` setter 在 **Automator**（:10244）不在 Live2DModel，`model.autoFocus=false` 会静默失败 → 必须 `from(url,{autoFocus:false})`。

修复 `src/Live2DCanvas.tsx` 两处（最小改动）：
1. `Live2DModel.from(modelUrl)` → `from(modelUrl, { autoFocus: false })`（治卡死；`autoHitTest` 默认 true 保留 Head/Body 点击）
2. `ny` 归一化处取反 `const ny = -Math.max(-1, Math.min(1, ...))`（治 y 反；与库 `-sin` 对齐，`focus(nx,ny)` 不变）

**验证**：`tsc --noEmit` 零错误；`cargo test --lib` 184 passed（未动后端）。用户实跑 `npm run tauri dev` 确认 360° 通过；遂删 `[gaze]`/`[ct]` 诊断日志 + `mode` 孤儿变量。详见 `docs/known-issues-2026-07-18.md` 🔧 修复小节。

## §未解决问题
- **Codex 技术债**：`tests/proactive_harness.rs`（Codex 写的）复刻了 `generate` 的旧逻辑，现可简化为调 `proactive::generate`。
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选（按优先级，等用户定）
1. **清技术债**（agent 做，小快）— `tests/proactive_harness.rs` 简化为调 `proactive::generate`，消除逻辑重复。
2. **docs 治理**（agent 做）— 去 `superpowers/` 嵌套 + 归档过期 `bug-audit`/`fix-plan`/`feature-checklist` 到 `archive/`，更新 CLAUDE.md/HANDOFF.md 导航路径。
3. **P16 Debug Panel 补全** — Prompt token 预算 / Retrieved score breakdown / Reflect 分区（核心状态面板已在）。
4. **闭环2 真实运行验证**（可选，用户做）— 设一个近期 pending（如"明天提醒我 X"），等自然到期看她是否主动冒泡提起；harness 已 ✅，此项是真实运行补验。
