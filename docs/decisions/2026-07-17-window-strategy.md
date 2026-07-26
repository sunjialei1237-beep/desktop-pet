# 决策记录:窗口策略与坐标系 (ADR)

> 日期: 2026-07-17
> 状态: Accepted
> 决策者: 用户 (SunJialei) + Codex (+ GPT 协议评审)
> 触发: 用户实测反馈 — 角色只剩一个头、气泡与输入框错位、无法拖拽、透明区域阻挡桌面点击

---

## 背景

审计 FIX-E (1dc46a3) 删掉了 `getCurrentWindow().startDragging()`, 理由是 "移动的是整个 OS 窗口而非窗口内的角色", 随后用 400x600 固定窗口 + `transform: translate(petPos)` 在窗口内移动角色。实测暴露四个同源问题:

1. 只剩一个头: `spatial.init()` 把角色放在 `y = windowHeight - 48 - 120`, 但 wrapper 本身 600px 高, wrapper 顶部贴近窗口底部后大部分溢出窗口外不可见。
2. 位置错位: 气泡用 CSS `bottom/left` 相对窗口定位, 角色被 `transform` 推到角落, 两套坐标系各走各的。
3. 不能拖拽: drag 的 mousemove/mouseup 监听器因早退守卫 + 依赖数组 `[isBeingDragged]` 永远不注册。
4. 透明区域吃桌面点击: 全项目无 `setIgnoreCursorEvents`, 400x600 透明 wrapper 摊在桌面上拦截背后所有点击。

四个问题同源于一个架构矛盾: 当前是 "400x600 固定小窗口 + 想在窗口内移动角色", 而 spatial/physics 按旧的全屏坐标系设计。

## 考虑过的方案

### 方案 A: 窗口里有个桌宠
窗口内元素。最简单, 生命感最差。当前架构的延续, 已证明有上述四个问题。

### 方案 B: 窗口就是桌宠 (本决策采用)
窗口尺寸 = 桌宠 + 预留交互区。移动桌宠 = 移动窗口。成熟方案 (BongoCat / Shimeji 系)。

### 方案 C: 桌面就是世界 (Shared World)
窗口只是渲染器, 桌宠拥有独立世界坐标, 支持多显示器 / 多 Renderer / 窗口吸附。设计文档 (6.8/6.9) 和 Shared World 愿景指向它, 但明确列为三期。

## 决策

**MVP 采用方案 B, 并对 GPT 提出的两个架构补充做明确取舍。**

1. 窗口固定为交互区 (约 400x760), 顶部预留气泡区, 底部留阴影。透明无边框 alwaysOnTop。**不动态 resize**。
2. 角色在窗口内静止居中, 不再用 `transform: translate(petPos)` 移动角色。气泡/输入框相对窗口定位, 消灭两套坐标系。
3. 移动桌宠 = 移动窗口 (`getCurrentWindow().setPosition`)。拖拽恢复 `startDragging()`。
4. `petPos` 语义 = 窗口左上角屏幕坐标, 初始屏幕右下角任务栏上方。
5. click-through = 后端 Windows API 全局鼠标钩子 + 收窄到模型包围盒的矩形 + 动态 `setIgnoreCursorEvents`。输入框可见时临时强制捕获。
6. 物理落体简化: 松手停原地; 30 秒自动回窝 (窗口缓慢移动回初始角)。physics.ts 的窗口内落体逻辑作废。

## 对 GPT 补充的取舍

### 不采纳: WorldPosition(Anchor + Offset)
违背 Architecture-Principles 第 9 条 ("不要一开始就做终局设计, 先留扩展点, 等复杂度长出来")。Shared World / 多屏 / 窗口吸附在设计文档里是三期。当真做 Shared World 时, 把 `petPos(x,y)` 迁移成 `WorldPosition(anchor,offset)` 是边界清晰的显式重构, 现在引入是投机性抽象 (Anchor enum 里五个成员四个空壳)。MVP 保持 `petPos` = 屏幕坐标 = 窗口位置。

### 不采纳: 基于 Live2D mesh Hit Test 的 click-through
Tauri 下技术不可行。click-through 靠 `setIgnoreCursorEvents(true)` 透传, 一旦 true 整个窗口收不到任何鼠标事件 (含 mousemove), 而 mesh hit test 必须在 webview 线程依赖鼠标事件 — 鸡生蛋。可行解是后端全局鼠标钩子只给屏幕坐标, 做不了 mesh 判断, 矩形绕不开 (但收窄到包围盒, 用户基本无感)。

### 采纳: 窗口 = 交互区而非只包角色
顶部预留气泡区、底部留阴影。但不动态 resize — GPT 担心的气泡撑爆窗口用固定预留区 + 气泡限宽限高解决。

## 给未来的迁移路径

当进入 Shared World (三期):
- `petPos(x,y)` 显式重构为 `WorldPosition { anchor: Anchor, offset: Vec2 }`
- 引入 preferred monitor / window anchor
- Renderer 与 Pet Entity 分离 (Window 可 hide/show, Pet 永远活着)
- click-through 矩形升级为 mesh hit test (届时若有多渲染器架构可解决鸡生蛋问题)

本决策刻意不预埋这些, 遵循 Architecture-Principles 第 9 条。

## Fix Log

> 2026-07-17 / 18

- 补充 4 个 `core:window` 权限到 `capabilities/default.json` (`allow-set-position` / `allow-outer-position` / `allow-scale-factor` / `allow-start-dragging`), 修复拖拽与初始定位无反应 (根因: 权限被拒, `.catch(() => {})` 静默吞错)。capabilities 改动需 cargo 重编, 必须重启 dev server。
- 气泡位置改用 CSS 变量 `--bubble-top` (230px) / `--bubble-top-pet` (215px) 下移到角色头顶上方; 输入框与思考圆点复用同一变量。微调只需改一个变量。
- `handleBodyClick` / `handleHeadClick` 加 280ms 延迟 + `pendingPokeRef` 做单/双击消歧: 双击在 `dblclick` 里立即清掉 pending 定时器、直接开输入框, 不再闪身体/头部气泡; 单击 (280ms 内无第二次点击) 正常触发戳反应。
