# Liri Spine — 动画规范（animation_spec）

> 由 `liri.json` 实际解析生成。共 **10 个动画**，命名已是分层式（美术已完成，勿改名）。
> 配套代码：`SpineCanvas.tsx` + `spineIntent.ts`（FSM/情绪意图 → Spine 轨道）。

## 关键约定：分层轨道（Track）

**不是"一次只播一个动画"**。Spine `AnimationState` 支持多 track 同时播放，高 track 覆盖低 track 它触及的属性。Liri 的正确播放方式是分层叠加：

| Track | 内容 | 循环 | 角色 |
|---|---|---|---|
| 0 | `body_breath` | ✅ loop | 基础呼吸（身体主轴微动） |
| 1 | `ear_idle` + `hair_idle` + `tail_idle` + `arm_idle` | ✅ loop | 辅助生命感（耳/发/尾/臂自循环） |
| 2 | `blink` / `wink_L` / `wink_R` / `smile` | ❌ 一次 | 表情层（触发式，播完自停） |
| 3 | `tail_happy`（事件层示例） | ❌ 一次 | 事件覆盖（未来 wave/sleep 等） |

> ⚠️ 常见误区：播 `smile` 时**不要**停 idle。正确是 `body_breath + hair_idle + tail_idle + smile` 同时存在。
> ⚠️ 表情 slot 的归属：几乎所有动画都 key 了 `右闭眼/半睁眼*/左闭眼/张大笑嘴`（把它们设为隐藏），只有 `smile`/`blink`/`wink`/`yawn类` 才让对应表情 slot 显示。分层播放时高 track 的 slot key 胜出——所以表情动画要放在比 idle **更高**的 track。

> 注意：Spine 的 loop 是**运行时属性**（`setAnimation(track, name, loop)` 的第 3 参），不存于 JSON。下表 loop 列是**代码应如何设置**。

## 动画清单

| 动画 | 时间轴 | loop（代码设置） | 用途 |
|---|---|---|---|
| `body_breath` | slots + bones + deform | ✅ | 基础呼吸：spine/spine2/spine3/head/ribbon/bangs 微动 |
| `arm_idle` | slots + bones + deform | ✅ | 手臂自循环：forearm_L + 袖 + spine 链 |
| `ear_idle` | slots + bones + deform | ✅ | 耳朵自然动：ear_l2/ear_r2 + tail_1 抖 |
| `hair_idle` | slots + bones + deform | ✅ | 头发飘动：所有 hair 链 + 耳 + 尾 摆 |
| `tail_idle` | slots + bones | ✅ | 尾巴自然摆：tail_1..5 链 |
| `tail_happy` | slots + bones | ❌ 一次 | 尾巴开心摆（幅度更大） |
| `blink` | slots | ❌ 一次 | 眨眼：切 `右闭眼/左闭眼/半睁眼` |
| `wink_L` | slots | ❌ 一次 | 左眼眨（单眼） |
| `wink_R` | slots | ❌ 一次 | 右眼眨（单眼） |
| `smile` | slots + deform | ❌ 一次 | 笑：显 `笑眯眼/半笑眼`，**不动骨骼**（纯表情叠加） |

## Mix（过渡）时间（代码里设，AnimationStateData.setMix）

不同层用不同过渡，避免"啪"地切换：
- 表情动画（blink/wink/smile）：`0.10~0.15s`
- 身体/尾巴动作（tail_happy）：`0.30~0.40s`
- idle 层切换：`0.20s`

## 与 FSM BehaviorState 的映射（MVP 子集）

FSM 有 14 个 behavior，Spine 只有 10 个动画，**非 1:1**。MVP 先接有对应动画的，其余 fallback 到 idle：

| BehaviorState | Spine 动作 |
|---|---|
| `Idle` / `Recovering` | 只留 track0/1 idle（不干预） |
| `Blink` | track2 播 `blink`（FSM 已随机触发） |
| `Talking` | track2 周期性 `blink` + 嘴型（lip-sync 占用嘴 slot，待接） |
| `Thinking` | track2 `blink` + 可选 `smile`（轻） |
| `Sleeping` | track2 切半睁/闭眼 slot（疲惫眼）+ 停 tail_idle |
| `Embarrassed` | track2 `wink_L`/`wink_R`（暂代，无专用动画） |
| 其余（LookAround/TiltHead/Sway/Stretch/Peek/Hum/Yawn） | **MVP fallback 到 idle**；后续可加骨骼程序化驱动（旋转 head/neck）或美术补动画 |

## 与情绪向量的映射（emotion → 表情 slot）

`EmotionVector`（mood/physical_energy/rest_need/stress…）→ 表情：
- `rest_need` 高 / `physical_energy` 低 → 显 `半睁眼左/右`（疲惫半眯）
- `mood` 高（>0.55）→ 显 `笑眯眼` + `小笑嘴`（轻笑常驻）
- `stress` 高 / `mood` 低 → 眉毛下垂（`左/右眉毛` slot 需程序化位移或补动画）
- 瞬时表情（backend `transient_expression` f00/f04）→ track2 触发 `smile` 一次

> Live2D 时代的 `emotionDriver`/`behaviorDriver` 写的是 Cubism 参数 ID（`ParamEyeLOpen` 等），Spine 用不上。**意图（EmotionVector / BehaviorState）复用，参数翻译层重写为 slot/track 操作。**
