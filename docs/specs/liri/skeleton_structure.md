# Liri Spine — 骨骼与插槽结构（skeleton_structure）

> 由 `liri.json`（spine 3.8.75）实际解析生成，非凭空推测。
> 源资产：`public/spine/liri/{liri.json, liri.atlas, liri.png}`
> 给集成代码（agent/人）的契约：不要自己猜骨骼名/slot 名，以本文件为准。

## Skeleton 元信息

| 字段 | 值 |
|---|---|
| spine 版本 | **3.8.75**（运行时必须用 Spine Runtime 3.8，即 `pixi-spine@4` 的 `@pixi-spine/runtime-3.8`） |
| 画布尺寸 | 573.35 × 1280.06（setup pose 坐标系） |
| 原点偏移 | x=-215.24, y=1.19 |
| Skin | 名为 `"0"`（单一 skin，attachments 挂在 skin "0" 下） |
| Events | 无（动画全由前端 FSM/状态驱动，不靠 Spine 事件） |

## 骨骼层级（name <- parent）

身体主轴：
```
root
 └ pelvis
    ├ spine → spine2 → spine3 → neck → head
    ├ thigh_L → calf_L → foot_L          （左腿）
    ├ thigh_R → calf_R → foot_R          （右腿）
    ├ skirt_L_1 → skirt_L_2              （左裙摆 2 节）
    ├ skirt_R_1 → skirt_R_2              （右裙摆 2 节）
    ├ ribbon_L_1 → ribbon_L_2            （左飘带）
    └ ribbon_R_1                          （右飘带）
```

上肢（挂 spine 上，受呼吸带动）：
```
spine
 ├ shoulder_L → upper_arm_L → forearm_L → hand_L
 │                                  └ sleeve_L_1 → sleeve_L_2   （左袖 2 节）
 └ shoulder_R → upper_arm_R → forearm_R → hand_R
                                    └ sleeve_R_1 → sleeve_R_2   （右袖 2 节）
```

头部附属（挂 head）：
```
head
 ├ ear_l1 → ear_l2          （左耳 2 节）
 ├ ear_r1 → ear_r2          （右耳 2 节）
 ├ hair_L_1 → hair_L_2 → hair_L_3     （左侧发 3 节）
 ├ hair_R_1 → hair_R_2 → hair_R_3     （右侧发 3 节）
 ├ hair_back_root
 │   ├ hb_mid_1 → hb_mid_2 → hb_mid_3     （后发中束 3 节）
 │   ├ hb_left_1 → hb_left_2 → hb_left_3  （后发左束 3 节）
 │   └ hb_right_1 → hb_right_2 → hb_right_3 （后发右束 3 节）
 └ liuhai1 → liuhai2 → lh3 → lh4         （刘海 4 节）
```

尾巴（挂 root，**6 节链**，物理感核心）：
```
root
 └ tail_root → tail_1 → tail_2 → tail_3 → tail_4 → tail_5
```

> 视线驱动：可旋转 `neck`/`head` 骨骼朝指针方向（Spine 无 Live2D 的 focusController，需自己写 bone rotation）。

## Slots（39 个）

> slot 名都是中文。表情变体是**独立 slot**，靠动画的 `slots` 时间轴切换 attachment/可见性。

身体/服装 slot：`尾巴` `后头发` `右腿` `左腿` `裙3` `裙2` `裙1` `衣服主体` `右飘带` `左飘带` `右袖` `左袖` `右手` `左手` `右耳` `左耳` `脸` `右头发` `左头发` `刘海` `头饰`

眼部 slot（默认显示）：`右眼` `右眼高光` `左眼` `左眼高光` `右眉毛` `左眉毛`

**表情变体 slot（默认隐藏，default attachment = `-`/null）**：
| slot | 用途 |
|---|---|
| `左闭眼` `右闭眼` | 全闭眼（blink/睡） |
| `半睁眼左` `半睁眼右` | 半睁（疲惫/困） |
| `左笑眯眼` `右笑眯眼` | 笑眼（^_^） |
| `左半笑眼` `右半笑眼` | 半笑眼 |
| `嘴` `小笑嘴` `半张笑嘴` `张大笑嘴` | 嘴型：默认/小笑/半张/大笑 |

> **表情 = 切换这些 slot 的 attachment 可见性**，不是换整张脸图。
> 代码里 `setExpression` 的实现路径：要么直接 `slot.attachment = ...`，要么播放 `smile`/`blink`/`wink` 动画（它们内部就是切换这些 slot）。MVP 走动画播放更稳。

## draw order

slot 在 `slots` 数组里的顺序即默认 draw order（从后到前）：
`尾巴 → 后头发 → 腿 → 裙 → 衣服 → 飘带 → 袖 → 手 → 耳 → 脸 → 眼/高光/嘴 → 头发 → 刘海 → 眉毛 → 头饰 → 表情变体 slot`。
表情变体 slot 排在最后（最上层），所以切换显示时会盖在基础眼/嘴上——正确。
