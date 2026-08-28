# Liri Spine — 动画规范（animation_spec）

> 由 v2 `liri.json`（2026-08-27 新导出，`Liri_Project - 副本`）实际解析生成。共 **17 个动画**。
> 配套代码：`SpineCanvas.tsx` + `spineIntent.ts`（组合程序模型）+ `liriAssetPatch.ts`（数据级补丁）。
> **播放架构为「常驻循环底座 + 通道变奏 + 呼吸边界对齐的组合程序」**，2026-08-28 与制作人确认后实施。

## 关键约定

1. **骨骼域互斥（补丁 C 剥钉保证）**：所有次要动画在 t=0 的跨域钉扎（head=0.57 / spine=0.54 /
   spine3=-0.03 / lh3=-4.47 / liuhai2=3.95 / ear_r2=-12.54 等，0 值钉以无 angle 字段出现）
   由 `liriAssetPatch.stripCrossDomainPins` 在加载期剥除，使**每个并发轨道拥有互不相交的骨骼集**：
   - `body_breath`：脊柱链+head 摆+双飘带（其 lh3/lh4/liuhai2 纯钉被剥，刘海归 hair）
   - `hair_idle`：侧发/后发束+刘海（其 ear/tail_1/spine 系钉被剥）
   - `ear_*`：仅 ear_l2/ear_r2（tail_1 钉剥除）；`tail_*`：仅 tail_1..5；`arm_idle`/`Skirt_l`：各自部件
   - **例外**：`thing`（手势）保留全部真实关键帧，在最高身体轨短暂接管，结束用空轨 mix 收势
2. **安静常态（2026-08-28 用户裁定）**：底座常驻 = **呼吸左右摆 + 裙摆/手臂微动氛围**。
   耳/发/尾为**间隔 ≥15s（15–25s 随机）的单部位一次性动作**（ear_idle/hair_idle/tail_idle，
   播完空轨淡出）——不再常驻循环。开心/难过/好奇等组合程序为"特殊情况"，只由事件触发
   （`requestProgram(id)` 待接情绪桥），idle 随机器不碰它们。
3. **呼吸边界对齐（组合程序）**：程序只在 body_breath `complete`（每 4.3333s）时启动；
   收束同样发生在边界上——所有成员通道**并行**淡出回空轨。
   即制作人规则「所有动画动作在一个完整的呼吸动作开始时并行结束」。
4. **模型显示比例 0.5**（2026-08-28 用户：0.7 → 0.5，`SpineCanvas` fit 系数）。

## 轨道布局（SpineCanvas / spineIntent.TRACK，低→高）

| Track | 内容 | 循环 | 角色 |
|---|---|---|---|
| 0 | `body_breath` | ✅ | 基础呼吸（身体主轴摆动+飘带）**常驻** |
| 1 | `Skirt_l` | ✅ | 裙摆慢飘（常驻氛围） |
| 2 | `hair_idle` | 空→一次性 | 随机单部位动作（≥15s 间隔） |
| 3 | `arm_idle` | ✅ | 左臂微动（常驻氛围） |
| 4 | 耳通道 | 空→一次性/程序成员 | 随机动作 & 情绪程序成员 |
| 5 | 尾通道 | 空→一次性/程序成员 | 同上（恒高于耳轨，防 tail_1 互踩） |
| 6 | 手势：`thing`（+未来摸头/戳尾） | ❌ | 一次性，GESTURE_FADE=0.35s 空轨收势 |
| 7 | 表情队列：`blink`/`wink_L`/`wink_R`/`smile`/`eye_sad` | ❌ | 串行 countdown（`exprBusyRem`）防打断；程序激活期整体冻结 |

> mix：`defaultMix=0.15`；表情自切 `setMixByName(a,a,0.12)`。
> ⚠️ 教训（续⁶⁵）：调度器倒计时**不可**把 per-frame `elapsedMS` 当时钟存 "now+dur" 时间戳——
> 第一次眨眼后 `wall < exprBusyUntil` 恒真，整个调度器冻结（笑/眨眼/程序全停）。一律用 countdown。

## 动画清单（17，时长=JSON 实测）

| 动画 | 时长 | loop | 用途 |
|---|---|---|---|
| `body_breath` | 4.333s | ✅ | 呼吸：spine ±0.8°/spine2 ±5.6°/spine3 ∓3.9° 左右摆 + head 微抬 + 双飘带（**一个 beat**） |
| `Skirt_l` | 5.67s | ✅ | 裙摆三节超慢轻晃（≤8°）；命名不属于系列，按"环境风"处理 |
| `hair_idle` | 2.70s | ✅ | 侧发/后发束 9 链 + 刘海 lh3/lh4/liuhai2 摆动 |
| `arm_idle` | 1.13s | ✅ | forearm_L + 左袖两节 |
| `ear_idle` | 3.10s | ✅ | 双耳慢速自然动 |
| `ear_2` | 2.43s | 变奏 | 左耳四连抖(0.73s)→停→右耳抖(0.9s)，自回基线 |
| `ear_sad` | 5.33s | ❌×1 | 左耳抬压后(46.7°)、右耳垂(-58.4°)，缓收**回基线**；一次性后静默保持 |
| `tail_idle` | 1.20s | ✅ | 轻摆（尾尖叠加 ±7°） |
| `tail_2` | 1.90s | 变奏 | 中幅欢快摆（+6.4/−3.4°） |
| `tail_happy` | 1.37s | 变奏 | 大幅摇（尾尖预扬 25.1°→−12.7°） |
| `tail_sad` | 2.07s | 变奏 | 低垂慢摇，尾尖拖 −31.2° 回收 |
| `blink` | 0.10s | ❌ | 分阶段眨眼：半睁眼(0.03)→闭眼(0.07)→复原 |
| `wink_L`/`wink_R` | 0.10s | ❌ | 单眼眨（Embarrassed 代用） |
| `smile` | 3.933s | ❌ | 笑：半笑眼闪现→笑眯眼持续→3.5s 复原；大笑嘴 deform 0.4–3.33s（需补丁 B 显形） |
| `eye_sad` | 2.00s | ❌ | 显`难过眼`+`难过嘴`(0.03)→收(2.0)；**眼+嘴一体** |
| `thing` | 0.33s | ❌ | 双手捧起+头上仰 6.7°；**无回收键**，代码空轨 mix 兜底放下 |

## 组合程序（spineIntent.PROGRAMS；窗口 = beats × 4.333s）

| 程序 | beats | 成员（并行通道） | 核算 |
|---|---|---|---|
| `sad` 难过 | 2 | ear_sad ×1 静默回位 ＋ tail_sad 循环 ＋ eye_sad 重触发(每 2.05s，窗口尾 1s 内休止) | 5.33/8.28(max)/8.15 ≤ 8.667 ✓ |
| `happyLong` | 2 | tail_happy 循环 ＋ smile @beat0 ＋ smile @beat1 | ✓ |
| `happyShort` | 1 | tail_happy 循环 ＋ smile @beat0 | 4.11/3.93 ≤ 4.333 ✓ |
| `curious` | 1 | ear_2 ＋ tail_2 | ✓ |
| `thing` | 1 | 手势轨道 thing；收势空轨 mix | ✓ |

Idle 生活随机器（安静常态）：每 **15–25s** 一次性动作，均匀三选一：ear_idle / hair_idle /
tail_idle（`pickPartAction` + `nextPartDelay`）。眨眼 4–6s、微笑 12–18s 照旧独立走表情轨。
组合程序（sad/happyLong/happyShort/curious/thing）**只**由事件经 `requestProgram(id)` 触发
（情绪桥接线 follow-up），idle 不再轮盘它们。

## 数据级补丁（liriAssetPatch v2，运行时加载期，三族）

- **A** 嘴槽位 `嘴/小笑嘴/张大笑嘴` setup 裸露 → 隐藏（防 idle 常驻叠嘴）。
- **B** `smile` 有`张大笑嘴`/`嘴` deform 但无显示键 → 注入 `t=0 show / t=3.9333 null`。
- **C** 跨域钉扎剥除（见上"骨骼域互斥"），匹配规则：**全部**关键帧角值 ≈ 基准(±0.01) 且始于 t=0；
  多值真实动画与任何非基准常量不动。基准表见补丁源码注释（v2 精确值，勿"顺手取整"）。
- 美术侧修复后 A/B 可删；C 在美术停止录制 setup 钉扎后可删。单元测试钉死全部变换
  （含**真实资产**装载测试，重导出改变结构会在这里红）。

## 与 FSM BehaviorState 的映射（现状）

| BehaviorState | 动作 |
|---|---|
| `Embarrassed` | 表情轨 `wink_L/R`（程序运行时抑制） |
| `Idle`/其余 | 六循环底座 + idle 生活程序；`Blink` 态由生理定时器（4–6s）接管，非 FSM |
| `Talking`/`Thinking` | 待接：Thinking→`thing`、Talking→blink 节奏（程序入口已备好 `requestProgram`） |

## 已知边界（诚实记录）

- 所有次要动画带 `衣服主体` 网格微 deform：并发时高轨赢得该网格（近似形变，肉眼平滑）；
  若实跑出现布料接缝抖动，follow-up=剥除次要动画的该 deform 只留 breath 所有。
- 循环重触发 eye_sad 的接缝有 ~30–50ms 隐藏闪帧（离散 attachment 切换，视觉不可察）。
- `thing` 收势是代码兜底而非美术键；美术补"放臂回收键"后可去掉 GESTURE_FADE。
- 新导出 skin 名 `"0"`→`"default"`（pixi-spine 自动兼容，无需配置）。
