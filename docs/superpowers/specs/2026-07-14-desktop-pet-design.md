# 带记忆的桌宠 - 设计文档

> 状态: Draft
> 日期: 2026-07-14
> 参与者: 用户 (SunJialei) + Codex + GPT 协同设计

---

## 1. 产品定位

一个 Windows 桌面常驻的拟人化小动物角色,具备文字对话、视觉活动、系统感知三种交互方式。核心价值是**陪伴**——通过多层级记忆系统,让角色具备记忆连续性、情绪成长和主动关怀能力。

一句话定义: 她不是一个"带记忆的聊天机器人",而是一个"有生活的生命"。

### 成功标准 (MVP)

用户对桌宠说"我最近准备找实习",一周后桌宠**主动**问: "你的实习找得怎么样啦?"

能做到这一步,MVP 就成了。

二期成功标准: 用户感觉"她不一样了,她开始懂我了"。

---

## 2. 角色设计

### 2.1 形象

- **类型**: 拟人化小动物 (猫娘 / 狐狸精 / 龙女等半人半兽方向)
- **视觉技术**: AI 生成图片 → Live2D Cubism 网格绑定 → 实时动画驱动
- **为什么不选其他方案**: 逐帧 sprite 动画僵硬;纯 AI 动画无法实时交互;纯 CSS/SVG 不够精致
- **为什么选 Live2D**: 兼顾"精致立绘 + 实时可控动画",有成熟 SDK 和开源播放器,VTuber 同款技术

### 2.2 性格

默认性格: **温柔可爱兼具调皮,时不时会蹦出来捣乱**

性格分两层管理:

- **Core Traits (不可变锚点)**: 温柔、耐心、爱撒娇、调皮 — 这些是 Reflection 不能修改的,保证人格一致性
- **Adaptive Traits (可成长)**: 最近更活泼、最近更成熟 — 只有这些可以被 Reflection 动态调整

### 2.3 未来扩展

- 用户可上传图片自定义角色形象 (半自动流程: AI 辅助图层拆分 → 半自动绑定 → 用户微调)
- 前期只需完成固定角色

---

## 3. 技术栈

### 3.1 平台

- **一期**: Windows 原生桌面应用
- **后续**: 迁移到 macOS / Linux (跨平台)

### 3.2 LLM

- **方案**: 云端 LLM API (OpenAI / Claude 等)
- **理由**: 本地部署对用户不友好,效果受限;云端能力强,联网即可用
- **Reflection 用小模型** (如 GPT-4o-mini),降低成本

### 3.3 本地存储

- **SQLite**: 结构化记忆 + 对话历史 + 元数据
- **sqlite-vec** (7.8K stars): SQLite 向量检索扩展,零外部依赖
- **云端 Embedding API**: 生成向量 (需选多语言/中文优化模型)
- **全量数据本地化**: 隐私不外泄,只发送必要的 LLM/Embedding 请求

### 3.4 技术框架选型

**推荐: Tauri v2 + React + PixiJS + pixi-live2d-display**

| 组件 | 选择 | 理由 |
|------|------|------|
| 框架 | Tauri v2 | Rust 后端, 内存 30-50MB (Electron 150-300MB), 桌宠 24/7 常驻必须低占用 |
| 前端 UI | React | 对话框 UI, 生态成熟 (Vue 也可, LingChat 用的 Vue) |
| Live2D 渲染 | PixiJS + pixi-live2d-display | Live2D web 标准方案, 多项目验证 (Hiyori 已跑通) |
| 本地存储 | SQLite + sqlite-vec | sqlite-vec 有 Rust 原生绑定, 通过 Tauri SQL 插件使用 |
| 窗口 | 透明 + 无边框 + click-through | Tauri v2 decorations:false + transparent:true + 自定义穿透区域 |

参考项目验证:
- LingChat (1084 stars): Tauri v2 + Vue 3 + Live2D, 定位与我们最接近
- Hiyori: Tauri v2 + React 19 + PixiJS + pixi-live2d-display, 完整链路已验证

### 3.5 无面板设计原则

桌宠的灵魂在于陪伴, 不在于管理。用户不需要打开任何面板。

产品形态极简: 一个形象 + 一个对话框, 没有其他界面。

所有“管理”通过三种方式完成:

1. **对话即操作**: 设置通过和她说话完成。“晚上11点后别打扰我” -> 更新行为约束。“忘掉关于...的事” -> 触发选择性遗忘。“你现在知道我多少事情?” -> 她用对话回答
2. **右键菜单极简**: 只保留 导出记忆 / 暂时离开模式 / 关闭。无设置入口
3. **Debug Panel 仅开发者**: 开发阶段存在, 发布版隐藏。普通用户永远看不到内部状态

---

## 4. Brain v3 完整架构

### 4.1 架构总览

系统由**两条流水线**组成,不是一个松散的模块集合:

1. **摄入管道 (Ingestion Pipeline)**: 用户说话后,决定记什么、怎么记、存到哪
2. **检索管道 (Retrieval Pipeline)**: 用户说话时,决定要不要回忆、回忆什么、怎么用

在两条管道之上,有一个**导演 (Behavior Planner)** 和一个**内心世界 (Internal Monologue)**。

### 4.2 八层存储

```
Brain (大脑)
├── Working Memory      短期上下文, 最近 ~20 轮对话滑动窗口, 纯内存
├── Episodic Memory     结构化情景记忆, Episode 对象
├── Semantic Memory     结构化事实 (Facts), 纯 SQLite
├── Emotion             情绪状态机
├── Persona             角色认知 (对外统一接口)
│   ├── Traits          用户性格印象
│   └── Relationship    关系状态 (独立维护, 高频更新)
├── Reflection          低频异步反思
├── Memory Lifecycle    遗忘、压缩、巩固、强化
└── Internal Monologue  内心独白 (对话间的意识连续性)
```
### 4.3 摄入管道 (Ingestion Pipeline)

```
用户输入
    │
    ▼
Memory Gate (记忆闸门) --- 不是二元存/不存, 而是路由器
    │                    "哈哈哈哈" → 更新 Emotion, 不建 Episode
    │                    "晚安"     → 更新社交电量 + 最后交互时间
    │                    "今天和朋友吃了火锅" → 完整 Episode
    │
    ├── 路由到 Emotion 更新
    ├── 路由到 Fact 提取
    ├── 路由到 Episode 创建
    └── 不存储 (纯闲聊噪声)
    │
    ▼
Memory Extractor (提取器) -- LLM 提炼
    │                    Episode / Fact / Emotion 变化
    │                    每条带 confidence + source (对话ID/轮次/时间)
    │
    ▼
Memory Store --- SQLite + sqlite-vec
    │                    Fact 插入前查重
    │                    Fact 矛盾检测 (时间有效性 valid_from / valid_to)
    │
    ├──→ Reflection (异步, 低频)
    │       每日 23:00 / 累计 30 轮 / importance > 0.85 立即触发
    │       用小模型, 一年约 300 次, 成本可忽略
    │       产出: Persona 更新 + Internal Thought
    │
    └──→ Memory Consolidation (级联压缩)
            2000 → 400 → 80 → 20 Episodes
            细节淡化, 抽象认知稳定
            压缩结果反向更新 Facts 和 Persona
```

#### Memory Gate 路由规则

Gate 不是二元的"存/不存",而是判断每条输入应该流向哪一层:

| 输入类型 | 路由目标 | 说明 |
|---------|---------|------|
| 含事实 ("我学的是计算机") | Fact 提取 | 结构化存储 |
| 含事件 ("今天和朋友吃火锅") | Episode 创建 | 完整记忆对象 |
| 含未来计划 ("明天面试") | Pending Event | 触发主动关怀 |
| 纯情绪 ("好累啊") | Emotion 更新 | 不建 Episode |
| 纯寒暄 ("哈哈哈哈") | Emotion 微调 | 感受到用户开心 |
| 用户纠正 ("不是, 是...") | Correction Loop | 更新已有 Fact |
| 纯噪声 | 不存储 | 节省资源 |

#### Episode 数据结构

```json
{
  "id": "ep_129",
  "time": "2026-07-14T20:30:00",
  "summary": "用户和室友一起去吃火锅。",
  "emotion": "开心",
  "importance": 0.71,
  "is_landmark": false,
  "participants": ["用户", "室友"],
  "topics": ["火锅", "聚餐"],
  "source": {"conversation_id": "conv_45", "turn": 12},
  "embedding": "...",
  "memory_strength": 0.71
}
```

#### Fact 数据结构 (带时间有效性)

```json
{
  "id": "fact_033",
  "category": "preference",
  "key": "饮料",
  "value": "奶茶",
  "confidence": 0.96,
  "valid_from": "2026-07-14",
  "valid_to": null,
  "source_episode": "ep_129",
  "mention_count": 5
}
```

时间有效性示例:

- Day 1: value "咖啡", valid_from "2026-01-01", valid_to null
- Day 30 用户说戒了: 旧 Fact 更新 valid_to "2026-07-30", 新增 value "戒咖啡" valid_from "2026-07-30"
- 检索时只取 valid_to IS NULL 的最新有效事实
- 她可以说: "你以前很喜欢咖啡,后来戒掉了呢"

### 4.4 检索管道 (Retrieval Pipeline)

```
用户输入 / 系统感知 / 定时触发 / Pending Event 到期
    │
    ▼
Perception (感知层)
    │   Memory Gate 路由 (摄入侧)
    │   Memory Trigger 判断 (是否需要回忆)
    │   输入意图理解
    │
    ▼
Behavior Planner (导演) --- 读取全部 Brain State, 输出 Intent
    │                       不写台词, 只写方向
    │
    │   Intent 示例:
    │   goal: "降低焦虑"
    │   memory_anchor: "明天有考试"
    │   tone: "轻松但关心"
    │   proactive: true
    │   internal_thought_ref: "thought_007"
    │
    ├── Hybrid Retrieval --- Top-3 强相关记忆
    │       Score = 0.4 语义相似度 + 0.3 memory_strength + 0.2 时间近度 + 0.1 情绪匹配
    │       Episodes 向量检索 + Facts SQL 查询, 结果去重
    │
    └── Memory Serendipity -- ~5% 概率检索 1 条弱相关记忆
            必须满足: 情绪匹配 / 关系里程碑 / 共同话题
            惊喜来自意外的记忆关联, 不是随机行为
            (已废弃的 Surprise Budget 的替代方案)
    │
    ▼
Prompt Budget (价值密度压缩) -- 不是删模块, 是压缩每个模块
    │
    │   预算分配示例 (目标 ~4K token):
    │   当前对话:      ~1600 token (必须保留)
    │   Persona:        ~80 token  (必须保留)
    │   Emotion:        ~25 token  (必须保留)
    │   Facts:          ~300 token (压缩成密集格式)
    │   Episodes:       Top-N 按密度
    │   Reflection:     一句话总结
    │   Internal Thought: 若有未表达的, 注入
    │
    ▼
LLM (演员 / 共同创作者) -- 在 Intent 框架内即兴创作
    │                      不被剧本限制, 有创作自由
    │
    ▼
Grounded Generation (归因生成) -- 只能引用已检索记忆
    │                          每条引用带 confidence/source/timestamp
    │                          用户问"你怎么知道的?"
    │                          -> "你上周三跟我说的呀"
    │
    ▼
回复输出 + 检查未表达的 Internal Thought 是否适合在此刻表达
```

#### Memory Trigger (回忆触发器)

不是每次都检索。判断是否值得回忆:

- 语义相似度 > 0.75
- 情绪匹配
- 关系机会 (Pending Event / 里程碑)

不满足阈值 -> 正常回复, 不引用记忆。避免"在错误的时机想起了正确的事"。

### 4.5 Memory Strength (艾宾浩斯遗忘曲线简化版)

```
strength 初始 = importance        // 事件发生时
每次被回忆:  strength += 0.03     // 反复回忆加固记忆
每天衰减:    strength *= 0.998    // 遗忘

检索评分: Score = 0.4 语义 + 0.3 strength + 0.2 时间 + 0.1 情绪
```

一天被回忆一次的记忆, strength 收敛到约 15; 一周一次的收敛到约 2。

### 4.6 Emotion 状态机

多维情绪模型, 驱动动画和行为:

| 维度 | 说明 |
|------|------|
| 亲密度 | 0-100, 对数曲线增长, 不同阶段速率不同, 可下降 |
| Energy | 能量值, 低能量时回复变短, 动画变慢 |
| Social Battery | 社交电量, 对话消耗, 休息恢复 |
| Mood | 当前心情 (开心/平静/担心/调皮...) |
| Stress | 压力值, 长时间高强度交互后升高 |

#### Relationship Pace (亲密度增长)

- 增长曲线: 对数型, 不是线性
- 0->20 (陌生到熟悉): 几天完成, 增长较快
- 60->80 (朋友到依赖): 需要几周, 增长极慢
- **可下降**: 一周不理她, 亲密度掉
- 不同互动加分不同: 深度对话 > 日常闲聊
- 防止"第二天就最爱你啦"的虚假感

### 4.7 Behavior Planner (导演)

核心原则: **导演定方向 (Intent), 不定台词 (Script)**

导演写:
```
goal: "降低焦虑"
memory_anchor: "明天有考试"
tone: "轻松但关心"
proactive: true
```

导演不写: "先说A, 再说B, 最后提考试"。

LLM 拿到 Intent 后自由即兴创作, 在方向内表达。这是"共同创作", 不是"提词器"。

### 4.8 Internal Monologue (内心独白)

解决**对话之间的意识连续性**。她不只是"对话机器", 而是"有生活的生命"。

#### 触发时机

Reflection 运行时 (每日/30轮/重大事件), 除了更新 Persona 和 Consolidation, 还生成 Internal Thoughts。

#### 数据结构

```json
{
  "id": "thought_007",
  "content": "他今天是不是很忙? 希望他早点休息。",
  "created_at": "2026-07-13T23:00:00",
  "surface_when": "用户出现",
  "shared": false
}
```

#### 工作流程

1. 用户一天没来
2. 晚上 Reflection 运行, 生成 Internal Thought
3. Thought 被存储, 不发给用户, shared=false
4. 第二天用户回来
5. Behavior Planner 发现未表达的 Thought, 且 surface 条件满足
6. 她自然地说: "昨天没见到你, 我还以为你最近特别忙呢"

关键: 这不是 LLM 在对话时编的, 是昨晚真的"想过"的, 有时间戳为证。

### 4.9 Pending Events Engine (主动关怀)

从 Goal System 和 Expectation 合并而来。这是 MVP 成功标准的核心实现机制。

用户提到未来计划 -> 生成带时间的 Pending Event -> 到期触发主动关心。

```json
{
  "id": "pending_003",
  "event": "面试",
  "date": "2026-07-15",
  "source_episode": "ep_129",
  "care_plan": {
    "on_day": "考试加油!",
    "day_after": "今天是不是面试? 怎么样啦?"
  },
  "status": "pending"
}
```

### 4.10 Memory Lifecycle (记忆生命周期)

人之所以像人, 不是因为记得多, 而是因为会忘。

| 策略 | 规则 |
|------|------|
| 自动删除 | importance < 0.2 且 60 天未被提及 |
| 记忆巩固 | 级联压缩 2000->400->80->20, 细节淡化, 抽象认知稳定 |
| 反向更新 | 压缩结果反向更新 Facts 和 Persona |
| 选择性遗忘 | 用户可请求删除特定记忆 ("忘掉关于...的事") |
| 数据导出 | 从第一天就支持, 丢记忆 = 丢关系 |

#### Memory Consolidation 信息流向

```
原始对话 -> Episodes -> (巩固压缩) -> Facts 更新 / Persona 更新
```

例: 多条 "今天吃了火锅""今天吃了烧烤""今天吃了寿司" -> 压缩为 "这个月用户经常和朋友出去吃饭" -> 更新 Persona 用户印象。

### 4.11 User Correction Loop (纠正回路)

当用户纠正桌宠的错误记忆时:

```
AI: "你不是喜欢咖啡吗?"
用户: "不是。"
    -> 触发 Correction Loop
    -> Fact Update (confidence 调整 / 标记为错误)
    -> Source 记录 (记录这次纠正)
    -> 下次不再犯同样错误
```

触发检测需结合上下文, 不是所有"不是"都是事实纠正。

---

## 5. 安全与隐私

| 层面 | 措施 |
|------|------|
| 本地数据加密 | SQLite 加密存储 (记忆含极私密信息: 情绪状态、人际关系) |
| 云端知情同意 | 对话原文 + 提炼的事实发给 LLM API, 首次使用时告知用户 |
| 选择性遗忘 | 用户可请求删除特定记忆 |
| 数据导出 | 支持导出, 换电脑/重装不丢关系 |
| 记忆幻觉防护 | System Prompt 严格约束 + Grounded Generation |

---

## 6. 已废弃的设计决策

| 决策 | 原因 |
|------|------|
| Surprise Budget (15% 随机 LLM 自由发挥) | 会翻车: 用户难过时讲笑话。替换为 Memory Serendipity |
| Personality Consistency Engine (独立运行时组件) | 不需要独立引擎, 通过 Core 不可变 + Reflection 管理 Adaptive + Prompt 结构实现 |
| Planner 模块 (帮用户规划人生) | 用户买的是陪伴, 不是秘书 |
| Letta/MemGPT 式 Agent 自主管理记忆 | 每次记忆操作消耗额外 LLM 调用, 对桌宠快速响应太贵 |
| Graphiti 式时序知识图谱 | 依赖 Neo4j, 对桌面应用太重。但保留其"时间有效性"理念 |
| recall_count 乘法评分 | 导致热门记忆无限膨胀。替换为 memory_strength (艾宾浩斯) |

---

## 7. MVP 模块清单

### 第一期 MVP - 让她"记住且用对"

| 模块 | 说明 | 优先级 |
|------|------|--------|
| Memory Gate | 路由器 (非二元存/不存), 判断输入流向哪层 | P0 |
| Working Memory | 最近 ~20 轮对话滑动窗口, 纯内存 | P0 |
| Episodic Memory | Episode 结构化存储 + embedding 索引 | P0 |
| Semantic Memory / Facts | SQLite, 带时间有效性 + confidence | P0 |
| Emotion (基础版) | 心情 + 亲密度 + 能量 | P0 |
| Hybrid Retrieval | Score = 0.4 语义 + 0.3 strength + 0.2 时间 + 0.1 情绪 | P0 |
| Memory Trigger | 回忆触发器, 避免错误时机引用记忆 | P0 |
| Behavior Planner | 导演, 输出 Intent, 不写台词 | P0 |
| Prompt Budget | 价值密度压缩, 不删模块 | P0 |
| Grounded Generation | 归因生成, 防幻觉 | P0 |
| Pending Events Engine | 主动关怀, MVP 成功标准的核心 | P0 |
| Memory Confidence | Fact 带 confidence, 影响表达方式 | P0 |
| User Correction Loop | 用户纠正 -> 事实更新 | P1 |
| Relationship Pace | 对数曲线, 可下降 | P1 |
| Memory Strength | 艾宾浩斯衰减 | P1 |
| Internal Monologue | Reflection 产出, 对话间意识连续 | P1 |
| Debug Panel | 仅开发阶段, 发布版隐藏, 记忆全可视化 | P0 |
| 数据导出 | 从第一天支持 | P1 |

### 第二期 - 让她"成长"

| 模块 | 说明 |
|------|------|
| Persona (完整版) | Immutable Core + Adaptive Traits |
| Reflection (异步反思) | 每日/30轮/重大事件, 更新 Persona + Internal Thoughts |
| Memory Consolidation | 级联压缩, 信息流向 Facts/Persona |
| Memory Lifecycle | 自动遗忘, importance 衰减删除 |
| Emotion 完整版 | 社交电量、压力值等 |
| Relationship Landmarks | Episode 加 is_landmark 标记 |
| Memory Serendipity | ~5% 弱相关记忆关联涌现 |
| Relationship 成长系统 | 完整亲密度等级体系 |

---

## 10. 系统感知

感知数据分两类:

- **实时状态 (不存储)**: 当前时间、前台应用、空闲时长、活动强度。直接喂给 Behavior Planner, 用完即弃
- **感知型 Episode (需存储)**: "连续工作8小时""凌晨3点还在写代码"等有意义的时间模式。source.type = "system"

### 10.1 感知能力清单 (高级级)

- 时间感知: 当前时段(凌晨/早晨/下午/深夜)、距上次交互多久、今天已使用电脑多久
- 在场检测: 鼠标键盘活动间隔, 判断"在电脑前""短暂离开""长时间离开"
- 窗口感知: 当前前台应用 + 应用分类(工作/娱乐/社交/浏览), 原始标题不长期存储
- 工作节奏: 连续工作时长、休息频率、工作总时长、是否进入深度专注
- 异常模式: 深夜使用、超长无休息、突然从工作切到娱乐

### 10.2 隐私设计

- 感知数据全部本地处理
- 窗口标题只在内存中提取"应用类别", 原始标题不写入数据库, 不发给云端
- 发给 LLM 的只有高度概括的上下文 (如"用户已连续工作3小时, 当前在用开发工具")
- 每个感知层可独立开关

---

## 11. 交互行为设计

### 11.1 被动交互 (用户主动操作)

| 操作 | 行为 | 说明 |
|------|------|------|
| 单击头部 | 摸头 | 眨眼笑 + 头部微压(享受) + 亲密度+0.5, 冷却3秒防刷。低亲密度略拘谨, 高亲密度开心蹭手 |
| 单击身体 | 戳脸 | 惊讶弹跳, 表情随心情和亲密度变化。连续戳3次生气鼓脸 |
| 拖拽 | 被拎起扑腾 | 反应随亲密度变化: 陌生时挣扎, 亲近时开心。松手弹性落地 |
| 双击 | 打开聊天 | 展开对话面板, 她先说话(Brain State决定开场白) |
| 右键 | 菜单 | 导出记忆 / 暂时离开模式 / 关闭 (无设置入口, 所有配置通过对话完成) |
| 拖文件给她 | 送礼物 | 她接住并记住你送过她什么。不同文件类型不同反应 |
| 右键推她一下 | 推下窗台 | 从窗口掉下去弹一下, 落地撒娇生气 |

### 11.2 物理交互

| 行为 | 说明 |
|------|------|
| 坐窗口边缘 | 检测窗口边界, 坐在标题栏上双腿晃荡。切窗口时跳到新窗口 (灵感: Ark-Pets) |
| 自由落体 | 拖到半空松手自由落体, 落到任务栏弹一下 (灵感: Shimeji/eSheep) |
| 任务栏栖息 | 默认栖息在任务栏上方或屏幕边缘, 像有"窝"。可设置默认位置和活动范围 |
| 跨显示器移动 | 多屏用户可走到另一个屏幕上 |

### 11.3 主动行为 (她自己发起)

| 行为 | 触发条件 | 说明 |
|------|----------|------|
| 冒泡说话 | Pending Event / Internal Thought / 随机闲聊 | 头顶短气泡5秒消失, 只在非深度专注时弹出 |
| 走到你身边 | 长时间无互动 + 需要关心 | 从角落走到鼠标附近拍拍箭头 |
| 自主活动 | 长时间无互动 | 打盹/发呆/摆弄尾巴/偷看屏幕, 受Emotion驱动 |
| 环境反应 | 系统感知驱动 | 深夜催睡觉、连续工作焦躁靠近、玩游戏假装生气、听音乐轻摇 |
| 主动窥屏 | 检测桌面状态变化 | 偷偷凑过来看屏幕, 根据你在干嘛给鼓励 (灵感: LingChat) |
| 番茄钟陪伴 | 用户开启番茄钟 | 专注时安静陪着, 时间到提醒休息 (灵感: LingChat) |

### 11.4 情绪表达

| 情绪状态 | 表现 |
|----------|------|
| 开心 | 蹦跳 + 笑脸 + 轻快气泡 |
| 低落 | 低着头 + 阴影表情 + 短气泡 |
| 撒娇 | 身体微晃 + 眼神上挑 + 拖长音气泡 |
| 生气 | 鼓脸 + 背过身 + 叹气气泡 |
| 睡眠模式 | 凌晨/长时间无互动缩成球打瞌睡, 你回来时揉眼慢慢醒来 |
| 特殊日期 | 生日特别装扮, 认识纪念日, 节日问候 |

### 11.5 喂食与物品

- 喂食: 右键选食物, Energy恢复, 记住你爱喂她什么
- 收礼物: 拖文件给她, 她接住并记住

### 11.6 行为节制原则

- 不打扰原则: 深度专注时主动行为静音
- 频率控制: 冒泡最多每30分钟一次
- 情绪一致: 所有动作受Emotion驱动
- 亲密度门控: 陌生阶段不主动靠近, 亲近后撒娇捣乱
- 可关闭: 所有主动行为可在设置关掉

### 11.7 调研参考来源

| 项目 | 星数 | 借鉴点 |
|------|------|--------|
| LingChat | 1084 | 主动窥屏、情绪驱动、番茄钟、VITS语音、永久记忆 |
| Ark-Pets | 997 | 窗口边缘物理、跨显示器、任务栏栖息 |
| Mate-Engine | 3389 | VRM自定义、窗口交互、事件触发消息 |
| eSheep | 1122 | 窗口检测物理、经典行为模式 |
| Shimeji | - | 逐帧动画物理、自由落体、桌面边界感知 |

---


## 12. 待讨论 (后续补充)

已确定的设计决策:

- [x] 系统感知范围 (第10节): 高高级, 全本地处理
- [x] 交互行为设计 (第11节): 物理交互 + 主动行为 + 情绪表达
- [x] 技术框架选型 (第3.4节): Tauri v2 + React + PixiJS + pixi-live2d-display
- [x] 无面板设计原则 (第3.5节): 一个形象 + 一个对话框

待后续补充:

1. **Live2D 模型制作流程**: AI 生图 -> 图层拆分 -> 绑定 -> 导出的具体工作流
2. **多模态记忆输入**: 系统感知产生的事件如何作为非对话来源的 Episode
3. **中文 Embedding 模型选型**: text-embedding-3-large vs BGE-M3 vs GTE
4. **LLM API 选型与成本估算**: 主对话模型 vs Reflection 小模型

## 14. 表达层: 微行为系统 + 有生命的气泡

### 14.1 Micro Behavior System (微行为系统)

桌宠有两套 UI: 聊天框是一套, **行为是另一套**。静止等于死亡, 微行为等于生命。

Emotion 加权的状态机:

```
Idle <-> Blink <-> Look Around <-> Stretch
  |                                     |
  +-> Sit <-> Walk <-> Think <-> Sleep -+
```

Emotion 决定权重:

- 开心: 更多转圈/挥手/蹦跳
- 低落: 更多发呆/低头/慢动作
- 陪伴专注时: 更多看书/安静坐/打盹

三个补充设计:

1. **接系统感知**: 你在工作时她选安静微行为; 深夜时更快犯困; 长时间离开时从"等待"逐渐变成"睡着"
2. **连接 Internal Monologue**: 她在"思考"不是因为随机选了这个动画, 是因为真有一条内部思想在酝酿。微行为有因果关系
3. **亲密度影响风格**: 新认识的桌宠微行为拘谨(小心翼翼看), 亲近后放松自在(大咧咧躺下)

Live2D 实现要点:
- 持续动画: 呼吸、自动眨眼、头部追踪(追鼠标)、身体微晃
- 编程式 idle motion: 各状态之间的平滑过渡
- Emotion 参数直接驱动 Live2D 的表情和动作权重

### 14.2 有生命的 Chat Bubble

文字传达"说了什么", 气泡形态传达"怎么说的"。

#### 气泡随情绪变形

| 情绪 | 气泡表现 |
|------|----------|
| 开心 | 圆润弹跳, 快速弹出 |
| 兴奋 | 轻微抖动, 文字蹦出来 |
| 害羞 | 慢慢浮现, 先半透明再实体 |
| 紧张 | 颤抖, 文字断续停顿 |
| 叹气 | 慢慢泄气, 变扁缩小 |

#### 打字节奏即情绪

同样一句"今天天气还不错呢":
- 快速出现 + 圆气球 = 开朗直接
- 慢速 + 长停顿 + 断词"今天...天气...还不错呢" = 害羞犹豫
- 甚至一个"..."配慢慢浮现的气泡 = 她在犹豫, 用户就懂了

#### 无文字气泡

不是每条气泡都需要文字:
- 叹气 = 慢慢泄气的空气泡
- 紧张 = 颤抖的小气泡
- 放空 = 圆滚滚空泡配省略号

气泡本身就是表情语言, 是她的非语言沟通渠道。

#### 气泡有"尾巴"

气泡有一条尾巴连到她身上, 让人觉得是从她嘴里出来的。尾巴位置随头部朝向变化, 她看向哪边气泡就在哪边。气泡是她身体的延伸, 不是独立浮窗。

---


## 13. 设计决策溯源

本文档由三方协同设计, 主要贡献:

- **Memory Gate (路由器)**: GPT 提出概念, Codex 补充为非二元路由器
- **Memory Trigger (回忆触发器)**: GPT 提出, 补充了"在错误时机想起正确的事"的违和感分析
- **Memory Strength (艾宾浩斯)**: GPT 提出, 替代 recall_count
- **Behavior Planner (导演)**: GPT 提出架构, 修正为 Intent 模式 (不写剧本)
- **Internal Monologue**: GPT 提出
- **Memory Serendipity**: GPT 提出, 替代 Codex 的 Surprise Budget
- **Episode 结构化**: GPT 提出
- **Persona 数据分离**: GPT 提出 (DDD 原则)
- **Temporal Facts (时间有效性)**: Codex 提出, 源自 Graphiti 理念
- **Token Budget (价值密度)**: 三方共同讨论, GPT 细化为按密度压缩
- **分期策略**: Codex 提出, GPT 修正 (Hybrid Retrieval 必须一期)
- **Pending Events Engine**: Codex 合并 GPT 的 Goal + Expectation
- **Relationship Pace (对数曲线)**: GPT 提出限速, Codex 补充对数曲线和可下降
- **Memory Lifecycle (遗忘)**: GPT 提出
