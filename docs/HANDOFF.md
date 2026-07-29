# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-07-29**

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
| 生命感 情绪连续外显 | ✅ build 过 / 待实跑 | emotionDriver → Live2DCanvas 连续参数插值（P10 emotionBridge）|
| 生命感 气泡生命力(节奏+glyph) | ✅ 实跑通过 | bubblePacing 打字节奏随情绪(关键词驱动) + bubble-glyph 无文字气泡（#12）|
| 对话 流式回复 | ✅ 实跑确认 | ipc::Channel 逐字（emit/listen 命令体内投递延迟+listener 立即 unlisten 全丢→Channel 正解）；用户长回复实跑确认逐字 |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）
**气泡生命力已完成并实跑通过（2026-07-29，Tier 1 #2 ✅）。** 打字节奏随情绪（`bubblePacing` + `inferPacingMood` 关键词驱动，见 §最近一轮 pacing 修复）+ 无文字气泡（glyph 省略号/叹气，#12）。提交 `abb9d49`（特性）+ `ebc1082`（pacing 修复）。release exe 已重建至 `ebc1082`（09:43）。**下一步**：Tier 1 #3 Foley 音效 / Tier 2 Soul 深度（converse 注入 thought），或用户指定。

## §最近一轮 (2026-07-29)：气泡生命力 — 打字节奏随情绪 + 无文字气泡（Tier1 #2）

**起因**：用户选 §下一步 Tier1 #2（北极星 #10 生命感，对话是核心交互）。审计发现气泡形态动画已有 5 种 keyframes（styles.css:445-501），但缺两块：①打字节奏固定 30ms 无论情绪 ②无文字气泡完全缺（#12 沉默表达未落地）。

**实现**（3 文件，纯前端零后端，原则 #1/#9/#10/#11/#12）：
- 新增 `src/animation/bubblePacing.ts`：`typewriterPacing(moodLabel)->{intervalMs,catchDiv,hesitate}` 纯函数，6 档映射（开心 22ms 快流畅 / 调皮 26ms / 平静 32ms baseline / 担心 42ms+20%停顿 / 难过 50ms+10% / 疲惫 55ms+15%喘），未知标签 fallback 平静
- `App.tsx` 流式打字机参数化：首 chunk 时按 `moodLabel` 取 pacing，interval/step/hesitate 全由 pacing 决定；hesitate 仅 `!streamEnded` 时生效（收尾必完成）。与气泡形态 class 用同一 moodLabel → 语气与表情一致
- `App.tsx` 无文字气泡两触发：①空回复 fallback「（……）」→ glyph「…」②`emotionTimer` idle 叹气（疲惫/难过 + 空闲 + 8% →「呼…」）。**stale-closure 守卫**：新增 `bubbleVisibleRef`/`isThinkingRef` + mirror effect（emotionTimer 是 setInterval，闭包捕获 stale state；ref-mirror 是现有 propsRef 模式），用刚获取的 `emo.mood_label`（实时）。已有 `onboardingActiveRef`/`awayMode` 守卫
- `styles.css`：`.bubble-glyph`（大字号 / 无尾巴 `::after{display:none}` / 淡 opacity:0.8 / `bubble-glyph-soft` 轻动画）

**架构契合**：#1 节奏纯规则不走 LLM / #9 MVP 6 档节奏+glyph 刚够用 / #10 说话语气+叹气=生命感 / #11 pacing 纯函数可测+JSDoc / #12 沉默省略号/叹气是表达。

**范围边界**（follow-up）：「害羞慢现」形态未做——后端 `label_for_mood_full`（emotion/state.rs:39）只产 6 标签无「害羞」，强加需改后端标签逻辑（破坏性）。叹气只接前端 emotionTimer（未接后端 life_loop 事件源）。

**验证（实跑通过 ✅）**：`tsc --noEmit` ✅ / `npm run build` ✅（480 modules）/ release exe 重建 / 用户实跑确认节奏差 + glyph + 叹气。

**后续修复（同日，`ebc1082`）：pacing 改用本轮输入关键词驱动。** 实跑发现「难过」与「讲故事」同速——根因 moodLabel 是**慢变量**双重滞后：①前端 `onChunk` 闭包 moodLabel 是 invoke 时捕获的旧值（stale closure）②后端 emotion 在 converse Step 12（LLM 之后）才更新，首 chunk 时后端 emotion 仍是旧的。改 `inferPacingMood(text, fallback)` 前端关键词启发式（难过/去世/哭→难过档），首 chunk 即时生效；moodLabel 降为无情绪词时的 fallback。零后端、即时。follow-up：后端流式前传统一情绪节奏信号，消除与 react.rs 的重复。

**release 重建踩坑（写入避免重复）**：`npx tauri build --no-bundle` 覆盖 exe 时，若桌宠**正在运行**，exe 文件被 Windows 锁 → cargo `failed to remove file ... 拒绝访问 (os error 5)`。**构建前必须 `taskkill //IM desktop-pet.exe //F`**，杀后 sleep ~3s 等 OS 释放句柄再 build，且**构建完成前不要重开快捷方式**（一次因构建中重开 → 新进程锁 exe → 失败）。

## §历史 (2026-07-28)：流式回复 — emit/listen → ipc::Channel（根因定位 + 修复）

**根因（对比定位，非瞎改）**：`download_embedding_model` 命令体内 `app.emit("download-progress")` **工作正常**（SettingsPanel listen 收到进度），因其 listener 在 `useEffect` 里**组件挂载时注册、长期存活**（命令返回后不立即 unlisten）→ 即使命令体内 emit 投递有延迟，listener 仍在 → 收到。而 chat-chunk 的 listener 在事件处理函数里**紧贴 invoke 注册**，`finally { unlisten() }` 在 invoke resolve 后**立即移除** → 命令体内 emit 的事件投递延迟到命令返回附近、被抢先 unlisten → **全部丢失** → `firstChunk` 恒 true → 走 `showBubble(res.reply)` 一次性 fallback。Tauri 官方文档明确：emit/listen「不适合低延迟/高吞吐」，`ipc::Channel` 才是命令体内流式正解（内部用于 download progress，命令期间实时投递、有序、不经全局事件总线）。上一轮 Channel「用法对但 onmessage 不触发」≈ 踩 v2 经典坑：后端 `on_event`（snake_case）要前端传 **camelCase `onEvent`**，传错则 Channel 不注入、onmessage 静默不触发。

**修复**（2 文件，原则 #1/#5/#10/#11）：
- `commands.rs`：`send_message` 去 `app: AppHandle`，加 `on_chunk: tauri::ipc::Channel<String>`；闭包 `move |delta| on_chunk.send(delta.to_string())`（Channel move 进 FnMut）。诊断：首 chunk 一次 `[chat-stream] first content chunk forwarded to channel` + send 失败 warn（不刷屏）。`send_message` 无 Rust 测试调用方，改签名不触发踩坑#4。
- `App.tsx`：去 `listen("chat-chunk")`/`unlisten`/try-finally；`const onChunk = new Channel<string>(); onChunk.onmessage = (delta)=>{...}; invoke("send_message",{text, onChunk})`（**camelCase 严格**）。打字机（`streamBufRef` buffer + 30ms interval reveal）保留——仍需绕开 React 同 tick 批处理。`firstChunk` fallback 逻辑保留（沉默/空回复走 showBubble）。

**验证（端到端确认 ✅）**：`npx tsc --noEmit` ✅ / `npm run build` ✅ / `cargo check` ✅。**① Channel 投递探针**（临时 `stream_test` 命令 send 10 chunk + 前端 `pong` 回调，验证后删）：`sent 0..9 ↔ received 0..9` 一一对应、同秒 → Channel 投递实时工作（隔离 LLM，不受 DeepSeek 速率干扰）。**② 真实 send_message**：dev.log 见 `[chat-stream] first content chunk forwarded to channel`（Step 9 + Channel.send 被调）+ chat_stream 7s 流式（长回复）。**③ 用户实跑确认**：长回复（"讲个稍微长一点的"）气泡逐字浮现 ✅。短回复（"你好啊"）content 仅 1-2 chunk 瞬间发完看不出逐字（DeepSeek v4 reasoning content 占比小，非 bug）。

**清理**：删 `tests/stream_debug.rs`、删 client.rs `[stream]` SSE 调试 log（start/DONE/ended，保留 `[llm-stream-empty]`/`[llm-stream] skip malformed` warn）、删 App.tsx 自动触发 effect、`[send-token]` 改为首 chunk + warn。

**踩坑总结（写入避免重复）**：
- **emit/listen 不适合命令体内流式**：命令体内 emit 的事件投递延迟到命令返回；若 listener 紧贴 invoke 生命周期（invoke 后立即 unlisten），全部丢失。对比：listener 长期存活（useEffect 注册）的命令体内 emit 能工作（download-progress）。**命令体内流式一律用 `ipc::Channel`**。
- **Tauri v2 Channel 参数 camelCase**：后端 `on_event` ↔ 前端 `onEvent`，传 snake_case 则 Channel 不注入、onmessage 不触发（静默）——上一轮 Channel 失败的疑似根因。
- **dev 实跑验证流式受 DeepSeek 速率制约**：reasoning 模型 gate/extractor 分类慢（接近 60s timeout），叠加 catch-up（系统睡眠触发 reflection+consolidate）多路并发易 rate-limit/超时，send_message 卡 Step 1 不到流式。验证流式前确保 DeepSeek 可用 + 无 catch-up 干扰。
- DeepSeek-v4-pro 是 reasoning 模型：短回复（"你好"）content 仅 1 个 chunk（一次性发完，看不出逐字），**测试流式必须用长回复**（如"讲个故事"→83 chunk）；reasoning_content 占 ~70% completion token（content 只剩 30%）
- **React StrictMode 双 mount 陷阱（诊断坑，曾误导整轮）**：dev 下 effect mount→cleanup→mount。自动触发 effect 若加 `flag` 守卫防重复，第一次 mount 设 flag+timer → cleanup 清 timer → 第二次 mount 见 flag 跳过 → **timer 永不执行**。上一轮自主 dev 实跑 dev.log 总无 `[stream]`/`[chat-stream]` 的真因即此（自动触发没跑，非后端问题）。教训：dev 一次性自动触发 effect **别加 flag 守卫**，让 React cleanup 天然去重（第二次 mount 的 timer 触发）。

## §历史 (2026-07-28)：情绪外显·连续表情插值（P10 emotionBridge）

**起因**：用户选 §下一步候选 #2（北极星 #10 生命感）。codegraph 调研发现 `emotion/state.rs` 注释明写"continuous vector for Live2D parameter interpolation"、设计文档（implementation-plan.md:809-810）点名 eye_open/mouth_form/motion_speed——但实际链路断裂：emotion 向量被 `label_for_mood` 压成 6 桶离散表情（f00-f05）经 `model.expression` 硬跳，energy/stress/loneliness 几乎不影响视觉。后端 `emotion-update` 事件早 emit 完整向量（loop_runner.rs:96-108），前端 App.tsx 监听却只取 mood_label。顺带发现离散映射疑似语义错位（`MOOD_EXPRESSION_NAME.happy="f00"`→F01.exp3 的 `ParamMouthForm=-1.76` 哭脸）。

**实现**（3 文件，纯前端零后端改动，原则 #1/#5/#9/#10/#11）：
- 新增 `src/animation/emotionDriver.ts`：`EmotionVector` 接口 + `DEFAULT_EMOTION`（对齐 Rust `EmotionState::default`）+ `getEmotionParams(e)` 纯函数（eye_open=energy/rest_need 疲惫半眯 / mouth_form=mood↗+stress↘ / eye_form=mood 笑眼 / brow=mood 下垂）+ `boostForTransientExpression(expr,base)`（f00→mood↑ / f04→stress↑mood↓，让对话强情绪也走连续路径）。参数 id 全部经 Haru expression 文件（F01.exp3）验证存在；brow 方向经 F01（难过 -0.56）确认。常量集中可调 + JSDoc。
- `Live2DCanvas.tsx`：props moodLabel→emotionVector；移除 `MOOD_EXPRESSION_NAME`/`moodToExpressionName` 离散映射 + 移除 mood→expression useEffect；`beforeModelUpdateFn` 里 `{...getEmotionParams(emoVector), ...getBehaviorParams(beh,elapsed)}`（emotion 基线叠 behavior overlay 之下，behavior 优先）；transient 用 boost 而非 `model.expression`（避免预设表情残留 brow/form 参数）。
- `App.tsx`：emotionVector state（DEFAULT_EMOTION）+ `toEmotionVector` 辅助；4 处填充（emotion-update 事件 / 5s emotionTimer / 启动 / send_message 后）；传 Live2DCanvas。moodLabel 保留给 `bubbleClassForMood`。rest_need 后端未暴露→0（energy 已覆盖疲惫，follow-up）。

**架构契合**：#1 参数纯函数无 LLM / #5 连续参数每帧写不依赖 LLM（断网仍跑）/ #9 MVP 4 维刚好够用（browForm/Angle 等细分残留留 follow-up）/ #10 表情连续流动=生命感 / #11 每维映射有 JSDoc + 参数来源可追溯。Talking 时 lipsync 管 `ParamMouthOpenY`、emotion 管 `ParamMouthForm`（嘴角随心情）不冲突。

**验证**：`npx tsc --noEmit` ✅ / `npm run build` ✅（479 modules）。**待实跑**：npm run tauri dev 观察情绪变化（戳身体 stress↑→嘴角下垂、心情好→微笑+笑眼、累→半眯）。

**Scope 边界**（follow-up）：rest_need 后端暴露（EmotionResponse + emit 加字段）/ `applyBehaviorToModel` 的 `model.expression`（Yawn→f05 等）仍走预设，其 browForm/Angle 细分参数残留未被 emotion 覆盖（主干 browLY/RY 已 cover）/ ParamAngleY 头部俯仰未映射（loneliness/energy 表达）。

## §历史 (2026-07-27)：Soul 慢循环闭环（Reflection 自动调度 + thought 融入回来招呼 + Consolidation 调度）

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

## §部署：桌面启动方式（2026-07-28）
release exe 构建一次，桌面快捷方式双击启动（无需终端 / `npm run tauri dev`）。
- 构建：`npx tauri build --no-bundle`（**勿用** `cargo build --release`——后者产物 embed 不全、webview 加载异常）。`--no-bundle` 跳过 msi 打包（wix 可能失败，非必需），只出 exe。
- 产物：`D:\cargo-target\desktop-pet\release\desktop-pet.exe`（CARGO_TARGET_DIR 重定向到 D 盘，**非** `src-tauri/target/`；bin 名 `desktop-pet`，非 productName `DesktopPet`）。
- 桌面快捷方式：`C:\Users\SunJialei\Desktop\DesktopPet.lnk`（Target=exe, Icon=src-tauri/icons/icon.ico）。
- 踩坑1（已修）：`open_devtools` 是 Tauri **debug-only** API（release 下该方法不存在 → E0599）。commands.rs 已加 `cfg(debug_assertions)` 守卫，release no-op。
- 踩坑2（已修，release-only 隐蔽）：PIXI ShaderSystem 需 CSP `unsafe-eval`，但 tauri.conf.json CSP 原本只有 `wasm-unsafe-eval`（给 Live2D Core）→ PIXI Application 创建即崩 → 桌宠空白不显示。dev 模式 tauri 自动放宽 CSP（dev 正常），release 用配置 CSP 才暴露。诊断法：WebView2 设 `--remote-debugging-port=9222` + CDP `Runtime.evaluate` 抓异常。已加 `'unsafe-eval'` 到 `script-src`。
- 重建：改 Rust/tauri.conf.json → `npx tauri build --no-bundle`；改前端 → 先 `npm run build`。快捷方式自动指向新 exe（同路径覆盖）。

## §审计：P0-P17 + 架构原则完成度（2026-07-28，对照 implementation-plan v1.1 + design v2 + Architecture-Principles）

Kill List 三闭环全部端到端跑通（Body→Memory→Soul）。逐项审计（✅ 完整 / ⚠️ 有缺口 / ❌ 未做）：

| 阶段 | 状态 | 说明 / 缺口 |
|---|---|---|
| P0 脚手架/配置 | ✅ | AppData config（踩坑#1）|
| P1 数据库 | ✅ | schema v2，8 层记忆全 |
| P2 Embedding | ✅ | BGE-M3，AppData 引导下载 |
| P3 LLM 客户端 | ⚠️ | 非流式；**流式 chat_stream 未做**（client `stream:false`）|
| P4 Emotion | ✅ | state/homeostasis/needs/pace 全 |
| P5 摄入管道 | ✅ | gate/extractor/store/correction/working |
| P6 检索管道 | ✅ | trigger/retrieval/budget/grounding，score breakdown |
| P7 Planner | ⚠️ | director+actor 闭环；**流式逐字渲染未做** |
| P8 Pending | ✅ | 闭环2 实跑 |
| P9 Body 窗口 | ✅ | Live2D/透明/点击穿透 |
| P10 FSM | ⚠️ | fsm+emotionDriver(连续表情)+microBehavior ✅；**circadian 未接入微行为选择**；idle_weights 硬编码(非 JSON) |
| P11 交互 | ⚠️ | 摸头/戳/注意力三态 ✅；**气泡生命力简化(无形态/节奏)、Foley 音效未做、Alt+Space 全局键未做** |
| P12 物理 | ⚠️ | 空间(窝/回巢)/昼夜 ✅；**自由落体/任务栏弹跳简化(松手停原地)** |
| P13 Soul | ⚠️ | reflection/monologue/consolidation+慢循环闭环 ✅；**TurnThreshold/MajorEvent 触发器、Consolidation 反向更新 Facts 未做** |
| P14 感知 | ✅ | time/presence/window 模块全 |
| P15 Life Loop | ✅ | 三循环+recovery(前端catch)+firstrun 访谈 |
| P16 Debug Panel | ⚠️ | Brain/Counts/Facts/Episodes/Pending/Timeline ✅；**Prompt token/Retrieved score/Reflect/AnimFSM/Cost 分区缺** |
| P17 Golden | ⚠️ | golden_conversations 测试数据有；**evaluation 框架+人格漂移 score+CI 未完整** |
| A1 BrainState 快照 | ❌ | converse 多参数，未统一 BrainState（架构债）|
| A2 统一 Scheduler | ❌ | loop_runner 线程+sleep，非 Scheduler trait（架构债）|
| A3-A6 | ✅ | 直接调用+事件 / Change Log / Suspend-Resume / schema_version |

## §未解决问题
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。现在 `BubbleOutcome.anchor` 已暴露，Debug Panel 可顺手显示"当前冒泡锚定的记忆"。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选（按优先级重排，基于 §审计 + 北极星 #10 + Kill List 已完成）

Kill List 三闭环已完成，现按"提升体验/生命感"→"闭环深度"→"Body 完善"→"开发者基建"→"架构债"→"二期"排序。

**Tier 1 — 生命感/体验（#10 北极星，对话是核心交互）**
1. ✅ **流式回复**（已完成并实跑确认）：ipc::Channel 逐字渲染（短回复看不出逐字是 DeepSeek-v4 reasoning content 占比小，非 bug）。详见 §最近一轮。
2. ✅ **气泡生命力**（P11.3，已完成 `abb9d49`，待实跑）：打字节奏随情绪（`bubblePacing` 6 档）+ 无文字气泡（glyph 省略号/叹气，#12）。形态动画本就有 5 种 keyframes。「害羞慢现」缺后端 mood 标签未做（follow-up）。
3. **Foley 音效**（P11.5）：pet/poke/drag/walk/sit/sleep/land/click wav。几十 KB 音效对生命感提升 >> 文字。

**Tier 2 — Soul/对话深度（闭环增强）**
4. **converse 注入 surfaced thought**：正常对话也带出昨晚念头（现只 welcome-back）。
5. **Reflection TurnThreshold/MajorEvent 触发器**：每 30 轮 / importance>0.85 自动反思（现只 Daily）。
6. **Consolidation 反向更新 Facts**（#9 V2）：压缩总结中的事实回写 Facts。

**Tier 3 — Body 完善**
7. **circadian 接入微行为**：DeepNight sleepy 权重（现 fsm.tick 不接收 circadian）。
8. **完整物理**（P12.1）：自由落体 + 任务栏弹跳（现简化松手停原地）。

**Tier 4 — 开发者基建（#11 Explainability）**
9. **P16 Debug Panel 补全**：Prompt token / Retrieved score breakdown / Reflect(has_thought/unsurfaced) / Cost 分区。
10. **P17 Golden 评估框架**：人格漂移 score + CI 自动跑（现 golden 数据有，框架不完整）。

**Tier 5 — 架构债务（重构，功能已在跑）**
11. **A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表。
12. **A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14 二期清单）**
13. Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。
