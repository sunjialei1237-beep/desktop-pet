# ADR: Scheduler 观测层 + 能力开关（推进 §A2，架构对齐版）

- **状态**：Accepted —— **取代** [`2026-08-07-scheduler-deferred.md`](2026-08-07-scheduler-deferred.md)
- **日期**：2026-08-08
- **关联计划**：§A2 Unified Scheduler；北极星 #6（每个能力可关）、#11（可解释）

## 背景

原计划 §A2 想要 `Scheduler { ticks_1s/30s/daily }` + `trait Tick` 的统一调度器。`2026-08-07` 的 ADR 正确地**否决了 trait-Tick 多态**：Body 跑在前端（#5），Rust 没有 1s 动画 tick；只剩 30s / 1h 两个节拍，`Box<dyn Tick>` 是投机抽象。但那个 ADR 把"可观测 / 可扩展"作为开放方向留了下来（"方案 B + 后果：可读、可扩展"），并未否定 §A2 的全部价值。

推迟期间也暴露了真实痛点：
- **#11 可解释性缺口**：Reflection / Consolidation / Review 这些昂贵的后台 LLM 任务，跑没跑、上次何时跑、成功还是失败——Debug Panel 看不到，排查只能翻日志。
- **#6 能力开关缺口**：用户想"关掉 Reflection 省钱，但记忆照常"做不到——它们和 core aliveness 绑死在 `loop_runner`，无独立开关。

## 决策

不引入被否决的多态调度器，**只交付 §A2 中那个 ADR 自己点名为"可扩展"方向的两块价值**：

1. **观测层**（`lifecycle/scheduler.rs`）：一个进程级注册表，每个后台任务一行 `JobStat { name, cadence, enabled, disableable, last_run_at, last_status, last_message }`。`loop_runner` 在每个任务执行点调 `record(name, enabled, status, msg)` 上报结果，Debug Panel 经 `get_scheduler_stats` 命令读出。
2. **能力开关**（`config [scheduler]`）：4 个昂贵的 Soul/清理能力各带 enable flag —— `enable_reflection` / `enable_consolidation` / `enable_relationship_review` / `enable_lifecycle_cleanup`。`loop_runner` 用 `scheduler::should_run(flag)` 门控，关闭时记 `skipped`（不执行、不盖时间戳）。Core aliveness（homeostasis / pending_check / emotion_push / presence_watch / lonely_nudge）**不**设开关——关掉它们等于杀死她，不是优雅退化。

执行点**保持原位不动**（`loop_runner` 直接调用），本模块只"记录结果 + 回答该不该跑"。无多态、无重排时序——正是原 ADR 警告的两件事。

## 取代关系

`2026-08-07-scheduler-deferred.md` 的核心否决（**不做 trait-Tick 多态**）仍然成立，本 ADR 不推翻它。本 ADR 只兑现那个 ADR 自己预留的"可观测 / 可扩展"开口，因此状态标记为 superseded（其结论已被这里的"做观测+开关"补全）。

## 后果

- ✅ #11：后台任务心跳可见——Debug Panel 一眼看到哪个任务上次何时跑、成功/跳过/失败及原因。
- ✅ #6：四个昂贵能力可独立关闭，关闭即优雅退化（"关掉 reflection，记忆照常"），配置见 `config.example.toml [scheduler]`。
- ✅ 无破坏性改动：执行逻辑零迁移，纯增量上报；267 库单测全绿，含 6 条 scheduler 纯函数测。
- ⚠️ 注册表是进程级 `Mutex<Vec<JobStat>>` 内存态，重启归零（`last_run_at` 不持久）。可接受——它是"最近一次心跳"观测，非账本。
- 🔭 未来若要真正统一时序（如新增 daily 节拍），在此注册表基础上加 `next_due_at` 字段即可，仍是本"记录+决策"模式，不需要回到 trait-Tick。
