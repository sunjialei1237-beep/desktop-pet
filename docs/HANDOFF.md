# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-08-03**

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
| 生命感 昼夜节律接入 | ✅ build 过 / 待今晚实跑 | circadian sleepiness 接入微行为权重（深夜 yawn↑/look_around↓，Tier3 #7）— PID 95248 挂着过夜 |
| 生命感 Foley 音效 | ✅ 实跑通过 | 真实 Foley 素材 10 接入 + 启动 hi + 权重静默优先 + cooldown + 亲密度分档；sleep 预留（Sleeping 未做）（Tier1 #3）|
| 对话 流式回复 | ✅ 实跑确认 | ipc::Channel 逐字（emit/listen 命令体内投递延迟+listener 立即 unlisten 全丢→Channel 正解）；用户长回复实跑确认逐字 |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）

> **2026-08-03 更新**：从 `D:\桌宠`（opencode 在本仓库副本上的工作）**合并** **B1 Consolidation 反向更新 Facts** + **B2 完整物理（自由落体/任务栏弹跳/1/3 飘落悬停）** + A4/A5 实跑方法论成果（CDP 自动化 + `Date.prototype.getHours` 重写模拟时段）。详见 §最近一轮 (2026-08-03)。两副本 base 完全一致（同 HEAD `50c45d2`，C/D 工作树在 grounding/reflection/Sleeping 等文件**逐字节相同**），故合并 = 纯增量复制 5 改文件 + 2 新文件（`gravity.ts`/`consolidation_harness.rs`），**零冲突**。验证：`cargo check --tests` ✅ / `cargo test --lib` **216 passed**（C 原 208 + B1 新增 8）/ `tsc --noEmit` ✅。清理了 harness 一处死变量（`ep_before`）。**当前无进行中任务**，下一会话按 B3（Sleeping 配套）→ B4（Debug Panel）→ B8 推进。
> **2026-07-31 18:01 更新**：三闭环 + 生命感主轴完成。**① 待验收代码层已全部闭环**——`cargo test --lib` 207 passed / `cargo check --tests` ✅ / `tsc` ✅ / `build` ✅，已 rebuild 进 18:01 release exe（含 A1/A2/A4 工作树 Rust 改动）。A1-A6 代码层 ✅、A7 勘误降级（未实现，单气泡覆盖）。**余下仅 GUI 运行时实跑**（A4/A5/A6 可立即验证；A1/A2/A3 需攒状态）——见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选) ①。**当前无进行中任务**，下一会话按 B1→B8 推进或先实跑 A4-A6。**主动开口幻觉已 A 档修复（19:10 rebuild，详见 §最近一轮）；残余：prompt 软约束无运行时阻断，B 档待命。**
**气泡 release rebuild 闭环（实跑确认 ✅ 2026-07-31）+ consolidation max_tokens 修复 + Reflection 触发器 Tier2 #5 + Sleeping 入睡机制（build 过 / 待实跑）。** 气泡：release exe 落后 dev 2 天，rebuild 后用户实跑确认居中。consolidation：生成任务 max_tokens 2048→4096（踩坑#3 复发）+ 空 content 防御。Tier2 #5：Reflection 事件驱动触发器（TurnThreshold 30 条对话记忆 / MajorEvent importance>0.85，1h 冷却，Daily→MajorEvent→TurnThreshold）。Sleeping：DeepNight(2-6) 无交互≥10min 自动入睡（forceState），交互（戳/摸/拖/对话/双击）markInteraction 唤醒 + 刷新 lastInteraction（天然 10min 清醒冷却）。后端 `cargo test --lib` 207 passed / 全 harness 编译 ✅；前端 `tsc`+`build` ✅。**下一步**：实跑 #4 converse thought / circadian 深夜 / 实跑 Sleeping（改系统时间 2-6 点+等 10min）/ 多气泡堆叠 / Tier2 #6。注：consolidation(≥100 episodes)/Reflection 触发器日常不易快速触发；Sleeping 需改系统时间到 DeepNight 验证。**全部已 rebuild 进 release exe（07-31 13:03），桌面快捷方式已含**；气泡已实跑确认，其余待择机实跑。

## §最近一轮 (2026-08-03)：合并 opencode 副本（B1 + B2 + A4/A5 实跑方法论）

**任务**：用户指出 `D:\桌宠` 是 opencode 在本仓库副本上做的改动（"主要落地与回位"），要求对比、打分、把不合适的部分修改后合并。

**对比方法（关键）**：两副本同 HEAD（`50c45d2`），逐文件 diff C 盘工作树 vs D 盘工作树。结果——grounding.rs / proactive.rs / system.txt / reflection.rs 四文件**逐字节相同**（证明 opencode 完整继承了我 07-31 的工作，base 一致）。真正增量仅在 4 块：① B2 物理（gravity.ts 新 + spatial.ts + App.tsx）② B1 Consolidation 反向更新 Facts（consolidation.rs + facts.rs + harness）③ App.tsx `data-behavior` 插桩 ④ HANDOFF。故合并 = 纯增量复制，零冲突。

**opencode 增量打分**：

| 增量 | 分 | 评 |
|---|---|---|
| B1 Consolidation→Facts backfill | **8.5** | 架构契合极好（#1 LLM 只提议 JSON、Rust 验证 category 白名单+confidence clamp 写库 / #8 低频可接受 / #11 source_episode 可追溯+失败 log）。prompt 质量高（明确"不推断"、中文 key 利于合并、confidence 分档）。冲突检测=expire_old+dedup_insert 是 V2 合理 MVP。失败隔离（backfill 失败只 warn 不阻断已成功的压缩）。单测全面（parse/write/dedup/revive）。max_tokens 4096（踩坑#3 已规避）。**唯一点**：每批 consolidation 多 1 次 LLM 调用（prompt 复用 summary，可接受设计权衡）。 |
| facts.rs dedup revive（B1 配套） | **8.5** | 修真实边缘 bug：过期同值事实"复活"撞 UNIQUE(category,key,value) → 改 UPDATE 复活原行保 mention 历史 + 测试。 |
| B2 物理：bug 修复 + ref 重构 | **9** | **重要发现**：原生 `startDragging` 吞掉 webview 所有鼠标事件，旧 `onUp`(mouseup) 是**死代码**——拖拽后 petPos 从未同步、land 音效从未真正播过（HANDOFF 07-31"Foley 落地 onUp"记录不准）。改用 `onMoved` 事件同步位置 + rAF 静止检测（300ms 静止+半空→落体）。petPos useState→petPosRef 重构正确解决 state 依赖致 rAF 每帧重建/dt 抖动/视觉卡顿。onMoved 节流 100ms 修 refreshOrigin IPC 洪水。floorY=workArea 底部（任务栏上沿）。经 CDP+Win32 真实鼠标注入实测验证轨迹。 |
| B2 物理：gravity.ts + 1/3 飘落 | **7.5** | 纯函数清晰、常量集中、#1/#5 契合。**待确认**：① gravity.ts 宣称"任务栏弹跳"（BOUNCE_DAMPING/BOUNCE_STOP_VY），但 App.tsx 的 `fallLimitBottomRef`（用户 08-01 偏好"飘落"）让她只落 1/3 距离就 grounded → 反弹分支永不触发 = **bounce 是配置下死代码**（逻辑健全但被覆盖）。② `GRAVITY = 1200/9` 魔法算式可读性弱（注释有 sqrt 推导，但不如直接 133）。③ `stepGravity` 原地 mutate g（违反全局 immutability 规范，但物理循环务实，可接受）。 |
| spatial.ts RETURN_DELAY 30→900 | **10** | 1 行，用户明确要求（拖走 15min 才回巢，配合飘落悬停）。 |
| A4/A5 实跑方法论 | **9** | `Date.prototype.getHours` 重写模拟 DeepNight（绕开改系统时间，无 UAC、秒级切换）+ CDP 自动化（WebView2 `--remote-debugging-port` + Node 原生 WebSocket）+ `data-behavior` DOM 插桩——**方法论资产**，后续所有"需改系统时间/需 GUI 实跑"的验证都能复用。 |
| HANDOFF 更新 | 8.5 | 记录详实，但视角是 opencode（合并时已改成"合并自副本"）。 |

**总评 ~8.3/10**：核心价值高（B2 修了一个隐蔽的死代码 bug + B1 架构干净），缺陷集中在 B2 的 1/3 配置遗留（bounce 死代码、魔法常量）——非阻断，记入 follow-up。

**关键技术吸收（写入避免重复/复用）**：
1. **`startDragging` 吞 webview 鼠标事件**（B2 踩坑）：Tauri 原生 `win.startDragging()` 把拖拽交给 OS 合成器，期间 webview 收不到任何 mouseup/mousemove → 拖拽结束**不能用** mouseup 检测，必须用 `win.onMoved()` + 静止期。旧 onUp 路径全是死代码。
2. **`Date.prototype.getHours` 重写**（A4/A5 验证法）：模拟时段不需改系统时间。`circadian.ts` 是唯一 getHours 调用点，重写后 ~16ms 生效；`Date.now()` 不受影响（入睡计时仍走真实时钟）。验证后恢复原函数。
3. **CDP 自动化**：`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 启动 release exe → Node 22 原生 WebSocket 查 DOM/截图/派发点击。脚本断线必须 `ws.onclose/onerror→exit(2)` 否则挂起。
4. **物理循环必须 ref 驱动**：rAF 回调若依赖 React state（petPos），每次 setState 触发 effect 重建 → lastTime 重置 → dt 毛刺 → 卡顿。位置一律走 ref。

**合并执行**：纯增量复制 6 文件（gravity.ts/spatial.ts/facts.rs/consolidation.rs/App.tsx + consolidation_harness.rs）D→C，C/D 逐字节校验一致。清理 harness `ep_before` 死变量（opencode 调试残留）。`cargo check --tests` ✅ / `cargo test --lib` **216 passed** / `tsc` ✅。

**待确认（非阻断，入 follow-up）**：
- **bounce 死代码**：gravity.ts 的弹跳逻辑在当前 1/3-飘落配置下永不触发。若用户确认"只要飘落不要弹跳"，可删 BOUNCE_* 常量+反弹分支；若两者都要，需让 1/3 规则可配置或仅对贴近 floor 的释放生效。
- **B1 多 1 次 LLM**：已接受（#8 低频）。若要省，可让 consolidation 那次 LLM 同时输出 facts（prompt 合并），省一次往返。
- **A6 弃测**：emotionBridge 连续表情 opencode 标"用户弃测（过渡角色）"——等最终 Live2D 角色交付后补测。

---

## §最近一轮 (2026-07-31 19:10)：主动开口幻觉 → grounding A 档收紧

**症状**：用户报桌宠主动开口说「你那个小睡衣项目怎么样了？」——把一件从未发生的事当真实记忆来追问（闭环3「她记得我」的反面：记得假事）。

**排查（三层 0 命中）**：代码/prompts 无"睡衣"；全数据库表（conversations 0、episodes 13、facts 35、reflections 6、internal_thoughts 12、pending 2）无"睡衣"。→ 纯幻觉。

**根因（纠正一处误判）**：主动开口路径**并非没有 grounding 约束**——`proactive`/`welcome_back` 经 `budget::allocate_and_compress` 注入了 system.txt 规则 8 + `MEMORY_CONSTRAINT`。失效在于：① **任务压力压过禁令**：规则 12 + proactive user prompt 强压"必须带出一个记忆并提问"，当检索到的真实记忆（如 `[work/current_project] desktop pet project`）不够具体/不好聊时，LLM 为完成"主动关心"而编造细节 ② **输出端零兜底**：`check_groundedness` 只挂 `converse`（且只 `warn` 不阻断），`proactive`/`welcome_back` **完全没挂**——编造的输出直达用户、连日志都没有 ③ **约束力打折**：规则 8 / `MEMORY_CONSTRAINT` 是英文、生成是中文，跨语言约束力下降。

**A 档修复（用户选 A：治本-prompt 收紧，3 处）**：
- `system.txt` 规则 8：英文禁令后追加**中文强禁令**（不得把记忆改写成别的项目名/事件、不得虚构、没合适话题就只简单招呼、不硬找话题不猜）。
- `proactive.rs::generate` user prompt：加运行时**锚点围栏**（只能围绕锚点原意、不得换成别的项目/编造），顺带修掉指向不存在的「规则 8/8a/8b」stale 引用 → 「尤其规则 8」。
- `proactive.rs::generate_welcome_back` anchor_clause：加同样围栏。
- `grounding.rs`：加 `test_system_prompt_contains_chinese_grounding_ban` 断言（防回归）。

**验证**：`cargo test --lib` **208 passed**（+1）✅ / release exe rebuild 19:10 ✅。**残余风险**：prompt 是软约束、无运行时阻断（那是 B 档）；若实跑仍偶发幻觉 → 升级 A+B（`check_groundedness` 加中文 claim + 主动开口输出端阻断）。

**附带发现（未修，入 backlog）**：① `conversations` 表生产路径 0 写入（`conversations::insert` 仅测试调用）= 死表，导致本次无法回溯她原话（#11 可追溯受损，C 档）② `check_groundedness` claim_patterns 全英文、中文漏检（B 档）。

---

## §最近一轮 (2026-07-31 续)：气泡 release rebuild + consolidation 修复 + Reflection 触发器 + Sleeping 入睡

**起因**：用户报"启动打招呼气泡位移没修好"。诊断：release exe（07-29 16:58）落后 3329d85（07-31 CSS translate 居中修复）2 天——上一轮"气泡实跑通过"仅 dev HMR 验证（自动热更 CSS），release exe 从未 rebuild，快捷方式跑老代码（`transform: translateX(-50%)` 被 bubble-* 动画 transform 覆盖→偏右跳变）。CSS 层面 3329d85 正确（`translate` 属性独立于 `transform`、无冲突声明），**纯打包问题，代码不动**。taskkill（PID 108248，踩坑#6 释放 exe 锁）→ `npx tauri build --no-bundle` → exe 07-31 12:10。用户双击快捷方式实跑确认居中、不再跳变（Foley 接线/circadian/converse thought 5 commit 一并进 release）。

**顺势修 consolidation llm-empty bug（踩坑#3 复发）**：`consolidation.rs:87` 生成任务（压缩总结）传 `max_tokens=2048`——reflection/extractor 同为生成都 4096，gate/correction 分类才 2048，consolidation 是**唯一**生成任务用 2048 的。DeepSeek-v4 reasoning 独占预算→content 空→静默写空白 episode（consolidation 返纯文本不解析 JSON，无 reflection 的 parse-fail 兜底）。修：① 2048→4096 ② 空 content 防御（warn + return Ok(0)，不写垃圾、下轮重试）。`cargo test --lib` **199 passed**。**待实跑**：触发需 episodes≥100，日常不易达；Rust 改动待下次 rebuild 进 release。

**Reflection 触发器（Tier2 #5，单测过 ✅）**：`ReflectionTrigger` 枚举早预留 TurnThreshold/MajorEvent 变体但只 Daily 被触发。补事件驱动触发：① TurnThreshold——自上次 reflection 起 ≥30 条 conversation episode ② MajorEvent——自上次 reflection 起有 importance>0.85 的 episode。两者共用独立 1h 冷却（Daily 仍 20h），`maybe_run_if_due` 优先级 Daily→MajorEvent→TurnThreshold 取首个命中。轮次源用 episodes（`conversations` 表零调用是死表、`total_conversations` 无基准），只数 source_type='conversation' 的记忆（gate 拦掉的不算）。抽 `last_reflection_at` helper 复用、重构 `should_run_reflection`。**不改签名**（maybe_run_if_due/run_reflection/ReflectionTrigger 签名都不变，slow_tick/commands 零改动，规避踩坑#4）/ **#8 成本**：事件驱动最多 1次/h + Daily 1次/天。`cargo test --lib` **207 passed**（+8：5 turn_threshold + 3 major_event，覆盖不足/达阈值/非 conversation/冷却内/无历史 + 高/低 importance）/ 全 harness 编译 ✅。**待实跑**：需攒 30 条对话记忆或高 importance 事件；Rust 改动待下次 rebuild 进 release。

**Sleeping 入睡机制（Tier3 续，build 过 ✅ / 待实跑）**：`Sleeping` 状态渲染全套早就绪（Live2D f05 / behaviorDriver 慢呼吸 / PetCharacter sleeping+Zz / styles sleeping 动画）但**从未被自动触发**——缺触发+唤醒。实现（纯前端 App.tsx）：① 入睡——FSM tick 定时器(2.5s)检查 `circadian.period===DeepNight(2-6) && state!==Sleeping && !isThinking && !Talking && Date.now()-lastInteractionRef>SLEEP_AFTER_IDLE_MS(10min)` → forceState(Sleeping) ② 唤醒——markInteraction()（摸/戳/拖/对话/双击 5 处入口）：刷新 lastInteractionRef + 若在睡 forceState(Idle)（forceState 绕过 transition 优先级锁——Sleeping 不可中断态只有 forceState 能出）③ 清醒冷却——唤醒刷新 lastInteraction 天然让 10min 内不再入睡。**不改 fsm/circadian/behaviorDriver**（渲染早就绪，只补 App.tsx 触发+唤醒）/ **#10 生命感**：她有睡眠周期 / **#1 纯规则无 LLM** / **#5 Body 层独立**。`tsc --noEmit` ✅ / `npm run build` ✅（1.96s）。**待实跑**：改系统时间到 2-6 点（DeepNight）+ 不交互等 10min → 观察入睡（闭眼慢呼吸+Zz）；戳/摸/对话 → 即时唤醒。follow-up：Sleeping 时抑制 DeepNight nudge 气泡（现睡着仍冒"早点睡"，像梦话）；LateNight(22-2) 不入睡只 yawn（现有）；Sleeping 音效（sleep 素材预留未接）。

**踩坑（写入避免重复）**：**dev HMR 验证 ≠ release exe 已含修复**——前端/CSS 改动 dev 下热更"看着修好"，但桌面快捷方式（release exe）不自动更新，必须 `npx tauri build --no-bundle`。涉及前端/CSS 的"实跑通过"必须在 release exe 上确认（本次气泡 + 上次 Foley 同一坑）。诊断 release 行为先比对 exe LastWriteTime vs commit 时间。

---

## §历史 (2026-07-31 早些)：Foley 接线补全 + 频率调整 + 气泡位移（实跑通过 ✅）

**起因**：用户实跑发现"音效完全没声"+"气泡启动偏右再跳变"+"戳身没声"。诊断：① Foley 的 **App.tsx 接线从未落地**（soundManager 孤立单例，无 import/调用；ContextMenu 要的 soundMuted/onToggleSound props 也没传，TS 报错被 vite esbuild 静默忽略）→ 旧 §历史"Foley 实跑通过 9 触发点"记录错误 ② 气泡 `bubble-*` 动画 keyframes 覆盖 transform 且不含 translateX(-50%)，动画期间丢居中→偏右，结束跳回居中 ③ 戳身无声 = HMR 后 sound 单例重建 buffers 空 + mount effect preload 不重跑 + poke cooldown 5000ms，首次现场加载无声后连戳全被 cooldown 拦。

**实现**（4 文件 + 素材，原则 #6/#10/#11）：
- `App.tsx`：补全 sound 接线——`import {sound, INTIMATE_THRESHOLD}` + `soundMuted` state + mount effect（`sound.preload()`+`sound.greet()`）+ mute sync + 8 触发点（摸头按 closeness 分档/戳按 pokeCount 分档 poke1-3/drag onMove/land onUp/send/dblclick）+ `handleContextMenu` 加 `sound.play("menu")` + ContextMenu 传 soundMuted/onToggleSound。
- `soundManager.ts`：权重调整（UI 类 dblclick/menu/send 必响；活物音 pet/poke/drag/land 出声概率 +15-20%，仍保留随机）+ 新增 `menu` trigger + poke cooldown 5000→2000（修戳身）。
- `ContextMenu.tsx`：soundMuted/onToggleSound props（此前 working tree 已改但 App.tsx 未接）。
- `styles.css`：气泡居中改用 `translate: -50% 0` 属性（独立于 transform），动画 transform 不再覆盖居中——`.chat-bubble`/`.bubble-pet`/`.hidden` 三处。
- `public/audio/`：11 素材（10 接入 + sleep 预留）。

**验证（实跑通过 ✅ 2026-07-31）**：tsc ✅ / dev HMR ✅ / 音频文件 fetch 全 200 / 用户确认：摸头/戳/拖动/落地/双击/右键/发送均有声 + 频率手感 OK + 戳身 cooldown 降后响 + 气泡不再偏右跳变。

**踩坑（写入避免重复）**：① **vite dev 用 esbuild 不做类型检查**——TS 错误（如缺 props）不阻断运行，运行时静默失败。改前端接线后必须 `tsc --noEmit` 验证，不能只看 dev 能跑 ② **HMR 后模块单例重建 + mount effect 不重跑**——sound 单例 buffers 清空、preload 不重新触发，首次交互现场加载。dev 下改音效代码后 F5 刷新；release 无 HMR 不受影响 ③ **CSS animation transform 覆盖元素 transform**——居中若靠 `transform: translateX(-50%)`，动画 keyframes 的 transform 会盖掉导致跳变。解法：居中用独立 `translate` 属性。

---

## §历史 (2026-07-31)：converse 注入 surfaced thought（Tier2 #4，build 过 / 待实跑）

**起因**：用户"按执行计划进度继续开发"。HANDOFF §下一步候选 Tier2 #1（北极星 #10 + Soul 深度）。审计发现 `surface_thoughts`（monologue.rs:18）只在 `generate_welcome_back`（proactive.rs:286）消费——**只有用户离开回来才浮现昨晚念头；正常对话从不带出**，thought 若用户没离开/没触发 welcome-back 就永远积压。

**实现**（1 文件 ~20 行，纯内联，原则 #1/#8/#10/#11）：
- `converse.rs`：Step 8 reminder bridge 之后、user message 之前加 `thought_clause`——调 `surface_thoughts(db)` 取首个 next_interaction thought，拼 system message 注入。措辞**克制**（区别 welcome-back 的"招呼里带出"）：「话题自然关联才轻带，无关别强提，正常聊」——避免她每轮翻昨晚念头。Err 降级空串（不阻断对话）。
- **不改签名**（踩坑 #4 规避，converse 内部加逻辑，send_message + 全 harness 零改动）/ **不增 LLM 调用**（#8 复用同一次 converse 的 LLM turn）/ **消费性**（surface_thoughts mark surfaced；welcome-back 与 converse 谁先触发谁消费，thought 只浮现一次，自洽）/ **#11 可追溯**（`[converse] surfaced thought` log）。

**架构契合**：#1 Rust 拼 prompt（含克制措辞），LLM 只配音 / #8 零额外 LLM / #10 正常对话也"记得昨晚念头"=生命感 / #11 surfaced log 可追溯。

**验证（build 过 ✅ / 待实跑）**：`cargo test --lib` **199 passed**（0 failed）/ `cargo check --tests` 全 harness 编译 ✅（17.99s）。**待实跑**：`npm run tauri dev`，等 reflection 产 thought（或手动插一条 next_interaction thought 到 DB）→ 对话观察她自然带出。

**Scope 边界**（follow-up，避免过度）：① thought 浮现无频率门控（每次对话 surface_thoughts 取 1）——现 thought 产出稀疏（reflection 每日≤1），不构成问题，多了再加"每 N 轮最多 1 次"门控 ② converse 无 thought 专用 harness（需完整 AppState 构造，重），靠 surface_thoughts 既有单测 + 实跑覆盖 ③ ConversationResult 未加 has_thought 字段（log 已够 #11，减少改动面）。

---

## §历史 (2026-07-29)：Foley 音效 — 真实素材接入（实跑通过 ✅，Tier1 #3 完成）

**起因**：用户提供 11 个真实 Foley 素材（桌面/新建文件夹，中文 mp3）。此前合成柔软占位在跑（见本节末历史）。

**素材映射（原名 → public/audio 语义名 → 角色）**：
- voice/surprise-soft（ow：轻微意外/疑惑）｜voice/startle-short（啊1 短促：戳受惊）｜voice/soft-ah（啊 稍长：摸头满足）
- voice/annoyed（生气：连戳3+）｜voice/laugh（笑：开心/亲近摸头）
- foley/cloth（布料声）｜foley/land（落地声）｜foley/lift（跳：抓起）｜ui/send（UI音效：发送）
- voice/greeting（hi：启动打招呼）｜voice/sleep（睡觉声：**仍预留**，Sleeping 入睡机制未做）

**核心设计（#10 宁少勿突兀 + #11 集中可调）**：
- `soundManager.ts` 重写为**纯 wav 加载**（fetch public/audio + decodeAudioData + AudioBufferSourceNode），**移除原 Web Audio 合成**。
- **权重随机 + 静默优先**：每 trigger 最大权重是"静默"（摸头陌生 静默70/布料20/ow10；亲近 静默45/啊25/笑15/布料15）。出声=惊喜非常态。
- **cooldown 出声/静默都计时**：rapid tap 不能叠声。摸头3s / 戳5s / 拖动0.8s / 落地0.6s / 发送0.4s / 双击1s。
- **亲密度分档**：摸头按 `closenessRef`（前端现成，15s 拉）≥ `INTIMATE_THRESHOLD`(40) 选 pet-intimate（啊/笑）vs pet-stranger（布料/ow）。声音=关系成长。
- **戳递进**：复用已有 `pokeCountRef` n=1/2/3+，音效跟随（poke1 ow/啊1 → poke2 啊1 → poke3 生气）。
- `preload()`：App 启动 mount effect 预热所有 buffer，首交无 fetch 延迟。

**触发点（App.tsx 9 处）**：摸头(773 按 closeness 分档) / 戳(801 按 n 分档) / 拖动(596) / 落地(609) / 发送(649 click→send) / 双击开对话(888 新接 dblclick) / **启动招呼（preload effect 调 `sound.greet()`）**。drag/land id 不变。ContextMenu 静音项不变（#6）。

**启动招呼 `greet()`（autoplay 处理）**：启动无用户手势 → AudioContext `suspended` 直接播不出。`greet()` 启动即尝试播 hi；若 ctx 挂起，挂一次性 `pointerdown` listener，首次互动 resume ctx 后补播（保证总能听到招呼）。`greeted`/`greetArmed` 双 flag 防 StrictMode 双 mount 重复。抽出 `playSample(key,ctx)` 给 play/greet 共用。**素材现状**：11 个接 10（含 greeting），仅 sleep 未接（Sleeping 未做）。

**验证（实跑通过 ✅ 2026-07-29）**：tsc ✅ / build ✅（2.52s）/ dist/audio 11 文件全打包 ✅。**CSP 排查**：`connect-src 'self'` 含同源 → fetch public 资源 release 不拦（同 Live2DCanvas `/live2d/` 模式），无 release-only 坑。`npm run tauri dev` 实跑：用户确认 9 触发点 + 启动 hi + 亲密度档效果不错。

**踩坑**：`import.meta.env.BASE_URL` 无类型（项目 tsconfig 无 vite/client），改硬编码 `/audio/`（同 Live2DCanvas 既有模式）。

**follow-up**：hi/sleeping 预留未接；走路脚步声 loop、Sleeping 入睡机制同期待办；权重/cooldown/阈值全集中 `soundManager.ts` 的 `TRIGGERS`，实跑后按手感调。

---

### 历史（已被真实素材取代）：合成柔软占位

**起因**：用户选 §下一步 Tier1 #3（北极星 #10）。审计 P11 标记"Foley 音效未做"，此前零音频代码。

**素材试错（关键教训）**：
1. 下载 CC0：从 [Kenney Interface Sounds](https://github.com/Calinou/kenney-interface-sounds)（GitHub raw 可 curl，避 Freesound 需登录）挑 5 个 UI 音（pluck/tick/maximize/drop/select）→ **实跑用户反馈"太难听、都很尖锐"**。根因：UI 音为界面反馈设计，高频瞬态、清脆冰冷，放柔软桌宠上刺耳。**下载素材风格不可控、无法预听筛选**——这正是当初推荐合成的核心理由。
2. 改 Web Audio 合成：sine（几乎无谐波）+ 低频基音 + 低通滤波 + 慢 attack（无瞬态）+ 长 release = 物理上不可能尖锐。每音参数集中可调（`SPECS`）。
3. 用户决定**自找真实 Foley 素材** → 本项 ⏸️ 列为待办。

**已落地（保留，不再折腾）**：
- `src/audio/soundManager.ts`：纯 Web Audio 合成版（`SPECS` 5 音参数 + `synth()` + `muted`/`toggleMuted`）。**已删 `public/sounds` wav**（不再用）。结构预留：未来加 wav 只需 `play` 里加"wav 优先 fallback 合成"分支，`synth` 已独立
- `App.tsx`：5 触发点接线（`handleHeadClick`→pet / `handleBodyClick`→poke / `handleDragStart` onMove→drag + onUp→land / `handleSend`→click）+ `soundMuted` state + `toggleSound` + ContextMenu 传参
- `ContextMenu.tsx`："静音/开启声音"项（#6）

**架构契合**：#5 Body 层零 LLM 依赖（断网照响）/ #6 右键静音，muted 时 no-op / #8 本地 DSP 零成本 / #10 交互有声=活物感。

**验证**：`tsc` ✅ / `build` ✅（481 modules）/ release exe 重建（PID 98956）。合成柔软占位在跑。

**⏸️ 待办（用户自找素材后接入）**：用户提供 pet/poke/drag/land/click 真实 Foley wav → soundManager 加 wav 加载分支（play 优先 wav、fallback 合成）+ drop `public/sounds/` + 重建。走路脚步声 loop（需 isWalking 周期触发）同期待办。

## §历史 (2026-07-29)：circadian 接入微行为（Tier3 #7）

**起因**：用户选 §下一步 Tier3 #7（北极星 #10 生命感）。审计 P10 标记"circadian 未接入微行为选择"——`getCircadianState()` 每帧算出 `sleepiness`（Morning 0.1 / DeepNight 0.9）但 `fsm.tick(App.tsx:199)` 根本没接收；`microBehavior` 的 `IDLE_BEHAVIORS` 权重写死、与时段无关 → 深夜桌宠和白天一样精神。顺带发现 `Sleeping` 状态渲染/参数全就绪（behaviorDriver 闭眼慢呼吸 / Live2D f05 / PetCharacter Zz）但**从未被自动触发**（无人 forceState，微行为池无 sleep 项）。

**实现**（3 文件，纯前端零后端，原则 #1/#9/#10/#11）：
- `microBehavior.ts`：`IdleBehavior` 加可选 `sleepy?: number`（困倦权重倍数，JSDoc 固化公式语义 + 数学预期）；`IDLE_BEHAVIORS` 8 项填值；`PickOptions` 加 `sleepiness`；权重循环加 `w *= 1 + (sleepy-1)*sleepiness`（`Math.max(0.01,w)` 兜底非负）
- `fsm.ts`：`tick` 签名加 `sleepiness: number`（closeness 后 now 前），透传进 `pickBehavior` 回调 opts
- `App.tsx`：`fsm.tick(...)` 调用喂入 `circadianRef.current.sleepiness`（ref 每帧已在 :542 更新）

**架构契合**：#1 纯规则无 LLM / #9 MVP 单标量 sleepiness 线性插值刚好够用 / #10 深夜她真的犯困=生命感 / #11 sleepy 字段 + 公式 JSDoc 可追溯。

**验证（build 过 ✅ / 待实跑）**：`tsc --noEmit` ✅ / `npm run build` ✅（480 modules）。**数学验证**（全池可见、closeness 足够）：白天 sleepiness=0.1 → yawn 占比 ~10.7%、look_around ~17.8%；深夜 sleepiness=0.9 → yawn ~32.2%（3×↑）、look_around ~6.5%（↓）。sleepiness=0 乘子=1 白天不变。**待实跑**：改系统时间到 2-6 点（DeepNight）观察 yawn 频率上升。

**Scope 边界**（follow-up，避免过度）：① `speedModifier`/`energyModifier` 未接动画速度/能量（circadian.ts 注释声称输出 speed，但 behaviorDriver 未消费）② **Sleeping 自动入睡/唤醒机制未做**——权重调制让深夜多 yawn，但不会真正 forceState(Sleeping)；Sleeping 是持续态（fsm tick 不自动退出），完整入睡需配"用户交互（戳/摸/对话）唤醒"机制，是更大设计 ③ idle_weights 仍硬编码（非 JSON 配置），可调但非数据驱动。

## §历史 (2026-07-29)：气泡生命力 — 打字节奏随情绪 + 无文字气泡（Tier1 #2）

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
| P10 FSM | ⚠️→✅ circadian | fsm+emotionDriver(连续表情)+microBehavior+circadian sleepiness 接入 ✅；idle_weights 硬编码(非 JSON，可调) |
| P11 交互 | ⚠️ | 摸头/戳/注意力三态 ✅；气泡生命力(节奏+glyph) ✅；Foley 音效 5 音 ✅；**走路脚步声 loop、Alt+Space 全局键未做** |
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

> ⚠️ **本节为 07-28 快照，已过时**（Tier1 三项全完成、Tier2 #4/#5 已做）。最新统一优先级 backlog 见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选)。保留下方作历史对照。

Kill List 三闭环已完成，现按"提升体验/生命感"→"闭环深度"→"Body 完善"→"开发者基建"→"架构债"→"二期"排序。

**Tier 1 — 生命感/体验（#10 北极星，对话是核心交互）**
1. ✅ **流式回复**（已完成并实跑确认）：ipc::Channel 逐字渲染（短回复看不出逐字是 DeepSeek-v4 reasoning content 占比小，非 bug）。详见 §最近一轮。
2. ✅ **气泡生命力**（P11.3，已完成 `abb9d49`，待实跑）：打字节奏随情绪（`bubblePacing` 6 档）+ 无文字气泡（glyph 省略号/叹气，#12）。形态动画本就有 5 种 keyframes。「害羞慢现」缺后端 mood 标签未做（follow-up）。
3. ✅ **Foley 音效**（P11.5，已完成实跑通过）：真实素材 10 接入（ow/啊/啊1/生气/笑/布料/落地/跳/UI/hi）+ 权重静默优先 + cooldown + 亲密度分档 + 启动招呼(autoplay 补播)；sleep 预留（Sleeping 未做）。详见 §最近一轮。

**Tier 2 — Soul/对话深度（闭环增强）**
4. ✅ **converse 注入 surfaced thought**（已完成，build 过 / 待实跑）：正常对话也带出昨晚念头。converse Step 8 后注入克制措辞的 thought_clause（#8 零额外 LLM、消费性与 welcome-back 自洽）。详见 §最近一轮。
5. **Reflection TurnThreshold/MajorEvent 触发器**：每 30 轮 / importance>0.85 自动反思（现只 Daily）。
6. **Consolidation 反向更新 Facts**（#9 V2）：压缩总结中的事实回写 Facts。

**Tier 3 — Body 完善**
7. ✅ **circadian 接入微行为**（已完成，build 过 / 待实跑）：sleepiness 调制 idle 权重（深夜 yawn↑/look_around↓）。详见 §最近一轮。follow-up：speedModifier 未接动画速度；Sleeping 自动入睡/唤醒机制（现只调权重，未真正入睡）。
8. **完整物理**（P12.1）：自由落体 + 任务栏弹跳（现简化松手停原地）。

**Tier 4 — 开发者基建（#11 Explainability）**
9. **P16 Debug Panel 补全**：Prompt token / Retrieved score breakdown / Reflect(has_thought/unsurfaced) / Cost 分区。
10. **P17 Golden 评估框架**：人格漂移 score + CI 自动跑（现 golden 数据有，框架不完整）。

**Tier 5 — 架构债务（重构，功能已在跑）**
11. **A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表。
12. **A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14 二期清单）**
13. Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

---

## §下一步总清单（2026-07-31，统一优先级 · 取代上方 §下一步候选）

> **权威 backlog。** 上方 §下一步候选 是 07-28 快照（Tier1 已全完成、Tier2 #4/#5 已做），仅作历史对照。
> Kill List 三闭环全部端到端跑通（活着 Body → 记住你 Memory → 懂你 Soul）。
> 排序驱动：北极星 #10（优先生命感不优先功能）+ 优先级阶梯（活着→记住→懂你→工具砍）+ 实施计划 P0-P17 / A1-A2。
> 两类工作：**① 待验收**（已编码、收尾即闭环，最高 ROI）→ **② 待开发**（按 Tier 优先级）。

### ① 待验收（代码层已全部验收 ✅ 2026-07-31 18:01；GUI 实跑待用户）

> **代码层闭环**：`cargo test --lib` **207 passed** / `cargo check --tests` 全 harness 编译 ✅ / `tsc --noEmit` ✅ / `npm run build` ✅（2.12s）。**全部已 rebuild 进 release exe**（`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 07-31 18:01，含工作树未提交的 A1/A2/A4 Rust 改动；桌面快捷方式自动指向）。A1-A6 代码层验收通过，余下仅 GUI 运行时实跑（见"运行时实跑"列）。

| # | 项 | 代码层验收 | 运行时实跑（用户） |
|---|---|---|---|
| A1 | consolidation max_tokens 修复 | ✅ `consolidation.rs:89` `Some(4096)` + `:97-103` 空 content 防御 | 需攒 ≥100 低 importance episodes 自然触发，难快速复现（不必强测） |
| A2 | Reflection TurnThreshold/MajorEvent 触发器 | ✅ 优先级 Daily→MajorEvent→TurnThreshold + 12 单测全过 | 需攒 30 条对话记忆 或 importance>0.85 事件 |
| A3 | converse 注入 surfaced thought | ✅ `converse.rs:202-221` 注入 + 消费性 | 需 reflection 先产 thought（一日以上），下次对话观察带出 |
| A4 | Sleeping 入睡/唤醒 | ✅ `App.tsx:216-222` 入睡 + `:604-607` 唤醒 | **可立即验证**：改系统时间 2-6 点 + 不交互 10min→入睡；戳/摸/对话→唤醒 |
| A5 | circadian sleepiness 调权重 | ✅ `microBehavior.ts` sleepy 公式 + `App.tsx:226` 喂入 fsm.tick | **可立即验证**：深夜 yawn↑ / look_around↓（对比白天） |
| A6 | emotionBridge 连续表情 | ✅ `App.tsx:56` toEmotionVector + `:934` 传 Live2DCanvas | **可立即验证**：戳→嘴角下垂；开心→微笑笑眼；久运行→半眯 |
| A7 | ~~多气泡堆叠~~ | ❌ **未实现** | 降级为 ③ follow-up（见下） |

> **A7 勘误**：原 backlog 把"多气泡堆叠"列为待验收，核验发现 `App.tsx:75-77` 气泡是单气泡状态（`bubbleText/Visible/Style/Pos` 均单一 useState）、`showBubble`(:159) 是覆盖语义（新气泡直接覆盖旧的 + 重置 timer），从未实现堆叠。降级为 follow-up；若用户确认需要"堆叠/排队"再开。

### ② 待开发（按优先级）

**Tier 2 — Soul/对话深度（懂你 · 闭环增强）**
- ~~**B1. Consolidation 反向更新 Facts**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：`consolidate` 成功后调 `backfill_facts`（LLM 从摘要提取 JSON 事实 → category 白名单+confidence clamp → `expire_old` 冲突过期 + `dedup_insert`）。失败隔离（只 warn）。+8 单测 + 新 `consolidation_harness`（真实 LLM 端到端）。详见 §最近一轮 (2026-08-03)。
- **B1b. Grounding 运行时阻断（B 档 · ⏳ 条件触发）**：A 档（prompt 收紧）实跑若仍偶发主动开口幻觉则升级——`check_groundedness` 补中文 claim 模式（现全英文、中文漏检）+ 在 proactive/welcome_back 输出端挂检测、发现编造就丢弃/降级。根因+修复详见 §最近一轮 (07-31 19:10)。

**Tier 3 — Body 完善（活着 · 生命感）**
- ~~**B2. 完整物理**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：自由落体 + 任务栏弹跳（P12.1）。新 `gravity.ts`（GRAVITY/BOUNCE 常量 + `stepGravity` 纯函数）。**关键**：发现 `startDragging` 吞 webview 鼠标事件（旧 `onUp` 死代码）→ 改 `onMoved`+静止检测；petPos useState→ref 重构修卡顿。用户偏好"1/3 飘落悬停"（不真触底，bounce 当前是死代码，待确认）。详见 §最近一轮 (2026-08-03)。
- **B3. Sleeping 配套收尾**（小项）：① 睡着抑制 DeepNight nudge（现睡着仍冒"早点睡"，像梦话）② 接 sleep 音效素材（已预留未接）③ LateNight(22-2) 不入睡只 yawn。

**Tier 4 — 开发者基建（#11 Explainability）**
- **B4. P16 Debug Panel 补全**：Prompt token 预算 / Retrieved score breakdown / Reflect(has_thought/unsurfaced) / AnimFSM / Cost 分区。`DebugPanel.tsx` 现 6 个 section 全无这些。
- **B5. P17 Golden 评估框架**：人格漂移 score + CI 自动跑 + `tests/evaluation.rs`。现仅 `golden_conversations.rs` 数据，无评估框架/CI。
- **B4b. conversations 死表修复（#11 可追溯）**：生产路径调用 `conversations::insert` 写对话日志（现仅测试调用、表 0 行）。独立 bug——本次幻觉排查中因它无法回溯她原话。

**Tier 5 — 架构债务（重构 · 功能已在跑）**
- **B6. A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表（架构债）。
- **B7. A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14）**
- **B8.** Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

### ③ 散落 follow-up（低优先 · 可并入相关 Tier）
Alt+Space 全局键（P11.4）/ 走路脚步声 loop（P11.5）/ 害羞慢现气泡形态（缺后端 mood 标签）/ rest_need 后端暴露（P10）/ speedModifier·energyModifier 接动画速度（circadian）/ idle_weights JSON 化（数据驱动）/ 选择性遗忘（用户请求"忘掉..."，P13 lifecycle_cleanup）。

> **建议下一会话起点**：先清 ① 待验收（A1-A7 逐项 rebuild+实跑，零新代码、闭环既有成果），再按 B1→B8 推进。实跑前提：`%APPDATA%\DesktopPet\config.toml` 配好 DeepSeek key + 桌面快捷方式（或 `npm run tauri dev`）。
