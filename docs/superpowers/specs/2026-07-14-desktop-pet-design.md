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

### 3.4 前端渲染

- Live2D Cubism SDK for Web (或原生)
- 桌宠角色常驻桌面最上层,透明背景
- 可拖拽、可点击、可展开聊天窗口

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
| Debug Panel | 开发自用, 记忆全可视化 | P0 |
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

## 8. 待讨论 (后续补充)

以下话题在记忆架构完成后需要进一步设计:

1. **系统感知范围**: 桌宠能感知哪些系统信息 (时间/前台窗口/工作时长/软件类型), 隐私边界在哪里
2. **预设交互动作**: 点击/拖拽/主动冒泡的具体行为设计
3. **Live2D 模型制作流程**: AI 生图 -> 图层拆分 -> 绑定 -> 导出的具体工作流
4. **技术框架选型**: Electron vs Tauri vs WPF, 前端框架选型
5. **多模态记忆输入**: 系统感知产生的事件如何作为非对话来源的 Episode
6. **中文 Embedding 模型选型**: text-embedding-3-large vs BGE-M3 vs GTE
7. **LLM API 选型与成本估算**: 主对话模型 vs Reflection 小模型

---

## 9. 设计决策溯源

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
