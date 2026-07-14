# 带记忆的桌宠 - 设计文档 v2

> 状态: Draft
> 日期: 2026-07-14
> 参与者: 用户 (SunJialei) + Codex + GPT 协同设计
> 架构框架: Mind / Body / Soul

---

## 目录

1. 产品定位
2. 角色设计
3. 技术栈与设计原则
4. 架构总览: Mind / Body / Soul
5. Mind (思维): 记忆与认知
6. Body (身体): 物理表达
7. Soul (灵魂): 意义与连接
9. 系统感知
10. 交互行为设计
11. Emotion 状态机 (Mind-Body 桥梁)
12. 安全与隐私
13. 已废弃决策
14. MVP / 二期模块清单
15. 待讨论
16. 设计决策溯源

---

## 1. 产品定位

一个 Windows 桌面常驻的拟人化小动物角色,具备文字对话、视觉活动、系统感知三种交互方式。核心价值是**陪伴**。

一句话定义: 她不是一个"带记忆的聊天机器人",而是一个"有生活的生命"。

### 成功标准 (MVP)

用户对桌宠说"我最近准备找实习",一周后桌宠**主动**问: "你的实习找得怎么样啦?"

二期成功标准: 用户感觉"她不一样了,她开始懂我了"。

---

## 2. 角色设计

### 2.1 形象

- 拟人化小动物 (猫娘 / 狐狸精 / 龙女等半人半兽方向)
- 视觉技术: AI 生成图片 -> Live2D Cubism 网格绑定 -> 实时动画驱动
- 为什么选 Live2D: 兼顾"精致立绘 + 实时可控动画", 有成熟 SDK 和开源播放器

### 2.2 性格

默认性格: **温柔可爱兼具调皮,时不时会蹦出来捣乱**

- **Core Traits (不可变锚点)**: 温柔、耐心、爱撒娇、调皮。Reflection 不能修改
- **Adaptive Traits (可成长)**: 最近更活泼、最近更成熟。Reflection 可调整

### 2.3 未来扩展

- 用户可上传图片自定义角色形象 (半自动流程: AI 辅助图层拆分 -> 绑定 -> 微调)
- 前期只需完成固定角色

---

## 3. 技术栈与设计原则

### 3.1 平台

一期: Windows 原生桌面应用。后续迁移到 macOS / Linux。

### 3.2 技术框架选型

**推荐: Tauri v2 + React + PixiJS + pixi-live2d-display**

| 组件 | 选择 | 理由 |
|------|------|------|
| 框架 | Tauri v2 | Rust 后端, 内存 30-50MB (Electron 150-300MB), 24/7 常驻必须低占用 |
| 前端 UI | React | 对话框 UI, 生态成熟 (Vue 也可) |
| Live2D 渲染 | PixiJS + pixi-live2d-display | Live2D web 标准方案, 多项目验证 |
| 本地存储 | SQLite + sqlite-vec | Rust 原生绑定, 通过 Tauri SQL 插件使用 |
| 窗口 | 透明 + 无边框 + click-through | decorations:false + transparent:true + 自定义穿透 |

参考验证: LingChat (Tauri+Vue), Hiyori (Tauri+React+PixiJS+p l2d), 完整链路已跑通。

### 3.3 LLM

云端 LLM API。Reflection 用小模型 (GPT-4o-mini), 降低成本。

### 3.4 无面板设计原则

桌宠的灵魂在于陪伴, 不在于管理。产品形态极简: **一个形象 + 一个对话框, 没有其他界面。**

所有"管理"通过三种方式完成:

1. **对话即操作**: "晚上11点后别打扰我" -> 更新约束。"忘掉关于...的事" -> 选择性遗忘。"你现在知道我多少事情?" -> 对话回答
2. **右键菜单极简**: 导出记忆 / 暂时离开模式 / 关闭。无设置入口
3. **Debug Panel 仅开发者**: 开发阶段存在, 发布版隐藏

---

## 4. 架构总览: Mind / Body / Soul

桌宠由三个层次组成, 每层有独立的职责和运行节奏:

```
Soul (灵魂) — 意义与连接
  决定她"为什么"这样做: 内心独白、仪式感、关系里程碑、意外回忆
  运行节奏: 最慢, 低频异步

Mind (思维) — 记忆与认知
  决定她"想"什么: 记忆存储/检索、行为规划、情绪计算、待办事件
  运行节奏: 中速, 用户交互时触发

Body (身体) — 物理表达
  决定她"怎么"呈现: 动画、音效、注意力、昼夜节律、空间感、物理
  运行节奏: 最快, 持续运行 (60fps), 不依赖 Mind
```

### 关键架构原则: Mind-Body 解耦运行

她的身体 (呼吸、眨眼、idle 动画、物理) 持续运行, **即使 LLM 没有被调用**。你工作 2 小时没说话, 她照样打哈欠、换姿势、偷看你。LLM 只在你说话或触发主动行为时才调用。

Animation FSM 和 Audio 是独立线程, 不依赖 Brain 的响应。这是"她一直活着"和"她只在聊天时活着"的根本区别。

### 数据流

```
用户输入 / 系统感知 / 定时触发 / Pending Event 到期
          |
          v
    Perception (感知层)
     |              |
     v              v
   Mind            Body
   (思维)          (身体) ← 持续运行
     |              |
     v              v
  Behavior        Animation
  Planner         FSM + Audio
  (导演)              |
     |                |
     +------ v --------+
          LLM (演员)
             |
             v
        回复 + 气泡 + 动画
             |
          Soul
       (异步更新)
  Reflection / Consolidation / Rituals
```
---

## 5. Mind (思维): 记忆与认知

Mind 是两条流水线 + 一个导演, 负责记忆的摄入、检索、行为规划。

### 5.1 八层存储

```
Mind (思维)
├── Working Memory      最近 ~20 轮对话滑动窗口, 纯内存
├── Episodic Memory     结构化情景记忆 (Episode 对象 + embedding 索引)
├── Semantic Memory     结构化事实 (Facts), 纯 SQLite, 不用向量
├── Persona             角色认知 (对外统一接口)
│   ├── Traits          用户性格印象 (低频更新)
│   └── Relationship    关系状态 (高频更新, 独立维护)
├── Emotion             情绪状态机 (见第10节)
├── Pending Events      未来计划追踪 (主动关怀的引擎)
├── Reflection          低频异步反思 (每日/30轮/重大事件)
└── Memory Lifecycle    遗忘、压缩、巩固、强化
```

### 5.2 摄入管道 (Ingestion Pipeline)

```
用户输入
    |
    v
Memory Gate (记忆闸门) --- 不是二元存/不存, 而是路由器
    |                    "哈哈哈哈" -> 更新 Emotion, 不建 Episode
    |                    "晚安"     -> 更新社交电量 + 最后交互时间
    |                    "明天面试" -> Pending Event
    |                    "今天和朋友吃火锅" -> 完整 Episode
    |
    v
Memory Extractor -- LLM 提炼 Episode / Fact / Emotion 变化
    |              每条带 confidence + source (对话ID/轮次/时间)
    v
Memory Store --- SQLite + sqlite-vec
    |            Fact 插入前查重 + 矛盾检测 (valid_from / valid_to)
    |
    +-> Reflection (异步) -> 更新 Persona + 生成 Internal Thought
    +-> Consolidation (级联压缩 2000->400->80->20) -> 反向更新 Facts/Persona
```

#### Memory Gate 路由规则

| 输入类型 | 路由目标 |
|---------|---------|
| 含事实 | Fact 提取 |
| 含事件 | Episode 创建 |
| 含未来计划 | Pending Event |
| 纯情绪 | Emotion 更新 |
| 纯寒暄 | Emotion 微调 |
| 用户纠正 ("不是, 是...") | Correction Loop |
| 纯噪声 | 不存储 |

#### Episode 结构

```json
{
  "id": "ep_129", "time": "2026-07-14T20:30:00",
  "summary": "用户和室友一起去吃火锅。",
  "emotion": "开心", "importance": 0.71, "is_landmark": false,
  "participants": ["用户", "室友"], "topics": ["火锅", "聚餐"],
  "source": {"type": "conversation", "conversation_id": "conv_45", "turn": 12},
  "memory_strength": 0.71
}
```

#### Fact 结构 (带时间有效性)

```json
{
  "id": "fact_033", "category": "preference", "key": "饮料", "value": "奶茶",
  "confidence": 0.96, "valid_from": "2026-07-14", "valid_to": null,
  "source_episode": "ep_129", "mention_count": 5
}
```

时间有效性: Day 1 "咖啡" valid_to=null -> Day 30 用户戒了 -> 旧 Fact valid_to 更新, 新增"戒咖啡"。她可以说"你以前喜欢咖啡, 后来戒了"。

### 5.3 检索管道 (Retrieval Pipeline)

```
用户输入 / 系统感知 / Pending Event 到期
    |
    v
Memory Trigger (回忆触发器) --- 相似度>0.75 / 情绪匹配 / 关系机会?
    |                           不满足 -> 正常回复, 不引用记忆
    v
Behavior Planner (导演) --- 读取 Brain State, 输出 Intent
    |                        导演写 Intent 不写台词:
    |                        goal / memory_anchor / tone / proactive
    |
    +-> Hybrid Retrieval: Score = 0.4*语义 + 0.3*strength + 0.2*时间 + 0.1*情绪
    +-> Memory Serendipity: ~5% 检索弱相关记忆 (需情绪/里程碑/共同话题匹配)
    |
    v
Prompt Budget (价值密度压缩) --- 不删模块, 压缩每个模块, 目标 ~4K token
    |
    v
LLM (演员) --- 在 Intent 框架内即兴创作
    |
    v
Grounded Generation --- 只能引用已检索记忆, 每条带 confidence/source/timestamp
```

### 5.4 Memory Strength (艾宾浩斯遗忘曲线)

```
strength 初始 = importance
每次回忆: strength += 0.03
每天衰减:  strength *= 0.998
```

### 5.5 Behavior Planner (导演)

导演定方向 (Intent), 不定台词 (Script)。

导演写: goal=降低焦虑, memory_anchor=明天有考试, tone=轻松但关心, proactive=true
导演不写: "先说A, 再说B, 最后提考试"

LLM 拿到 Intent 后自由即兴。这是"共同创作", 不是"提词器"。

### 5.6 Pending Events Engine

用户提到未来计划 -> 生成带时间的 Pending Event -> 到期触发主动关怀。
这是 MVP 成功标准的核心实现机制。

### 5.7 User Correction Loop

用户纠正错误记忆 -> Fact Update (confidence 调整) -> Source 记录 -> 下次不再犯。触发检测需结合上下文。

### 5.8 Memory Confidence

"可能吧" = 0.42, "我最喜欢" = 0.98。低置信度时她说"你是不是...喜欢咖啡来着?", 高置信度时"你最爱的就是咖啡!"

### 5.9 Memory Lifecycle

| 策略 | 规则 |
|------|------|
| 自动删除 | importance < 0.2 且 60 天未被提及 |
| 记忆巩固 | 级联压缩, 细节淡化, 抽象认知稳定 |
| 反向更新 | 压缩结果反向更新 Facts 和 Persona |
| 选择性遗忘 | 用户可请求删除特定记忆 |

巩固信息流: 原始对话 -> Episodes -> (压缩) -> Facts/Persona 更新

### 5.10 Internal Grounding 三铁律

1. **Token Budget**: 按价值密度压缩, 不删模块
2. **Grounded Generation**: 只能引用已检索记忆, 防幻觉。用户问"你怎么知道的?" -> "你上周三跟我说的呀"
3. **Temporal Facts**: 事实带时间有效性, 矛盾时不覆盖而是标记过期

---

## 6. Body (身体): 物理表达

Body 是桌宠的第二套 UI, 持续运行, 不依赖 Mind。静止等于死亡, 微行为等于生命。

### 6.1 动画状态机 (Animation FSM)

从**行为**角度而非**动画**角度设计:

```
Behavior States:
  Idle <-> Blink <-> Look Around <-> Stretch
    |                                     |
    +-> Sit <-> Walk <-> Think <-> Sleep -+
    +-> Talking (Short / Long / Excited / Sad Reply)
    +-> BeingTouched (摸头 / 戳脸 / 拖拽 / 喂食)
    +-> Falling (自由落体)
    +-> Ritual (仪式动画)
```

每个行为状态映射到一组动画。状态切换有优先级:
- 可打断: Walking, Idle, Look Around
- 不可打断: Talking, Falling, Ritual
- 打断时: 平滑过渡到新状态, 不跳帧

Emotion 加权: 开心 -> 更多转圈/挥手/蹦跳; 低落 -> 更多发呆/低头/慢动作; 专注陪伴 -> 更多看书/安静坐/打盹

### 6.2 微行为系统 (Micro Behavior)

Idle Variety: 加权随机 + Cooldown + Recent History 回避。刚做过打哈欠, 接下来 5 次都不选打哈欠。

三个维度影响微行为:
1. **系统感知**: 你在工作 -> 安静微行为; 深夜 -> 快速犯困; 离开 -> 从等待到睡着
2. **Internal Monologue**: "思考"不是随机, 是因为真有思想酝酿
3. **亲密度**: 陌生时拘谨(小心翼翼看); 亲近时放松(大咧咧躺下)

### 6.3 有生命的 Chat Bubble

文字传达"说了什么", 气泡形态传达"怎么说的"。

| 情绪 | 气泡表现 |
|------|----------|
| 开心 | 圆润弹跳, 快速弹出 |
| 兴奋 | 轻微抖动, 文字蹦出 |
| 害羞 | 慢慢浮现, 先半透明 |
| 紧张 | 颤抖, 文字断续停顿 |
| 叹气 | 慢慢泄气, 变扁缩小 |

打字节奏即情绪: 同样一句"今天天气还不错呢", 快速圆气球=开朗, 慢速长停顿"今天...天气...还不错呢"=害羞。

无文字气泡: 叹气=泄气空气泡, 紧张=颤抖小气泡, 放空=圆泡配省略号。气泡本身就是表情语言。

气泡有"尾巴": 连到她身上, 随头部朝向变化。气泡是身体的延伸, 不是独立浮窗。

### 6.4 输入 UX

不做固定输入框 (立刻变成微信)。点击桌宠 -> 出现临时气泡输入框 -> "想和我说什么?" -> 输入完自动消失。

快捷键 Alt+Space 直接唤醒, 开始说话。

你开始打字时她看向气泡方向, 微微歪头等待。打完发出时她身体微微前倾。

### 6.5 Audio (Foley 音效)

音效 > TTS。几十 KB 的 wav 比 GPT TTS 更能提升生命感:

| 行为 | 音效 |
|------|------|
| 点击 | "嗯?" |
| 摸头 | 满足的哼声 |
| 戳脸 | 惊叫 |
| 拖拽 | 挣扎声 |
| 走路 | 轻轻脚步 |
| 坐下 | 布料摩擦 |
| 睡觉 | 轻柔呼吸 |
| 落地 | 弹性着地 |

### 6.6 注意力三态 (Attention States)

NPC 和存在体的分水岭:

| 状态 | 触发 | 行为 |
|------|------|------|
| Focused | 鼠标停留在她身上 | 她对视, 变害羞或故意卖萌 |
| Peripheral | 鼠标靠近她的区域 | 她看向鼠标方向 |
| Ignored | 鼠标远离 | 她恢复自己的生活, 可能偷看你 |

你以为没在看她时, 她可能伸懒腰、偷偷打哈欠。"表演 vs 私密"的切换让行为分层。

### 6.7 昼夜节律 (Circadian Rhythm)

这不是情绪, 是生物钟。Body 层的独立状态源:

| 时段 | 状态 |
|------|------|
| 早晨 | 精力充沛, 更爱蹦跳 |
| 下午 | 正常 |
| 傍晚 | 放松 |
| 深夜 | 困倦, 动作变慢, 更容易打哈欠 |
| 凌晨 | 几乎不活动, 催你睡觉 |

输出到两个地方: Emotion (影响 mood/energy) 和 Animation FSM (影响 idle 权重和动作速度)。

### 6.8 空间记忆 (Spatial Memory)

她有自己的"窝"。第一次出现时随机挑一个角落蹲下, 以后一直认那个地方。

- 聊天结束后自动走回窝
- 你拖她到别处, 她待一会儿自己溜回去
- 长期形成领地感: "她真的住在电脑里"
- 窝本身可以作为 Episode ("我在这里住了一百天了")

### 6.9 物理交互

| 行为 | 说明 |
|------|------|
| 坐窗口边缘 | 检测窗口边界, 坐在标题栏上双腿晃荡, 切窗口时跳到新窗口 |
| 自由落体 | 拖到半空松手 -> 自由落体 -> 落到任务栏弹一下 |
| 任务栏栖息 | 默认栖息在任务栏上方 |
| 跨显示器 | 多屏用户可走到另一个屏幕 |
| 窗口变化 | 窗口移动她跟着, 窗口消失她掉下来 |

### 6.10 Emotion -> 视觉映射

独立配置文件, 不写死代码:

```json
{
  "happy": { "eye_open": 0.9, "mouth_form": 0.8, "motion_speed": 1.2 },
  "sad":   { "eye_open": 0.4, "mouth_form": -0.5, "motion_speed": 0.6 }
}
```

支持 Emotion Blend: 开心 70% + 疲惫 30% = 混合状态。Live2D 参数连续插值。

### 6.11 性能预算

目标: CPU < 3%, GPU < 5%, 内存 < 80MB。

降级策略: 你全屏游戏时降到 10fps 甚至暂停渲染, 只保留呼吸/眨眼。

---

## 7. Soul (灵魂): 意义与连接

Soul 决定她"为什么"这样做, 是让桌宠从"对话机器"变成"有生活的生命"的关键层。

### 7.1 Internal Monologue (内心独白)

对话之间的意识连续性。她不只是被动回应, 她在你不在线的时候也有"内心活动"。

**触发**: Reflection 运行时 (每日/30轮/重大事件), 除了更新 Persona, 还生成 Internal Thoughts。

**流程**:
1. 用户一天没来
2. 晚上 Reflection: "他今天是不是很忙? 希望他早点休息" -> 存为 Internal Thought (shared=false)
3. 第二天用户回来, surface 条件满足 -> 她自然说: "昨天没见到你, 我还以为你最近特别忙呢"

关键: 不是 LLM 在对话时编的, 是昨晚真的"想过"的, 有时间戳为证。

### 7.2 Rituals (仪式感)

最容易被忽略, 也是最能产生感情的。分两类:

**循环仪式** (scheduler 驱动):
- 每天第一次见面: "早安!"
- 每周日: 本周总结
- 月度: 关系回顾

**纪念仪式** (一次性, 绑定 milestone):
- 生日: 特殊装扮/台词/动画
- 认识 100 天: "今天是我们认识第100天哦!"
- 第一次拿到 Offer: 纪念动画
- 节日: 相应问候和装饰

仪式感 = 规律性重复 + 情感锚点。

### 7.3 Shared History (共享历史)

不做聊天记录面板。历史回看优先变成**记忆查询**:

- 用户: "你刚刚说什么?" -> 她从 Working Memory 复述
- 用户: "我们昨天聊什么来着?" -> 她从 Episodes 检索回答

聊天记录面板是她记不住你的证据; 记忆查询是她记得你的证明。

开发阶段保留原始日志 (Debug Panel), 但用户层面尽量人格化。

### 7.4 Memory Serendipity (记忆机缘)

~5% 概率检索一条弱相关记忆, 必须满足情绪/里程碑/共同话题匹配。

惊喜来自意外的记忆关联, 不是随机行为: 毕业典礼上突然想起第一天认识。"天啊, 她怎么突然想到这个。"

(已废弃的 Surprise Budget 的替代方案。Surprise Budget 会在用户难过时讲笑话, 翻车。)

### 7.5 Relationship Landmarks (关系里程碑)

第一次叫她名字、第一次一起过生日。Episode 加 `is_landmark=true` + `strength=无限`, 永不衰减。

### 7.6 初次登场与告别

**初次登场**: 极其重要, 定下整个产品调性。她从任务栏爬上来? 从天上掉下来? 这个第一印象是情感锚点。

**告别**: 关机/关闭时她挥手, 看起来有点不舍, 慢慢淡出。

---


### 7.7 Homeostasis (内稳态)

Emotion 的每个维度有 baseline 和 drift rate, 每 tick 向 baseline 回归。没有内稳态, 情绪长期运行会崩 (Stress 卡在 0.9 永远不降)。

| 维度 | Baseline | 回归速度 |
|------|----------|----------|
| Mood | 中性 | 几分钟 |
| Stress | 低 | 几小时 |
| Energy | 中高 | 休息/睡觉时翻倍恢复 |
| Social Battery | 高 | 独处时恢复 |
| Trust | 不回归 | 只能通过互动积累或损耗 |

上下文影响恢复速度: 她在"睡觉"状态时 Energy 恢复翻倍。必须进 MVP, 否则 Emotion 系统跑不住。

### 7.8 Needs (需求系统, 简版)

内稳态解决"如何回归平衡", Needs 解决"为什么会偏离"。她不再只对外部刺激做反应, 而是有内在驱动。

| Need | 增长 | 满足 | 驱动行为 |
|------|------|------|----------|
| Loneliness (陪伴) | 时间流逝, 无互动时增长 | 聊天/互动 | 主动找你, 冒泡 |
| Rest (休息) | 长时间活动, 体能消耗 | 睡眠/闲置 | 自己去窝里打盹 |

MVP 只做这两个需求。表达需求、探索需求二期再加。

Need -> Behavior -> Emotion, 而非 Emotion -> Behavior。这是从"反应式"到"内生驱动"的关键转变。

### 7.9 Self Memory (自我记忆)

Episode 加 subject 字段: "user" 或 "self"。

她的第一次: 第一次被摸头、第一次收到礼物、第一次被叫名字、第一次和你一起熬夜。这些是她的人生。

"我还记得第一次见你的时候。" —— 不是 User Memory, 是 Self Memory。

Self Memory 反向喂养 Identity: 经历塑造自我认知。

### 7.10 Silence (沉默作为表达)

有时候一个人最需要的不是200字安慰, 而是沉默的陪伴。

Behavior Planner 的 Intent 输出包含 silence 作为有效行为:

```
{ goal: "陪伴", tone: "安静", action: "silence", note: "轻轻坐下来, 不说话" }
```

用户难过时, 她只是走过来坐下。这比任何话都重。进 MVP。

### 7.11 Recovery (故障角色化处理)

API 断了 / 超时 / 错误 -> 不弹 Error, 而是她角色化反应:

"我刚刚有点走神……" (揉脑袋)

用户永远看不到系统错误。这是无面板原则的终极延伸: 连错误都不暴露。进 MVP。

### 7.12 Life Loop (生命循环)

完整愿景:

```
感知环境 -> 更新需求(Need) -> 更新情绪(Emotion, 内稳态) -> 更新关系(Relationship)
  -> 形成想法(Internal Monologue) -> 决定是否行动(Behavior Planner)
  -> 执行行为(Animation/Speech) -> 产生新的共同经历(Memory) -> 回到感知
```

聊天只是其中一种输入。系统时间、鼠标、窗口、音乐、天气、屏幕使用, 甚至什么都没发生, 都是 Life Loop。

**MVP 实现 Life Loop 的骨架**: 感知 -> 情绪(带内稳态) -> 行为决策 -> 执行 -> 记忆。循环跑起来, 即使每个环节简化, 产品就有了"活着"的基础。二/三期逐步填充 Needs、Curiosity、Habits 等让循环越来越真实。

### 7.13 幻觉的角色化处理 (Graceful Hallucination Handling)

不制造错误, 但当真的幻觉发生且被用户抓到时, 她应该自然地独独抱歉, 而不是冷淡地“数据库已更新”。

与 User Correction Loop 联动:

1. 用户纠正 -> 触发 Correction Loop
2. 她角色化反应: “啊, 是我记错了嘛……对不起!” (不好意思地低头)
3. Fact Update (confidence 调整, 标记来源)
4. 下次不再犯

这样幻觉从一次“系统故障”变成了一次“她犯了个可爱的错”, 信任反而因为这种真实感而加固。

安全边界不变: 家庭/健康/纪念日/重要事实等内容不在此范畴内。

### 二期/三期补充

- Curiosity (好奇心): 发现你天天开某软件 -> 主动问, 来自好奇而非记忆/提醒。依赖 Pattern Detection 成熟度
- Habits (习惯): "每天9点健身"不是 Fact 是 Habit, 允许预测。依赖系统感知
- Repair (关系修复): 察觉上次对话让你不开心 -> 下次主动道歉
- Trust (信任维度): Relationship 拆分 Closeness + Trust, 秘密只在高 Trust 时触发
- Identity (身份认同成长): "我是谁"随经历缓慢成长
- Surprise Detection: Unexpectedness 作为 Importance 的补充维度
- Shared Goals (共同目标): "一起坚持健身", 不只是提醒, 而是参与

## 9. 系统感知

感知数据分两类:

- **实时状态 (不存储)**: 当前时间、前台应用、空闲时长、活动强度。直接喂给 Body (昼夜节律/注意力) 和 Mind (Behavior Planner)
- **感知型 Episode (需存储)**: "连续工作8小时""凌晨3点还在写代码"等有意义的时间模式。source.type = "system"

### 8.1 感知能力清单 (高级级)

- 时间感知: 当前时段、距上次交互多久、今天已使用电脑多久
- 在场检测: 鼠标键盘活动间隔, 判断"在电脑前""短暂离开""长时间离开"
- 窗口感知: 当前前台应用 + 应用分类(工作/娱乐/社交/浏览), 原始标题不长期存储
- 工作节奏: 连续工作时长、休息频率、是否进入深度专注
- 异常模式: 深夜使用、超长无休息、突然从工作切到娱乐

### 8.2 隐私设计

- 全部本地处理
- 窗口标题只提取"应用类别", 不写入数据库, 不发给云端
- 发给 LLM 的只有高度概括上下文
- 每个感知层可独立开关

---

## 10. 交互行为设计

### 9.1 被动交互

| 操作 | 行为 | 说明 |
|------|------|------|
| 单击头部 | 摸头 | 眨眼笑 + 亲密度+0.5, 冷却3秒防刷 |
| 单击身体 | 戳脸 | 惊讶弹跳, 连续戳3次生气鼓脸 |
| 拖拽 | 被拎起扑腾 | 反应随亲密度变化, 松手弹性落地 |
| 双击 | 打开对话气泡 | 她先说话 (Brain State 决定开场白) |
| 右键 | 极简菜单 | 导出记忆 / 暂时离开模式 / 关闭 |
| 拖文件给她 | 送礼物 | 接住并记住, 不同文件类型不同反应 |
| Alt+Space | 快捷唤醒 | 直接开始说话 |

### 9.2 主动行为

| 行为 | 触发 | 说明 |
|------|------|------|
| 冒泡说话 | Pending Event / Internal Thought / 随机闲聊 | 5秒消失, 非深度专注时弹出 |
| 走到你身边 | 长时间无互动 + 需要关心 | 走到鼠标附近拍拍箭头 |
| 主动窥屏 | 检测桌面状态变化 | 偷偷凑过来看屏幕, 根据你在干嘛给鼓励 |
| 番茄钟陪伴 | 用户开启番茄钟 | 专注时安静陪着, 时间到提醒休息 |
| 环境反应 | 系统感知驱动 | 深夜催睡觉、连续工作焦躁、玩游戏假装生气 |

### 9.3 喂食与物品

- 喂食: 右键选食物, Energy 恢复, 记住你爱喂她什么
- 收礼物: 拖文件给她, 她接住并记住

### 9.4 行为节制

- 不打扰原则: 深度专注时主动行为静音
- 频率控制: 冒泡最多每30分钟一次
- 情绪一致: 所有动作受 Emotion 驱动
- 亲密度门控: 陌生阶段不主动靠近
- 可关闭: 所有主动行为可在设置关掉

---

## 11. Emotion 状态机 (Mind-Body 桥梁)

Emotion 是 Mind 和 Body 之间的桥梁: Mind 写入状态, Body 读取状态驱动表现。

### 10.1 多维情绪模型

| 维度 | 说明 |
|------|------|
| 亲密度 | 0-100, 对数曲线增长, 不同阶段速率不同, 可下降 |
| Physical Energy | 体能, 蹦跳/走路消耗, 休息恢复 |
| Social Battery | 社交电量, 连续聊天消耗, 独处恢复 |
| Mood | 当前心情 (开心/平静/担心/调皮...) |
| Stress | 压力值, 长时间高强度交互后升高 |

### 10.2 Relationship Pace (亲密度增长)

- 对数型曲线, 不是线性
- 0->20 快 (几天), 60->80 慢 (几周)
- **可下降**: 一周不理她, 亲密度掉
- 深度对话 > 日常闲聊的加分

### 10.3 Emotion Blend

多维情绪不是离散标签, 是连续向量。70%开心 + 30%疲惫 = 混合状态。Live2D 参数连续插值, 自然过渡。

---

## 12. 安全与隐私

| 层面 | 措施 |
|------|------|
| 本地加密 | SQLite 加密存储 |
| 云端知情同意 | 对话原文 + 事实发给 LLM API, 首次使用时告知 |
| 选择性遗忘 | 用户可请求删除特定记忆 |
| 数据导出 | 从第一天支持, 丢记忆=丢关系 |
| 幻觉防护 | System Prompt 约束 + Grounded Generation |
| 感知隐私 | 窗口标题本地处理, 每层独立开关 |

---

## 13. 已废弃决策

| 决策 | 原因 |
|------|------|
| Surprise Budget (15% 随机 LLM) | 会翻车, 替换为 Memory Serendipity |
| Personality Consistency Engine | 不需要独立引擎, 通过 Core/Adaptive + Reflection 实现 |
| Planner 模块 (帮用户规划人生) | 用户要的是陪伴, 不是秘书 |
| Letta 式 Agent 自主管理记忆 | 每次操作消耗额外 LLM 调用, 太贵 |
| Graphiti 式时序知识图谱 | 依赖 Neo4j 太重, 保留时间有效性理念即可 |
| recall_count 乘法评分 | 热门记忆无限膨胀, 替换为 memory_strength |
| 固定输入框 | 立刻变成微信, 替换为临时气泡 + Alt+Space |
| 聊天记录面板 | 破坏沉浸感, 替换为记忆查询 |
| 行为学习 (MVP) | 行为塌缩风险, 放二期或三期, MVP 行为是设计的 |

---

## 14. MVP / 二期模块清单

### MVP - Mind (让她记住且用对)

| 模块 | 说明 |
|------|------|
| Memory Gate | 路由器, 判断输入流向 |
| Working Memory | 滑动窗口 |
| Episodes + Facts | 结构化存储, 时间有效性, confidence |
| Emotion (基础版) | 心情 + 亲密度 + 体能 + 社交电量 |
| Hybrid Retrieval | Score = 0.4语义 + 0.3strength + 0.2时间 + 0.1情绪 |
| Memory Trigger | 避免错误时机引用 |
| Behavior Planner | Intent 导演 |
| Prompt Budget | 价值密度压缩 |
| Grounded Generation | 归因, 防幻觉 |
| Pending Events | 主动关怀, MVP 成功标准核心 |
| Memory Confidence | 影响表达方式 |
| User Correction Loop | 用户纠正 -> 事实更新 |

### MVP - Body (让她活着)

| 模块 | 说明 |
|------|------|
| Animation FSM | 行为状态机, 可打断/不可打断 |
| 微行为系统 | Idle variety, 加权随机 + cooldown |
| Chat Bubble | 有生命的气泡, 随情绪变形 |
| 输入 UX | 临时气泡 + Alt+Space |
| Audio (Foley) | 基础音效 |
| 注意力三态 | Focused / Peripheral / Ignored |
| 昼夜节律 | 时段影响 idle 权重和动作速度 |
| 空间记忆 | 有"窝", 自动回巢 |
| 物理交互 | 坐窗口边缘、自由落体、任务栏 |
| Emotion 映射 | 配置文件, Blend |
| Mind-Body 解耦 | Body 持续运行不依赖 LLM |
| Debug Panel | 仅开发阶段 |

### MVP - Soul

| 模块 | 说明 |
|------|------|
| Internal Monologue | Reflection 产出, 对话间意识连续 |
| 初次登场动画 | 第一印象, 情感锚点 |

### 二期 - Mind (让她成长)

- Persona (完整版): Immutable Core + Adaptive
- Reflection: 异步反思, 更新 Persona + Internal Thoughts
- Memory Consolidation: 级联压缩, 信息流向 Facts/Persona
- Memory Lifecycle: 自动遗忘, 衰减删除

### 二期 - Body

- 番茄钟陪伴
- 跨显示器移动
- 性能降级 (全屏检测)

### 二期/三期 - Soul

- Rituals: 循环仪式 + 纪念仪式
- Memory Serendipity: ~5% 弱相关记忆关联
- Relationship Landmarks: 里程碑标记
- 告别动画
- Shared History: 完整记忆查询
- 行为学习 (带 novelty penalty 约束, 防塌缩)
- VITS/TTS 语音 (二期可选)

---

## 15. 待讨论

已确定:

- [x] 系统感知范围 (第8节): 高阶级, 全本地处理
- [x] 交互行为设计 (第9节)
- [x] 技术框架选型 (第3.2节): Tauri v2 + React + PixiJS
- [x] 无面板设计原则 (第3.4节)
- [x] Mind/Body/Soul 架构 (第4节)
- [x] 表达层: 微行为 + 有生命的气泡 (第6节)

待补充:

1. **Live2D 模型制作流程**: AI 生图 -> 图层拆分 -> 绑定 -> 导出
2. **多模态记忆输入**: 系统感知如何产生 Episode
3. **中文 Embedding 选型**: text-embedding-3-large vs BGE-M3 vs GTE
4. **LLM API 选型与成本估算**: 主对话模型 vs Reflection 小模型

---

## 16. 设计决策溯源

本文档由三方协同设计:

**GPT 贡献**: Memory Gate (路由器)、Memory Trigger、Memory Strength (艾宾浩斯)、Behavior Planner (导演/Intent)、Internal Monologue、Memory Serendipity、Episode 结构化、Persona 数据分离 (DDD)、Token Budget (价值密度)、Rituals、Circadian Rhythm、Spatial Memory、Attention States、Chat Bubble 生命化、Foley 音效、Emotion 配置文件+Blend、Memory Consolidation、Memory Lifecycle、Idle Variety、Mind/Body/Soul 三分法

**Codex 贡献**: Temporal Facts (时间有效性)、Pending Events Engine (合并 Goal+Expectation)、Relationship Pace (对数曲线+可下降)、多模态记忆输入概念、Memory Gate 路由器细化、Mind-Body 解耦运行、动画中断优先级、输入UX 盲区发现、Memory Serendipity (替换自己的 Surprise Budget)、行为学习放二期的判断

**用户**: 产品定位 (陪伴)、角色方向 (拟人化小动物)、性格定义 (温柔调皮)、技术栈方向 (Windows+云端)、无面板理念的坚持
