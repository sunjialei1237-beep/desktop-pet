# 决策记录:不引入统一 Scheduler (ADR)

> 日期: 2026-08-07
> 状态: **Superseded（2026-08-08）** —— 核心否决（不做 trait-Tick 多态）仍成立；本 ADR 预留的"可观测/可扩展"开口已被 [`2026-08-08-scheduler-observability.md`](2026-08-08-scheduler-observability.md) 兑现（观测层 + 能力开关）。
> 决策者: Claude（自主推进批次）
> 触发: 实施计划 v1.1 §A2「统一 Scheduler」——原计划把所有定时任务收进 `Scheduler { ticks_1s / ticks_30s / ticks_daily / ticks_conversation }` + `trait Tick`。重新评估时发现该设计的前提与现状不符。

## 背景

审计当前 Rust 侧所有定时器（`grep thread::spawn|sleep|interval`）：

| 位置 | 周期 | 职责 | 性质 |
|---|---|---|---|
| `loop_runner.rs:30` | 30s | Mind：内稳态 / Pending / 情绪推送 / presence / lonely-nudge | 周期 tick |
| `loop_runner.rs:41` | 1h | Soul：decay / closeness drift / cleanup / reflection / consolidation / review | 周期 tick |
| `perception/cursor.rs:33` | ~ms | 感知：光标轮询（供注视/注意力输入） | 高频输入采样 |
| `lib.rs:119` | 一次性 | 启动：embedding 回填 | one-shot |
| `lib.rs:174` | 一次延迟 2s | 启动：延迟拉起 life loop | one-shot |

`start_life_loop` 已经是「所有周期定时器的唯一注册点」——两个线程块、一目了然。

计划 §A2 的 `ticks_1s`（Body：动画/物理/注意力）假设 **Body 跑在 Rust 里**。但实际实现遵循架构原则 #5（Mind-Body 解耦）：**Body（动画/物理/微行为）跑在前端**（`Live2DCanvas.tsx` / `fsm.ts` / `microBehavior.ts`），Rust 里根本没有 1s 动画 tick。`cursor.rs` 是**感知输入**，不是 Body 动画。

## 考虑过的方案

### 方案 A：按原计划引入 `Scheduler` + `trait Tick`
把 medium/slow 收进 `Vec<Box<dyn Tick>>`。
- 收益：单一调度器结构、理论上可注入测试。
- 代价：① 对仅 2 个周期、每周期单一实现引入 trait object 多态——零多态收益；② 强行把 ms 级光标轮询塞进 `Box<dyn Tick>` 反而劣化（高频、不需要 BrainState）；③ 重写承载近期所有主动行为（presence/lonely-nudge/reflection/review）的时序核心，高风险；④ 前提（Body-in-Rust）不成立，等于给一个不存在的结构补抽象。

### 方案 B：保持现状，`start_life_loop` 即调度器（本决策采用）
`start_life_loop` 已是定时器注册中心。tick 逻辑（homeostasis/decay/reflection-due 等）已在 `db`/`soul`/`emotion` 模块单测覆盖；loop 函数本身是 emit + 调 DB 的胶水，胶水不值得重型单测。

## 决策

**搁置 §A2 Scheduler 重写。** 理由链：
1. **前提不符**（原则 #5）：Body 在前端，Rust 无 1s 动画 tick，`ticks_1s` 字段无对象可装。
2. **投机抽象**（Karpathy 简单性原则 / 北极星 #9 just-enough / #10 优先生命感）：无多态、无注入需求时引入 trait object 是为对齐计划文本而非服务产品。
3. **风险/收益倒挂**：重写时序核心、零用户可感价值。
4. **可观测性不受损**：所有定时器仍集中在 `start_life_loop`，可读、可扩展（加新周期 = 加一个线程块）。

## 后果 / 何时复议

- 若未来出现「需要在多个 tick 实现间多态」或「需要把 loop 时序纳入单测」的真实需求，再引入 `trait Tick`，届时 medium/slow 可平滑迁移。
- 若 Body 动画迁入 Rust（如 Spine+PixiJS 重做后端化渲染，目前设计仍是前端），届时 §A2 的 `ticks_1s` 才有意义，连同本 ADR 一并复议。
- 本决策不阻塞任何功能；lonely-nudge / presence / reflection / review 等所有周期行为继续基于现有 `start_life_loop` 工作。
