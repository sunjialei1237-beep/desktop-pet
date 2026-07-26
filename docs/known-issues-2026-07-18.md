# 已知问题记录 (2026-07-18)

> 这份文档记录已确认但暂未解决的问题，保留诊断上下文，避免下次重新摸索。

---

## 🔴 P1：视线无法 360° 跟踪（上下方向卡死）

**状态：** 暂时跳过，待 revisit。当前表现为"视线只在画布二分之一位置（中心点）附近有效，上下方向几乎不动"。

### 已查明的根因（部分）
1. **库 `pixi-live2d-display-lipsyncpatch` 的 `model.focus(x,y)` 实现有缺陷**（`dist/cubism4.es.js:9826`）：用 `atan2` 把 (tx,ty) 锁到单位圆，x/y 视线参数耦合。要抬头(y=-1)必须鼠标正上方极远且无水平偏移。**已绕过此调用**。
2. **Haru 模型本身上下范围没问题**：库在 `cubism4.es.js:10242` 用 `ParamAngleY = focusController.y * 30`，即 ±30°，范围足够。**不是模型资源限制**。
3. **已绕过 `model.focus()`，改直接调 `m.internalModel.focusController.focus(nx, ny)`**（src/Live2DCanvas.tsx focusTickerFn），nx/ny 独立归一化到 canvas 尺寸（400×600），不经过 atan2。**归一化基准已修正为 app.screen.width/height**。

### 仍未解决的可能原因（下次从这里查起）
- **A. 库的 `_autoFocus` 仍每帧覆盖我们的值**：库内部可能挂全局 mousemove 监听，调用 `model.focus()` 用真实鼠标覆盖我们写入 focusController 的目标值。focusTickerFn 注释自己写过"library auto-tracks the global mouse"——这个 auto-track 可能没真正关掉。**查 autoFocus 开关、或每帧把库写入的 focusController.targetX/Y 再覆写一次**。
- **B. `as unknown` 类型断言拿到的 `im.focusController` 可能结构不对**（不同版本字段名/层级不同），调用静默失败。**在 devtools 打 model 实例 inspect internalModel.focusController 是否真存在、focus() 是否真生效**。
- **C. y 方向可能需要取反**：`focusController.focus(nx, ny)` 若上下反，改成 `focus(nx, -ny)`。从 `[gaze]` 日志的 nx/ny 和实际头部朝向对照判断。

### 留在代码里的诊断（勿删，revisit 时要用）
- `src/Live2DCanvas.tsx` focusTickerFn：`[gaze]` 日志（每 500ms，含 mode/focusX/focusY/nx/ny）。
- `src/App.tsx` global-cursor 回调：`[ct]` 日志（宽松矩形边界 + inside）。
- 右键菜单"开发者工具"入口（`invoke("open_devtools")` → Rust `open_devtools` 命令）用于读这些日志。

### 相关已生效改动（这些是对的，别回退）
- `HEAD_FOCUS = {x:200, y:0}`（Ignored 闲置视线钉点在画布顶部）。
- click-through：宽松矩形 PAD=40% + 顶部额外上扩 15%；紧矩形 INSET=10% 用于点击命中（两矩形分离）。
- 穿透期间 pointerRef 由 global-cursor 事件持续更新（client 坐标口径），视线数据源不会断。

### 关键文件
- src/Live2DCanvas.tsx focusTickerFn（约 190 行起）
- src/App.tsx global-cursor 回调（约 327 行）
- `node_modules/pixi-live2d-display-lipsyncpatch/dist/cubism4.es.js:9826`（库 focus 缺陷）、`:10242`（ParamAngleY 映射）

## ✅ 已实现：反问频率控制（2026-07-20）

用户反馈“每次分享都被反问，像审问”。已加信用桶 + 冷却 + 轻随机机制，反问频率控制在 **~30%（FOLLOWUP_PROB=0.6）**，且**绝不连续两轮反问**。planner 保持纯函数，节流在 converse 层执行。详见 `docs/followup-frequency-2026-07-20.md`。要调密度只改 `src-tauri/src/mind/pacing.rs` 的 `FOLLOWUP_PROB` 一个常量。
