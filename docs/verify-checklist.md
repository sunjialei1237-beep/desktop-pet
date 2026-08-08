# 验收清单 — 不改系统时间的运行时验收

> 配合 `src/App.tsx` 的 **dev-only** `window.__pet` 钩子。**release exe 不含此钩子**（`import.meta.env.DEV` 在 prod build 被 Vite 死代码消除，已 grep `dist/` 验证 0 命中）。所有"需改系统时间 / 需等 10min"的验收都用它。
> 原理：在 webview 进程内重写 `Date.prototype.getHours`，让桌宠"以为"现在是某时段——`circadian.ts` 是 `getHours` 唯一调用点，`Date.now()` 不受影响（入睡/冷却计时仍走真实时钟）。**对真实系统时钟零影响，无 UAC。**

## 前提
1. `npm run tauri dev`（**必须是 dev**，钩子 DEV-only；release 无）。
2. **右键桌宠 → DevTools** 菜单项（dev 专用，调用 `open_devtools`）→ 打开浏览器 DevTools → Console。
   > 注意：**F12 是应用内的 Debug Panel**（看 BrainState 的那个 React 面板），**不是**浏览器 DevTools。要跑 `window.__pet` 必须用右键菜单的 DevTools。
3. Console 应打印 `[dev] window.__pet ready: ...`（确认钩子挂载）。

## `window.__pet` API

| 方法 | 作用 |
|---|---|
| `__pet.setHour(h)` | 模拟时段。`h=3` → DeepNight(2-6) sleepiness 0.9；`h=10` → Morning sleepiness 0.1；`h=23` → LateNight。下一帧(~16ms)生效。 |
| `__pet.resetHour()` | 恢复真实时钟。**验收完务必调一次。** |
| `__pet.forceIdle(mins)` | 把"上次交互时间"倒拨 N 分钟，让 DeepNight auto-sleep 条件在下个 2.5s tick 立即满足（**走真实代码路径**，含 sleep 音），跳过 10min 等待。 |
| `__pet.sleep()` | 直接入睡（`forceState(Sleeping)` + sleep 音）——跳过所有前置条件，专验入睡渲染+音效。 |
| `__pet.wake()` | 直接唤醒（= `markInteraction`，刷新 idle 计时 + forceState Idle）。 |
| `__pet.probeNudge()` | 立即跑一次 nudge 检查（跳过 10min interval）。睡着时 no-op（验 B3① 抑制）。 |
| `__pet.state()` | 读 `{behavior, period, sleepiness, idleSecs}`——自动化断言用。 |

## 一键自检（机制层，复制到 Console）

跑完会打印 A5/A4 的 PASS/FAIL（时段时间映射 + 入睡/唤醒状态切换）。**感知项**（yawn 频率、sleep 音、表情）仍需肉眼/耳确认——脚本结尾会列出。

```js
(async () => {
  const p = window.__pet, sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const ok = (c, m, ...a) => console.log(`[verify] ${c ? "✅ PASS" : "❌ FAIL"} ${m}`, ...a);
  if (!p) { console.error("[verify] window.__pet 未就绪——确认是 dev 模式且钩子已挂"); return; }

  p.setHour(3);   await sleep(60);
  let s = p.state();
  ok(s.period === "deep_night" && s.sleepiness === 0.9, "A5 setHour(3)→DeepNight", s);

  p.setHour(10);  await sleep(60);
  s = p.state();
  ok(s.period === "morning" && s.sleepiness === 0.1, "A5 setHour(10)→Morning", s);

  p.setHour(3);
  p.sleep();      await sleep(120);   // 直接入睡（渲染 + sleep 音）
  s = p.state();
  ok(s.behavior === "sleeping", "A4 入睡 sleep()", s.behavior);

  p.probeNudge();                      // 睡着 → 应无气泡
  console.log("[verify] B3① probeNudge：睡着时屏幕应【无】新气泡（再按几次 __pet.probeNudge() 确认）");

  p.wake();       await sleep(120);
  s = p.state();
  ok(s.behavior !== "sleeping", "A4 唤醒 wake()", s.behavior);

  p.resetHour();
  console.log("[verify] ✅ 机制层完成，时钟已还原。感知项请肉眼/耳确认：");
  console.log("  ① A5 yawn 频率：__pet.setHour(3) 多 / setHour(10) 少");
  console.log("  ② B3② sleep 音：__pet.sleep() 入睡瞬间有声（右键静音后无声）");
  console.log("  ③ A6 表情连续：戳身→嘴角下垂；开心话题→微笑；久运行→半眯");
})();
```

## 逐项验收

### A5 — circadian sleepiness 调权重（深夜 yawn↑）
```
__pet.setHour(3)            // DeepNight, sleepiness 0.9
__pet.state()               // 确认 {period:"deep_night", sleepiness:0.9}
// 观察 ~1 分钟：yawn（打哈欠）频率明显高于白天
__pet.setHour(10)           // Morning, sleepiness 0.1
// 再观察：yawn 明显变少、look_around 变多
```
**通过判据**：肉眼对比两个时段 yawn 频率，深夜明显更多（HANDOFF 数学验证：0.1→10.7% yawn，0.9→32.2% yawn，约 3×）。

### A4 — Sleeping 入睡（走真实 auto-sleep 路径）
```
__pet.setHour(3)
__pet.forceIdle(15)         // 倒拨 15min，越过 10min 门槛
// 等约 2.5s（一个 fsm tick）→ 她自动入睡
__pet.state()               // 确认 {behavior:"sleeping", period:"deep_night"}
```
**通过判据**：闭眼 + 慢呼吸 + 头顶 Zz；`state().behavior === "sleeping"`。
**快速版**（跳过条件，直验渲染）：`__pet.setHour(3); __pet.sleep()`。

### B3② — sleep 音效
随 A4 入睡（任一方式）瞬间：应听到 sleep 音（`public/audio/voice/sleep.mp3`，轻入睡声）。
**通过判据**：入睡那一刻有声；静音（右键菜单静音）后再 `__pet.sleep()` 无声（#6）。

### A4 — 唤醒
入睡后任选其一：
- 手动：戳/摸/拖/双击/发消息 → 即时醒；
- 钩子：`__pet.wake()` → 醒。
**通过判据**：恢复 Idle 表情；`state().behavior !== "sleeping"`；唤醒**不响** sleep 音（醒是安静态）。

### B3① — 睡着抑制 nudge（不梦话）
```
__pet.setHour(3)
__pet.sleep()
__pet.probeNudge()          // 睡着 → 不冒泡（抑制生效）
__pet.probeNudge()          // 多确认几次
__pet.wake()                // 醒来
__pet.probeNudge()          // 醒着 + DeepNight → 偶冒「早点睡」（0.4 概率，多按几次）
```
**通过判据**：睡着时 `probeNudge` 反复调用**绝不冒泡**；醒后在 DeepNight 反复调用会偶发冒「这么晚了还不睡呀…/别熬夜了…/早点睡吧…」（证明抑制是"睡着"挡的，不是 nudge 本身坏了）。

### A6 — emotionBridge 连续表情（不需钩子，可直接验）
无需改时间：
- **戳身体** → stress↑ → 嘴角下垂（连续参数，非离散跳变）；
- 聊个开心话题 → mood↑ → 微笑 + 笑眼；
- 让她久运行 → energy↓ → 半眯眼。
**通过判据**：表情是**连续流动**的（不是 f00-f05 硬跳），嘴角/眼形随情绪渐变。

### 收尾（必做）
```
__pet.resetHour()           // 恢复真实时钟，避免后续行为被假时段影响
```
HMR / 重启 webview 会自动恢复（effect cleanup 也 restore），但仍建议显式调。

## 不易快速验收的项（代码层已单测，靠日常攒数据）
| 项 | 为何难快速触发 | 代码层依据 |
|---|---|---|
| A1 consolidation max_tokens | 需 episodes ≥ 100 自然攒 | `consolidation.rs:89` `Some(4096)` + `:97-103` 空 content 防御，`cargo test --lib` ✅ |
| A2 Reflection TurnThreshold/MajorEvent | 需攒 30 条对话记忆 或 importance>0.85 事件 | 优先级 Daily→MajorEvent→TurnThreshold + 12 单测 ✅ |
| A3 converse 注入 surfaced thought | 需 reflection 先产 thought（一日以上） | `converse.rs:202-221` 注入 + 消费性，surface_thoughts 单测 ✅ |

## 本批次新增验收（2026-08-07 自主批次：Memory / Soul / Loneliness）

> 本节验收**两个新观察/操作入口**：① **F12 = 应用内 Debug Panel**（React 面板，看 BrainState + 本批次新增的「记忆编辑器」）；② **右键 → DevTools** 的 `window.__pet`（见上文，控时段/入睡）。两者都是 **dev-only**。
> 关键：本批次的 **Emotion 编辑器**（Debug Panel 内 5 滑块 + Apply）让原本"需等几小时"的情绪驱动行为（loneliness 主动找你 / rest_need 疲惫眼）可**秒级触发**——这是 D2/D3/D4/D7 的主工具。

### D1 — Debug Panel 记忆可视化编辑（Task #11）
前提：`npm run tauri dev` → **F12** 打开 Debug Panel。
- **Facts** 行末 ✕ → confirm → 该 fact 消失（`expire_by_id` 软删，`get_active` 不再返回）。之后再正常聊出该偏好 → 重新习得（revive 路径）。
- **Episodes** 行末 ✕ → confirm → 删除（同步删向量，检索不再命中）。**地标记忆不可删**（按钮点了行不消失 = 守卫生效，非 bug）。
- **Pending** 行末 ✕（仅 `pending` 状态显示）→ 取消该提醒。
- **Emotion** 5 滑块拖动 → Apply → Brain 行数值即时变 + **桌宠表情即时变**。
**通过判据**：每个 ✕ 操作后 ≤2s（轮询周期）行消失；Emotion Apply 后桌宠脸马上变（如 mood=0.1 → 难过脸；stress=0.9 → 嘴角下垂）。

### D2 — loneliness 主动找你（核心新功能）
前提：**closeness ≥ 20**（关系够熟；Debug Panel Brain 行看 closeness。若 <20，多聊几句 / 摸头攒到 ≥20）。
```
// Debug Panel Emotion 编辑器：loneliness 滑块拉到 0.80 → Apply
// 然后 Idle（不聊天）等约 2 分钟（越过 recent_interaction<120s 门槛）
// 再等 ≤30s（后端 medium tick 检查）
```
**通过判据**：醒着 + loneliness>0.6 + closeness≥20 + 2min 没聊 → 冒一句 LLM 生成的"想你"类气泡（`lonely_bubble`，1 句、不催回复不逼问）。30min 冷却内不重复冒。
**反向**：closeness<20 时即使 loneliness 高也**不**冒（早期关系不黏人，镜像 planner Rule 4）。

### D3 — loneliness 睡着抑制（Task #12①）
```
__pet.setHour(3); __pet.sleep()        // 入睡
// Debug Panel 把 loneliness 拉到 0.80 → Apply（睡着也能编辑后端 emotion）
// 等 ~60s（越过 2 个 medium tick）
```
**通过判据**：Sleeping 时即使 loneliness 高也**绝不**冒"想你"气泡（`fsmRef.state===Sleeping` 守卫挡住）；`__pet.wake()` 醒后才可能冒。
> 注：lonely-nudge 由后端 30s `medium_tick` 触发，**不是** `probeNudge`（probeNudge 只测"该睡了"nudge）。验 D3 靠"拉高 loneliness + sleep + 等 30s 看不冒泡"。

### D4 — 摸头降 loneliness（Task #12②）
```
// Debug Panel 记下 Brain 行 Lonely 值（或 Emotion 编辑器读）
// 摸头（pet_head）一次
// 等 ≤2s 刷新 → Lonely 值降 ~0.10
```
**通过判据**：每次摸头 loneliness 降约 0.10（clamp 在 0）；poke（戳）**不**降（逗弄非安慰）。

### D5 — Forget 流程（忘掉记忆）
前提：先聊出一条可遗忘的记忆（如"我下周要吃火锅"）。
发：「忘掉我之前说的火锅那件事」
**通过判据**：她简短确认忘了（如"好，我忘了"/"嗯，已经不记得了"）且**绝不复述**该内容（复述=遗忘失败、惊悚）；Debug Panel 该 fact 标记过期 / episode 删除。

### D6 — QA 直答（Question route）
发纯知识/技术问题：「Python 的 GIL 是什么？」
**通过判据**：她**直接答问题**，不扯桌宠记忆、不硬关联私事；Debug Panel **Last Turn** 区 `Route: Question`、`Retrieved` 为空（QA 模式跳过 episode/fact 检索，只保留 identity）。

### D7 — rest_need → 疲惫眼（用编辑器秒级触发）
```
// Debug Panel Emotion：physical_energy 滑块拉到 0.10 → Apply
// 等几分钟（rest_need 经 homeostasis 按 LOW_ENERGY_THRESHOLD 增长）
```
**通过判据**：低 energy → rest_need 增长 → 半眯/疲惫眼（连续渐变，非离散跳变）。energy 拉回 0.7 → rest_need 衰减 → 眼睛重新睁开。

### D8 — 深度专注抑制主动气泡（P14.3 is_deep_focus 接线，2026-08-08）
```
// 1. dev 模式打开一个 Work 类前台 app（如 VSCode/终端）连续 ≥25 min 不切换
// 2. F12 Debug Panel → Focus 分区看「连续工作 N min」增长，到 25min 变「🔒 深度专注中」
// 3. 此时若有到期 pending / loneliness 高 → 不应冒主动气泡（trigger_proactive Rule1 抑制）
```
**通过判据**：Focus 分区计数 ≥25min 显示深度专注；专注期间主动气泡（welcome-back/lonely-nudge/proactive-prompt）被抑制；切到非 Work app（如浏览器/游戏）或锁屏离开 → 计数归零。`enable_window=false` 时恒不专注（#6 优雅退化）。代码层 `perception/focus.rs` 6 纯函数测已覆盖累积/重置/阈值；此项验"线程采样+抑制端到端"。
## 本批次不易快速验收（代码层已单测）
| 项 | 为何难快速触发 | 代码层依据 |
|---|---|---|
| 关系 review（relationship_reviews） | episode-gated，需攒够 N 条新对话 episode 才触发 | `soul/review.rs` + `maybe_run_review_if_due` + 多单测 ✅；攒够后 Debug Panel Timeline 可见 review 记录 |
| converse 空回复重试（#8） | 瞬态：flash reasoning 偶发 finish_reason=length 空 content | 日志 `[converse] main reply empty on first attempt; retrying once` 为观察点；`converse.rs` 重试块 + extractor 同款模式 |
| surfaced thought 注入 | 需 reflection 先产 thought（一日以上） | `converse.rs` 注入 + 消费性 + `surface_thoughts` 单测 ✅（与既有 A3 同） |
| B6 BrainState（#9）/ 死代码清理（#13） | 纯重构 / 删除，行为不变 | lib 255 + `check --tests` ✅，无需手感验 |

## 安全说明
- `setHour` 只重写 `getHours`，`Date.now()`/`new Date()` 真实值不受影响 → DeepNight 入睡计时（`Date.now() - lastInteraction`）、cooldown 仍走真实时钟。
- 钩子是 `import.meta.env.DEV` 守卫，prod build 0 命中（已验证）。
- `resetHour` 用"保存原生→赋值恢复"（非 `delete`，因 native 属性 strict 模式下 non-configurable 不可删）；effect cleanup 兜底 restore，防 StrictMode/HMR 泄漏假时段。
