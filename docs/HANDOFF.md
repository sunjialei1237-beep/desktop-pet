# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-07-27**

## 项目一句话
见 [`CLAUDE.md`](../CLAUDE.md)。Kill List 三闭环驱动开发：活着 Body → 记住你 Memory → 懂你 Soul。

## 当前进度（以测试为准）

| 闭环 / 层 | 状态 | 锚定测试 |
|---|---|---|
| 闭环1 说→记住→跨会话召回 | ✅ | `cargo test --test memory_recall` |
| 闭环2 到期主动提起 | ✅ | harness + 实跑：3分钟后主动冒泡提醒 |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 闭环3 "她记得我"体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| 库单测 | ✅ 194 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ✅ | 实跑验收通过（autoFocus:false + ny 取反） |
| 生命感 回来主动招呼 | ✅ 实跑通过 | loop_runner presence 转换 → welcome_back_bubble |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）
**P13/P15 Soul 慢循环闭环（2026-07-27）。** 补 Soul 三块调度断链：①Reflection 接 `slow_tick` 自动调度（抽 `should_run_reflection`+`maybe_run_if_due` 纯函数，>20h 冷却；commands 瘦身 wrapper，IPC 签名不变）②Consolidation 接 `slow_tick`（同 `block_on` 块，内部 100-episode 阈值守护）③welcome-back 消费 surfaced thought（`generate_welcome_back` 调 `surface_thoughts`，注入招呼 prompt——隔夜回来她说出昨晚反思的念头）。验证：cargo test --lib **197 passed**（+3 新）/ --no-run 全 harness 编译 ✅ / tsc ✅ / **`soul_loop_harness` 2 passed（真实 LLM 端到端）**——核心两条新路径已自主验证通过（详见 §最近一轮）。仅 slow_tick periodic 接线待常开 >1h 实跑确认。下一步候选见文末。

## §最近一轮 (2026-07-27)：Soul 慢循环闭环（Reflection 自动调度 + thought 融入回来招呼 + Consolidation 调度）

**起因**：用户"严格按 plan 执行"。对照 `implementation-plan.md` 验证标准逐项核对，P13 #1（Reflection 自动触发）/ #3（thought 融入下次交互）+ P15.1 慢循环清单**未达成**——Soul 三块逻辑（Reflection / Monologue / Consolidation）实现完整但**调度链断**：Reflection 仅启动 IPC 触发（常开不重启 → 永不跑）、thought 仅 `App.tsx` 启动独立气泡（没融入招呼）、Consolidation 未被 `slow_tick` 调用（只有 `lifecycle_cleanup` 被调）。

**实现**（4 文件，原则 #1/#5/#6/#8/#11）：
- `soul/reflection.rs`：新增同步纯函数 `should_run_reflection(db)->bool`（20h 冷却判断，可单测）+ async `maybe_run_if_due(db,llm)->Result<bool>`（调 `run_reflection(Daily)`，Err 传播）。+3 单测（无记录 true / 1h 前 false / 25h 前 true）
- `commands.rs`：`trigger_reflection_if_due` 瘦身为调 `maybe_run_if_due`（**IPC 签名不变**，规避踩坑 #4，前端 `App.tsx:387` 无感；Err 吞为 Ok(false) 保前端契约）
- `lifecycle/loop_runner.rs`：`slow_tick` 末尾加 `tauri::async_runtime::block_on` 块（slow_tick 在 std::thread，`block_on` 进入 runtime 不 panic；cadence 1h 阻塞可接受）→ `maybe_run_if_due`（Gap1）+ `consolidate`（Gap4）。`use crate::commands::AppState`。LLM 未配 → 跳过（#6 优雅退化）
- `pending/proactive.rs`：`generate_welcome_back` 调 `surface_thoughts`，thought 拼进同一条 user prompt（**不增 LLM 调用** #8）；`surface_thoughts` 消费性（mark surfaced）保证只浮现一次。降级路径（`welcome_back_bubble` canned 分支）不 surface——thought 留给下次 LLM 招呼，不白白消费。log 加 `has_thought`（#11）

**架构契合**：#1 thought 由 Rust surface、LLM 只配音 / #5 slow_tick 后台不阻塞 Body（Body 走自己的 medium loop）/ #6 LLM 未配优雅退化 / #8 reflection 每天≤1 次、thought 并入已有那次 LLM 调用 / #10「隔夜回来她说出昨晚念头」是设计文档点名场景 / #11 has_thought + source_reflection 时间戳可追溯。

**验证**：cargo test --lib **197 passed**（194+3）/ cargo test --no-run 全 harness 编译 ✅ / npx tsc --noEmit ✅ / **`cargo test --test soul_loop_harness` 2 passed（真实 LLM 端到端，新增 harness）**：①`maybe_run_if_due` 无历史→跑通，reflections 0→1，产出 3 traits+2 thoughts；②`welcome_back_consumes_surfaced_thought` 预插 thought「他今天好像有点累，希望他早点休息」→ `generate_welcome_back` 消费（surfaced_at 标记 ✓）→ 回复「你回来啦。刚才休息了一下，现在还累吗？要不要喝杯奶茶缓缓？」——**昨晚念头被自然融入招呼**（has_thought=true）+ 记忆锚定（奶茶）。**仅 slow_tick periodic 调用接线本身待常开 >1h 实跑确认**（编译 + 代码审查已高度可信）。

**Scope 边界**（留 follow-up，避免过度）：Gap3 Consolidation 反向更新 Facts（plan #9 V2）/ TurnThreshold·MajorEvent 触发器（本轮只做 Daily，满足 plan「自动触发」）/ converse 注入 thought（本轮只 welcome-back，最自然的「回来」时机）。

## §历史：welcome-back 回来主动招呼 (2026-07-27 早些)
**起因**：用户选 §下一步候选 #2（新增生命感，北极星 #10）。诊断发现 `perception/presence.rs`（`idle_seconds`/`classify`/`current_presence`）**完全空转、零调用方**；现有"回来招呼"只在 ①首次启动 welcome（`lib.rs:119`）②系统睡眠唤醒（`loop_runner` app-status resumed）触发——**缺失核心场景**：用户离开>5min 回来动鼠标她不察觉。

**实现**（6 文件；决策：LLM 记忆锚定+降级 / 仅 LongAway 触发）：
- `perception/presence.rs`：新增 `Transition::ReturnedBack{away_secs}` + `classify_transition(prev,now,away_secs)` 纯函数（仅 LongAway→Active 触发，BriefAway 不触发）+ 5 单测
- `pending/proactive.rs`：新增 `generate_welcome_back(away_secs)`（**不改 `generate` 签名**，规避踩坑 #4，现有 3 调用方零改动）。区别于 generate：不查 pending_due（回来是连接非跟进）、anchor 可选（无 anchor 仍说话，不像 generate 沉默）、welcome 语境 prompt（"用户离开了 N 分钟刚回来"）。复用 retrieve/budget 管道，tone 随 mood
- `emotion/react.rs`：新增 `welcome_back_canned(mood,away_secs) -> &'static str` 降级纯函数 + 4 单测（随 mood 桶+时长桶选文案，纯规则 #8）
- `commands.rs` + `lib.rs`：`welcome_back_bubble(away_secs)` 命令（LLM 优先/None 降级）+ invoke_handler 注册。**踩非 Send 坑**：`if let` 条件中的 MutexGuard 跨 `.await` 使 future 非 Send——改独立 `let` 绑定（同 `proactive_bubble` 模式）
- `lifecycle/loop_runner.rs`：medium 线程闭包持有 `last_presence`+`away_since`（线程局部无锁）；`check_presence_transition` 检测转换 → emit "welcome-back"；`recent_interaction_secs` 30s 守卫防与正在进行的对话撞
- `src/App.tsx`：`listen("welcome-back")` → `invoke welcome_back_bubble` → `showBubble`（onboarding/awayMode 守卫，与 proactive-prompt 一致）

**验证**：cargo build ✅ / `cargo test --lib` 194 passed（185+9）/ cargo test --no-run 全 harness 编译 ✅ / `npx tsc --noEmit` ✅。**实跑通过**：离开 5.5min（away_secs=330）回来 → 单气泡"回来啦，刚刚想到星际穿越了，怪想你的"（记忆锚定 + 情感连接到位）。

**实跑修的 2 个 bug**：
1. **StrictMode 双气泡**：`main.tsx` 用 `<React.StrictMode>`，dev 下双挂载 effect；`listen()` 异步注册，第一次 mount 的 listener 在 cleanup 之后才 resolve → 泄漏成第二个 listener → 同一 welcome-back 事件双触发、冒两个气泡（现有 6 个 listener 都有此竞态，welcome-back 是首个显著一次性事件才暴露）。修：事件 effect 加 `cancelled` 标志，late-resolving listener 自我 unlisten。**坑**：HMR 清不掉已泄漏的 listener，必须重启/刷新 webview 才能验证修复。
2. **启动误触 away_secs=0**：dev 用脚本启动 app 时用户可能已 idle>300s，`last_presence` 初始=LongAway、`away_since`=None，第一次 tick 动鼠标 → LongAway→Active 误判"回来"、away_secs=0。修：`loop_runner` 初始 `last_presence=Active`（生产用户双击启动本就 Active），只对真实 Active→away→Active 循环反应。

**架构契合**：#1 Rust 选 anchor+拼 prompt、LLM 只配音 / #5 后端 life_loop 持续检测不依赖前端 / #8 最多 1 次 LLM + 降级零成本 / #3 anchor 只来自检索不编造 / #6 onboarding/awayMode 可关 / #10 生命感优先。

## §历史：docs 治理 — 去 superpowers 嵌套 + 归档过期 (2026-07-26)
**起因**：用户选 §下一步候选 #1（docs 治理）。`docs/superpowers/` 是无意义嵌套层；docs 根堆了多份过期文档，混淆"当前有效"与"历史快照"。

**执行**（6 git mv + 8 引用）：
- design/plan：`docs/superpowers/{specs,plans}/` → `docs/{specs,plans}/`（与 `decisions/` 子目录风格一致）
- 归档：`bug-audit` / `fix-plan` / `feature-checklist` / `known-issues` → `docs/archive/`
- 引用同步 8 处：CLAUDE.md ×5（项目一句话 + 文档导航 design/plan + known-issues 描述）、Architecture-Principles.md ×2、implementation-plan.md ×1（自引 design）
- `known-issues` 主体 P1 视线已修复（86ca465），归档并改 CLAUDE.md 描述为"历史问题诊断"；`test-checklist-v2.md` 保留原位（仍有效）

**验证**：grep `superpowers` 路径引用 0 命中；git status 6 rename + 引用修改。

## §历史：清技术债 — proactive_harness 简化（2026-07-26）
`tests/proactive_harness.rs`（Codex 写）复刻了 `generate` 的 emotion/retrieval/anchor/budget/LLM 管道。简化为直接调 `proactive::generate`；顺带 `generate` 返回值升级为 `Option<BubbleOutcome{reply, anchor}>`（暴露 anchor 让 harness S1 复用 + 满足原则 #11）。3 调用方同步（commands.rs / closed_loop2_harness / proactive_harness）。lib 185 + 全 tests 编译通过。已提交 `fc8bcfe`。

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
1. **P16 Debug Panel 补全** — Prompt token 预算 / Retrieved score breakdown / Reflect 分区（核心状态面板已在；`BubbleOutcome.anchor` 可补"冒泡锚定"展示；本轮 `has_thought` + unsurfaced_count 可顺手接入 Reflect 分区）。
2. **新增生命感**（原则 #10 北极星）— 情绪外显（Emotion→Live2D 视觉映射 P10 emotionBridge）/ 更自然的呼吸眨眼 / 更多 idle 微行为，而非工具性功能。（回来主动招呼 + 隔夜念头已由近两轮完成）
3. **Soul follow-up** — TurnThreshold/MajorEvent 触发器（每 30 轮自动反思）/ Consolidation 反向更新 Facts（原则 #9 V2）/ converse 注入 surfaced thought（正常对话也带出昨晚念头）。
