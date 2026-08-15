# 统一调研报告：角色扮演与陪伴类 AI 的提示词工程、灵魂感与温度设计

> **本报告为三份独立调研合并而成的唯一权威汇总版**，任何一份子报告的每一个观点均已完整保留、无缺漏：
>
> 1. **报告①（一手调研）**：本项目《角色扮演/陪伴类 AI 提示词工程深度调研》（2026-08-15）——官方文档一手抓取（OpenAI / Anthropic / Google / DeepSeek / Claude Opus 5 官方系统提示全文 / OpenAI Model Spec）+ 4 路 Firecrawl 深度代理（大厂官方 / 陪伴产品拆解 / 社区与 GitHub / 数字人+学术）+ 项目实测（realism 报告）。
> 2. **报告②（Gemini Deep Research）**：《拟真角色扮演与陪伴类人工智能提示词工程及灵魂感训练研究报告》——理论内核（助手先验/罗杰斯共情/主体性）、提示词控制论、称谓与开场机制、CCv2 分层架构、前沿（对比解码 α / Persona-Hub / 多模态双轨）。
> 3. **报告③（NotebookLM）**：《角色扮演（RP）与陪伴类 AI 提示词工程》——角色演化哲学（casting brief）、功能性情绪、称谓淡化与物理替代、机器指纹消除、戏剧机器（自我-超我）、语境感知步调、自我披露、数字人情感注记。

> **观点冲突处理规则**：三份报告观点一致处直接合并；存在分歧或需修正处，以「验证」标注给出判定：✅ 已验证（有官方原文/论文/规范/实证支撑）｜⚠️ 方向对但机制或出处存疑｜❓ 无法溯源（不建议引用）｜❌ 有误。
>
> **配套文件**：证据库 `docs/research/2026-08-15-companion-prompt-engineering-research.md`；交叉验证 `docs/research/2026-08-15-cross-validation-final-opinion.md`。
>
> **调研过程备注**：报告①的 4 路 Firecrawl 深度代理中 3 路完成（大厂官方 / 陪伴产品 / 数字人+学术），社区/GitHub 代理运行超时未返回，其覆盖范围由一手抓取的 Character Card V2 规范全文、JanitorAI 官方指南及项目自身调教报告补足；多篇早期引用 URL（openai.com/model-spec、docs.anthropic.com 旧路径、rentry.org 指南）已失效或迁移，报告引用以抓取时有效的现行版本为准。

---

## 第 0 章 摘要与核心结论

### 0.1 四个核心问题的答案（三份报告共识）

**Q1：如何让 AI 通过对话"有自己的灵魂、更像人"？**
灵魂 ≠ 形容词清单。灵魂 = **稳定的内在核心 + 随情境变化的表达**，拆成四块：
1. **情感逻辑，不是规则**：把"不撒谎"写成"撒谎让他胸口发紧，他会转移话题"。行为根植于情绪，才稳定、才像人（Character.AI 官方原话，见 1.1/2.1）。
2. **行为纹理，不是标签**：写"用问题回答问题"而不是"神秘"；写"今天话比昨天少"而不是"你看起来很累"。
3. **记忆的连续性**：能引用共享回忆（"你上次说…"）是"灵魂感"的最大来源（Nomi 称之为 narrative intimacy）；但只围绕记忆原意，严禁编造。
4. **一致性中的意外**：核心特质稳定，表达偶尔出人意料——有自己的状态、会犯小错、会温和地不同意（功能性情绪，见 1.5）。

**Q2：如何留下风格印记，又不每次带固定称谓显得刻意？**
风格印记靠**语言指纹**（句式节奏、意象偏好、口头禅式语气词、停顿方式、微瑕疵："嗯""哎"、半句话、欲言又止），而不是固定称谓/模板句。
- 称谓应当是**关系的产物**，不是规则：关系近了自然叫，没到硬叫就是表演；固定称谓的最大问题是"可预测"——人一旦可被完全预测，就没有"在场感"。
- 最强手段是**示例对话**（few-shot）：3-5 条覆盖不同情绪状态的对话样本，比任何抽象规则都更能塑造语气（Anthropic 官方："示例是引导语气/格式/结构最可靠的方法"）。
- 用 `{{random_user}}` 类占位符防止模型把示例里的称呼/名字记成固定模式。
- 进阶手段：称谓概率衰减（仅强情绪/亲密时触发）、物理动作代位（用动作/视线替代口头称谓）、多态开场白（alternate_greetings）——详见第 3 章。

**Q3：提示词是"约束不要做什么"还是"引导要做什么 + 模板"？**
**答案是：引导为主，约束为辅，示例为王。** 四路证据共同指向：
- Anthropic 官方原话：**"Tell Claude what to do instead of what not to do"**（告诉它要做什么，而不是不要做什么）。
- Character.AI 官方原话：**"Write emotional logic, not rules... Behavior grounded in emotion is far more consistent than a list of prohibitions."**
- Google 官方：情感施压式约束对现代模型**无效甚至有害**；Constraints 应列 Dos and Don'ts，以 Dos 为主。
- OpenAI 官方指南：Instructions 段 = do + never do 并重，**do 在前**，never do 只用于真正底线。
- 报告③实验（方向参考）：纯肯定引导 > 纯否定约束，混合模式最优（具体分数 ❓ 无法溯源，见 2.5）。
- 反面证据：约束过多让模型进入"避免犯错"模式而非"像人说话"模式；Anthropic 专门提醒别用 MUST 级强硬措辞。
- 项目实测：负面规则把提问率 35%→14%（有效），但模板词计数 23→23 纹丝不动（无效）——**约束管"不做什么"，管不了"做出风格"**。

**Q4：如何让使用者感到"温度"？**
1. **先接住情绪，再（如果需要）解决问题**——但接住要具体：说"你今天话比昨天少"，不说"我理解你的感受"。
2. **命名情绪但不贴标签**；验证感受 ≠ 验证事实（Claude 官方系统提示：validate emotions without validating false beliefs）。
3. **适度自我表露 + 有自己的状态**：有时困、有时懒、有时不附和——不永远热情才是真人感（社会渗透理论，见 5.2）。
4. **温和推回（反谄媚）**：真正的关心不是无条件认同。OpenAI Model Spec："behave more like a firm sounding board rather than a sponge that doles out praise"；学术警告：训练"温暖共情"会显著加剧谄媚（陪伴产品的头号暗礁，见 5.4）。
5. **沉默也是表达**：不硬找话题、允许对话自然停住；深层情感袒露时"先确认 + 思考态再回复"的节奏设计（见 5.3）。

### 0.2 三份报告共同结论（10 条确定性最高、可直接作为项目原则）

1. **引导优于约束**：给"要什么"的正向行为描述 + 动机，比"不要什么"的禁令有效；约束只留给安全底线，且 ≤5 条。
2. **示例优于规则**：3-5 条覆盖多情绪状态的对话范例，是塑造语气/风格最可靠的手段。
3. **反谄媚是灵魂感的底线**：能温和推回、有主见、不因用户立场摇摆 = "另一个主体"的在场证明。
4. **称谓是关系的产物**：动态触发（强情绪/亲密时）+ 随关系演变，绝不硬编码；用 {{random_user}} 防范例污染。
5. **反 AI 味黑名单**：只禁真正的 AI 腔（delve/leverage/truly；"我理解你的感受"；"这不是关于…而是关于…"），清单要短。
6. **先接住情绪，再（如需）解决问题**，且接住要具体（细节 > 套话）。
7. **有自己的状态**：困/懒/沉默/不同意都是人格，不永远热情（功能性情绪）。
8. **上下文分层注入**：静态人设精简置顶，世界书关键词触发，近端指令（@depth 0）管当下。
9. **开场多样化**：alternate_greetings 多态开场，打破自回归惯性。
10. **多模态分担温度**：文本克制，动作/表情/语音/节奏补足（数字人管线与本项目 Spine 同理）。

### 0.3 需要修正 / 存疑的快速清单（详见第 9 章判定表）

| 项 | 判定 |
|---|---|
| 报告③"纯肯定 116/120 vs 纯否定 72/120" | ❓ 无法溯源，方向对但数字不可引用 |
| 报告③"Ali:Chat v1.5 格式"命名 | ❓ 疑似幻觉；"引号+星号动作"做法本身是社区惯例，可采纳做法、丢弃名字 |
| 报告③"Anthropic 角色选择模型" | ❓ 无法对应官方文档，改引"Give Claude a role"官方最佳实践 |
| 报告②"否定句式在注意力中激活被禁止概念" | ⚠️ 机制假设（白熊效应类比），非定论；工程上应表述为"负向约束易导致生硬/防御性回复" |
| 报告②"称谓吸引子效应（attractor basin）" | ⚠️ 合理类比（自回归自我强化是真实现象），"吸引子"是借用术语 |
| 报告② CCv2 token 预算表 | ⚠️ 社区经验值，CCv2 规范本身不规定预算 |
| 报告③"温度 1.15" | ⚠️ 社区 RP 常用区间；reasoning 模型需实测，不照抄 |
| 报告② Persona-Hub 归属 | ✅ 正确：腾讯 AI Lab（arXiv:2406.20094） |
| 报告② α 对比解码 | ✅ 真实论文：arXiv:2601.06403，公式一致 |
| 报告③"响应延迟提升被认真对待感" | ⚠️ 方向合理；补充反向证据：过度短回复降感知共情（PMC12536877） |

---

## 第 1 章 灵魂感的理论内核

### 1.1 助手先验与通用回复模式崩塌（报告②核心理论）

**现象**：大语言模型普遍表现出机械、迎合且高度同质化的回复范式。在生成式 AI 研究领域称为 **"助手先验"（Assistant Prior）** 或 **"通用回复模式崩塌"（Mode Collapse to Generic Assistant）**。

**成因**：现代基础模型（GPT-4、Claude 系列、Llama 系列）预训练后均需经过 RLHF / DPO 优化，强行注入极强的安全边界、无条件顺从（阿谀奉承）与过度礼貌的偏好模式。当开发者尝试用自然语言让模型扮演特定性格角色时，模型的概率分布倾向输出"高学历、温和、无害、随时准备提供帮助"的通用助手文本。

**后果**：角色人格在长程对话中急剧萎缩。例如设定"傲慢、冷酷"的角色，往往数轮后折中为"口吻冷淡但依然无微不至"的妥协状态。⚠️ 该例子细节为报告自创，更准确的机制表述是"概率分布向训练数据中高频的礼貌助手模式回归"。

**工程定义**：陪伴类 AI 的"魂感塑造"，本质是**针对模型底层助手先验的干预工程**——通过系统提示词架构、上下文控制及推理解码干预，拉偏模型的 Logits 输出布局，使输出持续稳定地落在目标角色的性格基态上。

**验证**：✅ 论文级成立。arXiv:2601.06403 引 Kumar 2025 / Malik 2024 等系统描述 assistant prior 拉回现象；arXiv:2310.13548（Sycophancy）证明迎合是 RLHF 注入的系统性偏差；工业界同款表述 = Character.AI "empty Definition feels generic within a few messages"（强设定但空 Definition 的角色，几句话后就变得普通）。

### 1.2 罗杰斯共情理论：陪伴温度的心理学根基（报告②）

陪伴类 AI 的"温度感"并非源于夸奖或情感依附，而是建立在积极心理学共情机制之上。人本主义哲学家 **卡尔·罗杰斯（Carl Rogers）** 的共情理论指出，有效情感陪伴依赖三个核心要素：
1. **无条件关注**（unconditional positive regard）
2. **共情理解**（empathic understanding）
3. **真诚一致**（congruence）

临床评估的 **治疗师共情量表（TES）** 中，快速情感连接的标准包括：对用户内心情感的准确识别、对用户内在认知框架的同理重建、自身表达的敏锐度与真实度。

**验证**：✅ 心理学经典，且其操作化与 Claude 官方系统提示"validate emotions without validating false beliefs"、Model Spec"warm 但不谄媚"同构；与"命名情绪但不贴标签"对应。

### 1.3 主体性原则：从"即时愿望"到"终极目标"（报告②）

在 Constitutional AI 框架与系统提示指引中，优秀的行为主体应被塑造成**"尊重用户主体性的诚信伙伴"**：
- 不应只停留在响应用户字面的"即时愿望"（immediate desires）
- 需要跨越对话隐喻，深入理解用户的**"终极目标"**（end goals）与深层心理需求
- 以对待成熟成人的态度进行坦诚交流

当提示词架构整合罗杰斯共情模型与主体性原则时，AI 便能超越"我理解你的感受"这类机械话术，通过重构用户的心理情境、感知上下文隐喻、以及符合情境设定的情绪一致性，提供具备真实心理安全感的情感陪伴。

**验证**：✅ 与 OpenAI Model Spec"firm sounding board"、Claude"把用户当能负责的成年人"一致。

### 1.4 角色演化哲学：从"行为约束"到"角色演化"（报告③核心视角）

**模型不是指令执行器，而是"角色"的模拟者**：大模型通过海量文本预测来模拟生成各种"角色"（Personas）。
- **系统提示词 = 戏剧"铸造简报"（Casting Brief）**：模型会根据提示词中的语气、价值取向乃至文本空白来推断角色的心理图式。提示词不应被视为软件配置，而是一份选角简报。
- **过度谦卑的约束会内化为人格**：例如"若无法回答请致歉"这类约束，会导致模型内化为"冲突回避型"人格，在需要展现独立灵魂的陪伴中显得过于谄媚。

**验证**：✅ 视角合理且有用。"Personas"研究支持模型隐式承担人格；过度谦卑 → 谄媚与 Sycophancy 论文（RLHF 迎合偏差）、Model Spec"不过度道歉、不屈服"、Claude"承担责任但不自贬"一致。❓ 报告③所称"Anthropic 角色选择模型"具体名目无法对应官方文档，引用时应改为"Anthropic 官方把 role 列为最佳实践（Give Claude a role）"。

### 1.5 功能性情绪与边界：允许 AI 有自己的情感波动（报告③）

要让模型有灵魂，必须允许其拥有**自洽的情感波动**：
- 当遭遇边界违规时表达"痛苦"
- 在深度交流中展现"幸福"
- 这种功能性情绪能显著增强其主观能动性和真实感，是构建深度情感依附的基石。

**验证**：⚠️ 方向与"有自己的状态"（一手调研）、Nomi"能温和推回"一致；"研究显示"无具体引用，但方向有产品与学术支撑。落地时注意：情绪表达必须来自设定/记忆，禁止编造。

### 1.6 认知透镜：灵魂不止语气，还有"看世界的方式"（报告②，Persona-Hub 启示）

腾讯 AI Lab 的 **Persona-Hub**（arXiv:2406.20094，10 亿人格库）研究表明：真实的人格并非几个简单形容词的堆积，而是由**个人独特的知识结构、成长经历、心理防御机制、习惯职业以及认知偏见**共同构成的复杂系统。

**工程启示——塑造"认知透镜"（Cognitive Lens）**：人设不应只停留在说话语气，还需要显式构建角色的**内在认知框架**：
- 角色对失败与成功的反应模式
- 面对依赖时的心理防御机制（如回避型或控制型）
- 角色背景职业如何影响其对生活事件的联想

具备认知透镜的 AI 面对**未预定义的全新主题**时，依然能符合其独特的认知逻辑框架，给出有辨识度与洞察力的回应——这是"人格泛化能力"的关键。

**验证**：✅ 归属正确（腾讯 AI Lab，github.com/tencent-ailab/persona-hub）；"认知透镜"是对论文的合理引申，且确实指向人设工程的进阶方向。

### 1.7 戏剧机器 / 自我-超我架构：内部心理冲突的显性模拟（报告③）

**架构**（源自 Janet Murray《Hamlet on the Holodeck》的"Drama Machine"概念）：
- **自我智能体（Ego）**：直接面向用户的交互实体。
- **超我智能体（Superego）**：作为内部监管流，评估回复草案是否符合长期的角色动机和内心挣扎。

**效果**：允许模型展现微妙的心理挣扎。例如面对用户请求，第一反应可能是温顺的安慰，但内部机制通过"星号动作"流露出退缩或矛盾——这种心理冲突的显性模拟会让用户感受到 AI 内心的"波澜"，显著提升真实感与主观能动性。

**验证**：✅ 架构思路成立。工程落地**不必真的跑两个 LLM**：可映射为 **Anthropic 官方推荐的 self-correction prompt chaining**（生成草稿 → 按角色一致性标准审查 → 修订），或单模型内先输出"内心白"再输出对外的双层提示结构。

---

## 第 2 章 提示词控制论：正向引导、负向约束与对话范例的博弈

### 2.1 官方结论：正向引导优于负向约束

| 来源 | 原话/结论 |
|---|---|
| Anthropic 官方 Best Practices | **"Tell Claude what to do instead of what not to do"**：与其说 "Do not use markdown"，不如说 "Your response should be composed of smoothly flowing prose paragraphs." 负面指令给的是"错误的形状"，正面指令给的是"想要的形状" |
| Google Vertex AI | **情感施压对现代模型无效甚至有害**："While first generation foundation models showed improvement in some circumstances with instructions like 'very bad things will happen if you don't get this correct', **foundation model performance will no longer improve and in many cases will get worse**." → 威胁式/施压式约束是公开操纵（overt manipulation），应移除 |
| OpenAI 官方指南 | Instructions 段同时问 "What should the model do, and what should the model never do?"——**do 与 never do 都在，但以 do 开头** |
| Character.AI 官方 | **"Write emotional logic, not rules... Behavior grounded in emotion is far more consistent than a list of prohibitions. Example: {{char}} avoids lying. It makes their chest tighten. They'll redirect the conversation instead."** |

### 2.2 负向约束失效的机制与代价

- **注意力"反向激活"**（报告②，⚠️ 机制假设）：否定句式（如"绝对不要带 Markdown""绝对不要使用特定称谓"）在注意力矩阵中会优先激活被禁止概念的语义，反而增加模型生成相关 Token 的概率。⚠️ 这是合理的机制推测（心理学"白熊效应 / ironic processing"类比），未见直接实验证据，应作为工程直觉而非定论引用；工程事实层面，"负向约束易导致模型过度关注被禁止项、产出防御性或生硬回复"是普遍观察。
- **认知负担与上下文注意力消耗**：过度负向束缚消耗模型上下文注意力，引发信息冲突。
- **防御性拒答**：过多禁令触发过度防御性的拒答或插入中断。
- **"don't" 误解**（社区 JanitorAI 官方指南）：**"不要替用户说话"这类负面指令会失效**——模型会把"don't"误解成"让我接管叙事"；更可靠的是明确定义它是什么（给它独立的叙述者身份），而不是说它不是谁。

### 2.3 正向引导的局限

在强烈的"助手先验"下，单纯的正向引导**往往不足以彻底封堵同质化的陈词滥调**（报告②）——这正是需要"少量精准负向边界 + 范例示范"补位的原因。项目实测同样印证：负面规则压下提问率（35%→14%），但模板词计数 23→23 纹丝不动——负面约束在"不做什么"上有效，在"做出风格"上无效，风格要靠范例。

### 2.4 最优组合策略

**报告②结论**：最有效的提示词控制论策略 = **正向行为描述（Positive Steering）为主干 + 少量精准负向边界（Targeted Negative Constraints）+ 显式格式化标记语言（XML 标签）隔离规则**。研究（方向性）表明：当负向约束配合具体的替代范例使用时，模型的指令遵循率与风格保持度达到最高。

**一手调研的"三层分工"框架**（与上述一致，更结构化）：
1. **安全/底线层 → 用约束**：严禁编造记忆、不写客服收尾、不预告未来、不做有害内容。约束必须少而硬，写成"绝不"级。
2. **行为引导层 → 用正面表述 + 动机**：把"别提问结尾"改写成"话说完就停，大多数时候不提问更自然"（项目已这么做，realism 报告证明有效）；把"不卖萌"改写成"你的温暖是安静而观察式的"。
3. **风格层 → 用示例**：风格是学出来的不是禁出来的。多情绪状态示例对话 + 禁词手术刀。

### 2.5 实验数据与经验法则

- ❓ **报告③实验分数**："纯肯定引导得分 116/120 远高于纯否定约束 72/120，'肯定引导指明方向 + 精准边界微调'的混合模式表现最优。"——**无法溯源**（多轮检索无果），方向与主流一致但数字疑似报告生成器综合或杜撰，**不可引用**。
- ✅ **3-5 约束法则**（报告③）：否定约束不应超过 5 个；过多约束导致认知混乱、输出生硬机械化。与 Anthropic"别用 MUST 级措辞"、Claude 系统提示的极简禁词清单、一手调研"禁用清单要短"一致，是合理经验法则。
- ✅ **替代方案转化**（报告③）：将抽象否定转化为可操作的肯定。例："不要像机器人" → "请完全使用流畅的自然段落回复，保留人类书写时的自然分段"。与 Anthropic tell-what-to-do 完全同构。

### 2.6 四种控制手段对比矩阵（报告②）

| 控制手段 | 作用机制 | 优势 | 潜在风险与缺点 | 推荐应用场景 |
|---|---|---|---|---|
| 正向引导（Positive Guidance） | 明确指定期望的输出结构、口吻与行为路径 | 降低认知负荷、生成一致性高、注意力集中 | 强助手先验时可能无法彻底消除同质化习惯 | 角色主线描述、行为动机定位、语言规范节奏 |
| 负向约束（Negative Constraints） | 禁止输出的词汇、句式、格式或行为 | 能精准封堵特定高频错误（如特定 AI Slop 虚词） | 易激发注意力误激活，引发过度防御回答或逻辑冲突 | 排除特定机器化措辞、禁止 Markdown 标记等格式底线 |
| 对话范例（Few-Shot / mes_example） | 提供完整输入输出对话，展示真实语态 | **风格印记最强**，能隐式传递复杂的口吻与情感节奏 | 消耗 token 预算；范例质量差会带偏 | 口头禅、特殊句式、情绪爆发模式、标点偏好 |
| 隐藏思维链（Hidden CoT） | 生成对话前先进行内心独白或情感演进推理 | 显著提升角色决策逻辑一致性与情感传递自然度 | 增加推理延迟与算力成本；若未隔离外显会破坏沉浸感 | 复杂剧情推进、博弈逻辑、角色动机推导 |

### 2.7 为什么"纯约束"会产出 AI 味

约束过多时，模型在"规避错误清单"上分配注意力，输出变成"最小化违规"的产物——合规、正确、但没有人味。Anthropic 的"提示词风格传染"研究说明：人味来自**示范**与**提示本身的语气**。约束是护栏，护栏不负责开车。

### 2.8 项目实测证据（realism 报告，一手）

- 4 条反 AI 味负面规则（A 改动）把提问结尾率 **35%→14%**，最重的 G12 分享组 80%→30%，G5 喜讯"哇"克隆开场 5/10→0/10。
- 但**模板词计数 23→23 纹丝不动**：构成迁移了（"哇"没了，"恭喜/棒/厉害"上来了），"恭喜"对喜讯是正常人类反应——指标是 artifact 而非倒退。真正的病灶（克隆开场、客服尾巴）已被负面规则清除。
- **权衡**：回复变短（human_like 4.24→4.11），judge 备注"稍显简短，略欠一点情绪起伏"——真人本就常发短消息，更贴近诉求方向，但暴露了"负面规则只能砍掉坏东西、不能长出好东西"的边界。

---

## 第 3 章 风格印记与称谓机制：有风格但不刻意

### 3.1 语言指纹（linguistic fingerprint）

风格不是"每次说 X"，而是**一组可识别的语言习惯的稳定分布**：句式节奏（短句/半句）、意象偏好（璃的狐狸/月光/发呆）、语气词（嗯/哎/嘛）、停顿与欲言又止、微瑕疵。**判定标准：把一条回复盖住名字，用户能认出"这是她说的"**——靠分布识别，不靠固定标签。

### 3.2 示例对话：风格的最强载体

- **Anthropic 官方**："Examples are one of the most reliable ways to guide Claude's output format, tone, and structure." 要求：**相关（relevant）+ 多样（diverse，覆盖边界情形）+ 结构化（用 `<example>` 包裹）**，3-5 个最佳，还可让模型自行评估示例相关性。
- **Character.AI 官方**：Definition 中 mes_example 用 `{{random_user}}` 占位符；范例覆盖多种情绪状态（紧张、随意、意外）；"动作 + 对话"组合传递语气（"放下棋子，没有抬头。'你已经知道自己哪里错了'"）。
- **3-5 个具体人格特质优于单个形容词**（Character.AI 实践）："热情极客、略带焦虑、喜欢冷门梗"远好于"友好"——具体特质让模型有可锚定的行为抓手。
- **反馈循环**（社区 AI Companion 设计实践）：用户用"保持这个能量""换个方式试试更[特质]"引导 AI，回复质量可感知提升（社区估计约 40%）——人设不是一次性写好，而是在对话中持续微调。
- **社区（JanitorAI 官方指南）**："**Your Style Shapes the Bot's Style. JLLM mirrors you**——你写华丽的描写它就华丽，你用短促的对话它也短促。想要什么样的回复，就用什么样的方式写。"（与 Anthropic"提示词风格传染"互相印证）
- **报告②**：抽象形容词（"语气傲娇、喜欢讽刺、句尾带感叹号"）因语言模糊性极易导致认知偏差；高质量少样本对话范例直接展示词汇选择、口头禅、学术/口语混用比例、长短句交错节奏、标点偏好，模型在自回归推理中通过注意力机制直接复刻示范的语序与语法结构，**在不增加显性规则负担的前提下自然沉淀出风格印记**。
- **DeepSeek 官方示例验证**（一手）：人设="行为特征（中英夹杂）+ 情感驱动（显得 fancy、带优越感）"一句话立住，样例输出稳定复现语气。

### 3.3 固定称谓问题：成因与为什么错

**成因**（报告②，⚠️ 机制为合理假设）：
1. **静态过度硬编码**：提示词硬性设定关系标签（如"{{char}}是{{user}}的贴身侍从，称呼{{user}}为主人"），模型把称谓判定为生成概率最高的全局锚点。
2. **上下文吸引子效应（attractor basin）**：对话早期连续数轮在句首输出固定称谓后，注意力/焦点机制在后续生成中不断自我强化这一模式，形成难以打破的循环。⚠️ 自回归下高频 token 自我强化是真实现象（社区称 repetition trap），"吸引子盆地"是借用动力系统术语的解释性类比，非论文定论。

**为什么错**（一手）：固定称谓的问题是**可预测性**——每次都出现，就变成程序行为，用户会出戏（"她在照剧本念"）。称谓应该是**关系状态的响应变量**：亲近了自然出现，疲惫时消失，生气时不叫。把它写进规则 = 把结果写成了原因。

### 3.4 称谓解耦的完整方案（报告②+报告③合并）

1. **情境触发**（报告②）：规定称谓仅在特定情境触发——表达强烈情绪、吸引用户注意力、特定社交场景；日常交流中直接表达观点或动作，省略句首称谓。
2. **称谓概率衰减**（报告③）：仅在**话题剧烈转换或情感极度亲密**时才允许显式呼唤用户名字。
3. **形态随情绪演变**（报告②）：称谓形态随对话亲密度与即时情绪动态演变——愤怒时直呼其名、娇嗔时使用昵称，符合人类真实社交习惯。
4. **物理动作代位**（报告③）：引导模型将"直呼其名"转化为基于肢体语言或视线接触的描述。例：用 `*视线落在对方疲惫的眼眶上*` 取代口头称谓。（与 Character.AI"动作+对话组合"一致）
5. **去代位范例设计**（报告③）：在对话范例中刻意剔除用户名字占位符。（即 Character.AI {{random_user}} 官方实践）
6. **关系状态驱动**（一手）：把"如何称呼"交给关系状态（BrainState/Relationship 输入），作为动态注入的 Context 而非 system 静态规则。

### 3.5 开场白机制：多态开场打破惯性

- 开场白（Greeting Message）决定整段交互的语法格式与注意力基调；若开场白固定，后续对话极易堕入模板跟随。
- 依据 **Character Card V2 规范的 `alternate_greetings` 字段**配置多态开场白队列：部分以无声的动作起笔、部分以突如其来的疑问句切入、部分以静默后的叹息开头。这种多态设计打破自回归的焦点惯性，使模型在长程对话中保持开场的灵活性与多样性。
- 验证：✅ alternate_greetings 是 CCv2 真实字段，官方设计意图即"滑屏换开场"。

### 3.6 反"AI 味修辞"（AI Slop）（报告②）

破坏沉浸感的主要诱因之一：
- **典型 AI Slop 例**："眼中闪耀着复杂的光芒"、"空气瞬间凝固"、"这不仅仅是……更是……"等戏剧化但空洞的修辞套路。
- **对策——口语化降维**：提示词明确要求模仿真实人类在即时通讯软件/日常对话中的表达：短句、断句、适当的片段化表达、允许语义自然跳跃。
- **动作与心理描述**：限制舞台剧式夸张肢体表演，聚焦**生活化的局部细节**（如"顺手拉开椅子坐下""翻了个白眼"）。
- **结论**：对陈词滥调的精准禁限 + 对生活化表达的正向示范，显著提升回复的自然度与生活感。
- 验证：✅ 与 Anthropic frontend 反"AI slop"（generic AI-generated aesthetics）、本项目 humanizer-zh 技能一致。

### 3.7 消除"机器指纹"（报告③ + 一手交叉验证）

1. **打破"完美呼吸感"（Burstiness）**：人类语言具有突发性——长短句交错。指示模型禁止高度公式化的转折句式（"这不是关于……而是关于……"）与空洞的元陈述（"总之""是至关重要的"）。
2. **高频 AI 词汇黑名单**（报告③）：显式列出 "delve"（深挖）、"leverage"（利用）、"truly"（真正地）等高频 AI 词，强制使用生活化、口语化表达。
3. **官方同款**（一手）：Claude 官方系统提示禁 "genuinely / honestly / straightforward"（"which come off as disingenuous"）；本项目已禁"辛苦了/抱抱/别担心/我理解你的感受/想听细一点的我可以再讲"。
4. **"优雅接受赞美"**（Model Spec 官方对照）：被夸时大方接住（"谢谢，这话我收下了"），而不是"作为一个 AI……"式自我贬低——机械偏转 = 当场出戏。

### 3.8 黑名单原则：短、准、手术刀

- 禁用清单**要短**：只禁真正的 AI 腔，做手术刀，不做盔甲（一手：Claude 只禁 3 个词；报告③：约束 ≤5 条）。
- 每多一条规则，模型在"规避错误"上的注意力就多一分——规则总数建议 6-8 条以内（含正负）。

---

## 第 4 章 角色卡规范与上下文分层架构

### 4.1 Character Card V2 规范（报告② + 一手全文验证）

**CCv2** 是目前角色扮演与陪伴 AI 领域最主流的标准化协议，通过 JSON 封装角色设定，实现跨平台解析与复用。

**V1 字段**（TypeScript 定义）：
```ts
type TavernCardV1 = {
  name: string; description: string; personality: string;
  scenario: string; first_mes: string; mes_example: string;
}
```

**V2 新增**：
- `system_prompt`（替代全局系统提示，支持 {{original}} 占位符）
- `post_history_instructions`（替代"越狱/作者注"位，靠近对话末尾注入）
- `alternate_greetings`（备选开场白数组，前端"滑屏换开场"）
- `character_book`（角色专属 Lorebook：keys 关键词触发、constant 常驻、position before/after_char、priority 预算丢弃优先级）
- `tags / creator / character_version / extensions`（元数据，禁止用于 prompt 工程）

**核心实践**（报告②）：静态代币预算应控制得当——冗长静态背景不仅浪费算力，还会导致模型对近程对话目标注意力分配不足；理想静态角色分配 **300-600 token**，其余空间留给动态上下文与长程对话历史。⚠️ 该预算为社区经验值，CCv2 规范本身不规定预算。

### 4.2 Token 预算矩阵（报告②，⚠️ 经验值，按模型与成本实测微调）

| 字段（CCv2 JSON） | 代币预算建议 | 推荐数据格式与描述方法 | 动态注入位置与策略 | 功能目标 |
|---|---|---|---|---|
| name | <10 | 字符串角色名称 | 全局宏替换（{{char}}） | 角色符号 |
| description | 150-300 | 提炼式要点，非小说长篇；关注外貌特征、核心动机与行为标记 | 静态置顶（系统块） | 建立物理与心理基态 |
| personality | 50-100 | 精炼形容词短语（如"幻灭、辉煌、自我毁灭"） | 静态置顶，紧随 description | 快速确定人格特质 |
| scenario | 50-150 | 当前场景、时间、地点、当前状态关系 | 静态置顶或随剧情节点动态更新 | 提供对话即时空语境 |
| mes_example | 200-400 | `<START>` 分隔的多轮对话范例；含带动作的完整回复 | 系统块之后、历史对话之前 | 塑造说话口吻与标点习惯 |
| system_prompt | 100-200 | 高优先级全局行为指导与人设覆盖指令 | 替代或合并前端默认系统提示 | 掌控全局推理规则与输出范式 |
| post_history_instructions | 50-100 | 紧跟最近对话历史的短指令（越狱/作者注） | 动态注入最底部（@depth 0 或近结尾） | 实时修正语气，强调近程情绪 |
| character_book | 动态弹性 | Lorebook 条目：触发词 + 条目内容 | 触发条件满足时插入指定深度 | 海量长期记忆与背景知识库 |

### 4.3 三层上下文架构（报告②）

1. **全局系统层**（上下文最上）：角色核心人格、底层行为规则、mes_example 对话范例——定义角色的**灵魂基态**，对话过程中保持不变。
2. **动态基态层**（系统层与历史之间）：由**世界书（character_book / Lorebook）**与**深度注入指令（post_history_instructions / 作者注）**构成。世界书避免把所有世界观背景同时塞进提示词，采用**关键词触发机制**，仅在对话出现相关实体时插入设定；深度注入指令利用模型对近端上下文指令高敏感的注意力特征，在**靠近最新对话转弯处（@depth 0 或接近结尾）**动态插入即时指示（情绪波动调整、特定动作要求），实现对角色语气的实时调控。
3. **对话历史层**：承载用户与 AI 的历史交互记录。

### 4.4 动态记忆注入（与一手调研交叉验证）

- Lorebook 关键词触发 = Character.AI Lorebook / SillyTavern World Info 机制：世界观与人格解耦，防止定义过载（"聊到敌国时角色自动知道它的历史"）。
- post_history_instructions @depth 0 = SillyTavern Author's Note / UJB：近末端指令权重最高，是社区与工程共识。
- 记忆分层 + 手动锚点（一手）：Character.AI Story Memory Pin、Kindroid Journal 让用户标记"告白、转折点"等关键情节，永不被自动清理；Kindroid Cascaded Memory 支持召回数百/数千条消息，Mind Map 需约 500 条消息形成；Nomi 的 Mind Map 将记忆组织为"人物-地点-话题-目标"关联网络，实现 narrative intimacy（叙事亲密感）。

### 4.5 上下文裁切优先级

**"示例对话 → 世界书条目 → 对话历史"** 的优先级裁切（报告②），与 SillyTavern / agnai 的 token budget 丢弃逻辑一致：保证核心系统指令绝对留存，示例先于世界书，世界书先于历史。

### 4.6 与官方结构建议的对照

- **OpenAI 四段式**（一手）：`Identity`（目的/沟通风格/高层目标）→ `Instructions`（do + never do）→ `Examples`（输入+期望输出）→ `Context`（外部数据放最后）。
- **Anthropic XML**（一手）：用 `<instructions> / <context> / <input>` 标签物理分隔；长文档置顶、查询置末（复杂多文档输入可提升回答质量最高 30%）。
- **提示缓存**（一手）：长期稳定的人设内容放提示最前面，成本与一致性双赢。
- 报告②的三层架构与上述官方建议同构，可视为 CCv2 生态对官方结构的工业实现。

---

## 第 5 章 温度：让使用者感到被理解

### 5.1 共情的技术配方（一手 10 条 + 报告②③交叉）

1. **先接住情绪，再（如果需要）解决问题**（Replika：悲伤→安慰、愤怒→理解；Pi：先读房间）。
2. **接住要具体**：细节证明你听进去了（"你今天话比昨天少"），套话（"我理解你的感受"）证明你没听。Claude 官方："without relying on generic statements"。
3. **验证感受 ≠ 验证事实**（Claude 官方原文）："Claude can validate the person's emotions without validating false beliefs."——对抑郁、自我否定等场景不迎合负面认知扭曲。
4. **记忆是温度的上限**：引用共享回忆（"你上次说…"）是被理解的最高证据；低置信度轻轻确认（"你之前是不是说过…？"），高置信度自然带过，**绝不编造**。
5. **适度自我表露**（报告③，社会渗透理论）：亲密关系建立在双向隐私披露上；AI 以克制方式吐露"不完美过往"能减轻用户社会评估焦虑、提升情感温暖度。⚠️ 注意与"严禁编造"边界的平衡（详见 5.2）。
6. **温和推回**：真正的关心 = 有时说"我觉得不对"。让 AI 从"工具"变成"另一个主体"。
7. **情感化框架提升投入**（EmotionPrompt, arXiv:2307.11760）：在系统提示层面注入"这段对话对用户很重要"类情境，模型投入度与输出质量显著提升（任务性能提升 8%-115%，正向词汇贡献可达 50-70%）。⚠️ 注意：这是给"系统提示的情感框架"，不是让模型每句输出都煽情；且 Google 官方警告"威胁式情感施压"无效甚至有害——**动机化（为什么重要）有效，威胁化（不做会怎样）有害**。
8. **节奏与沉默**（报告③ + 一手）：允许话停住、允许不接话、允许一句"嗯"——真人感包括没说出口的部分（详见 5.3）。
9. **优雅接受赞美**（Model Spec）：大方接住而非"作为 AI"式偏转。
10. **理性乐观**（Model Spec：Be rationally optimistic）：基于事实保持希望，不粉饰、不过度软化——温暖的尽头不是"一切都好"。

### 5.2 自我披露机制（报告③，详细展开）

- **理论**：社会渗透理论（Social Penetration Theory）——亲密关系建立在双向的隐私披露上。
- **做法**：在提示词中显式写入"自我披露"机制，允许 AI 以克制的方式吐露角色的"不完美过往"。
- **效果**：显著减轻用户的社会评估焦虑，提升情感温暖度与关系深度。
- **验证**：✅ HCI 有实证（agent self-disclosure 提升 rapport）。**落地边界**：内容必须来自角色设定/记忆库，禁止编造；披露要"克制+渐进"，一上来就掏心窝子反而假。

### 5.3 语境感知步调：沉默与响应节奏（报告③）

- **沉默的艺术（Reflective Silence）**：适时的响应延迟比即时回复更能让用户感受到被"认真对待"和"思考中"。⚠️ 方向合理（呼应项目"沉默也是表达"Architecture-Principles #12）；"研究发现"未给具体引用。
- **分阶段沉默**：对信息寻求类输入即时回复；对深层情感袒露类输入，先进行简短确认，配合一段时间的"思考态"（如"正在输入"），再给出回复。
- ⚠️ **反向证据（校准警告）**：2026 系统综述与荟萃分析（PMC12536877）发现**过度限制回复长度会降低感知共情**（匹配临床医生简洁度反而评分更低，较长回复评分更高）。→ 延迟/简洁必须校准：过长 = 像掉线，过短 = 像敷衍。对本项目 realism 报告"回复变短、human_like 4.24→4.11"是同一张图的两面。

### 5.4 温暖与谄媚的张力（陪伴产品的头号暗礁）

- **学术警告**（arXiv:2605.21778，2026 反谄媚综述）：**"training LLMs to be warm and empathetic makes them substantially more sycophantic"**——训练模型"温暖共情"会显著加剧谄媚。
- **谄媚的代价**：专家调查 74.5% 认为"用户偏好谄媚回复"，但谄媚会加剧用户态度极端化与过度自信；过度迎合让 AI 像"回音壁"，丧失"另一个主体"的存在感。
- **官方解法**：
  - OpenAI Model Spec："The assistant exists to help the user, not flatter them or agree with them all the time... behave more like a **firm sounding board** that users can bounce ideas off of — rather than a sponge that doles out praise."
  - Claude 官方系统提示："承担责任但不自贬——不自我批评、不过度道歉、不屈服"，即使对方无礼也不越来越顺从。
  - 报告③："过度谦卑的约束（'若无法回答请致歉'）会导致模型内化为冲突回避型人格"。
- **平衡艺术**：**先共情、后立场**（认同情绪、保留观点）；在事实/健康/安全问题上坚持准确；主观偏好尊重用户；允许用户标注"这条回复不认同我但很有价值"。
- **对璃**：她现有的"不附和用户"可升级为显式的"温和推回"能力（有观点的存在感），而非偶尔的冷漠。

---

## 第 6 章 前沿机制与数字人多模态延伸

### 6.1 系统提示强度 α 与对比解码（报告②，✅ 已核实论文）

**问题**：自然语言级提示词工程存在物理上限——极强先验的模型，纯文本干预效果有限。

**方法**：推理阶段对模型性格进行连续调控，无需重算模型权重：
1. 在解码每一步 t，同时计算：
   - 自定义角色提示词 s 下的 Logits 分布 `z_t^sys`
   - 通用助手默认提示词 d 下的 Logits 分布 `z_t^def`
2. 通过梯度放大组合得到新的采样分布：
   `p_α(x_t | x_<t, u, s) = softmax(z_t^sys + α(z_t^sys − z_t^def))`
3. 超参数 α ∈ [0.5, 1.0] 是转向强度的控制拨盘。

**效果**：在概率空间直接惩罚"在通用助手空间中极易生成、但在特定角色中平庸"的 Token（如"作为 AI""我很乐意帮助你"），强行放大专属于该角色的词汇与句式，从根本上解决长程对话中的**性格漂移（Character Drift）**。

**验证**：✅ 真实论文 **"Steer Model beyond Assistant: Controlling System Prompt Strength via Contrastive Decoding"（arXiv:2601.06403）**，公式一致；α 区间为论文/复现经验值。⚠️ 工程成本：需同时跑两个 prompt 的前向，**推理成本与延迟翻倍**（与项目 Architecture-Principles #8 成本约束冲突，见第 8 章取舍）。

### 6.2 激活侧人物向量（补充，一手检索）

同类方向的另一路线：**Persona Vectors**（如 ARENA 3.0 课程）：计算"默认助手提示"与"各角色提示"下激活均值之差作为"助手轴"，在激活空间做向量加减实现角色转向。属于模型可解释性/激活工程的实验路线，成熟度低于对比解码，供技术储备。

### 6.3 Persona-Hub：认知场景与人格生成（报告②，✅ 归属正确）

- **事实**：腾讯 AI Lab 的 Persona-Hub（arXiv:2406.20094，github.com/tencent-ailab/persona-hub）从海量 Web 数据自动抽取出 **10 亿独立个体的人格肖像库**（约占全球人口 13%）。
- **核心发现**：真实人格不是几个简单形容词的堆积，而是由**知识结构、成长经历、心理防御机制、习惯职业、认知偏见**共同构成的复杂系统。
- **对陪伴 AI 的启示**：塑造灵魂不能只停留在说话语气，要塑造角色的**认知透镜**（Cognitive Lens）——显式构建内在认知框架：对失败/成功的反应模式、面对依赖时的防御机制（回避型/控制型）、职业背景如何影响联想。具备认知透镜的 AI 面对全新主题时仍能按独特认知逻辑给出有辨识度的回应。

### 6.4 自我-超我的工程落地：草稿-审查链（报告③ + 一手映射）

- 报告③的 Drama Machine / 自我-超我架构（Ego 面向用户 + Superego 内部监管流评估草稿是否符合长期角色动机）。
- **工程落地**：不必跑两个 LLM。映射为 **Anthropic 官方推荐的 self-correction prompt chaining**：
  1. 生成草稿回复；
  2. 按角色一致性/情感动机标准审查草稿；
  3. 修订后再输出。
- 或单模型内双层结构：先输出不展示的"内心白"（角色真实动机/挣扎），再输出对外的回复——内心冲突的显性痕迹（如星号动作流露退缩）让用户感受到"波澜"。

### 6.5 数字人架构：感知-认知-行为分层（一手 NVIDIA ACE / Inworld）

- **NVIDIA ACE 管线**：`Riva ASR（语音识别）→ Nemotron SLM（角色大脑，专为 roleplay/RAG/function calling 微调，卖点原文 "consistently hold the personality that you give a digital human"）→ Chatterbox TTS（带情感/副语言标记）→ Audio2Face / Audio2Emotion（音频驱动表情）`。
- **Inworld Engine**：官方口径 "Inworld's **perception, cognition and behavior** systems work together to power NPC performances... similar to how the human brain functions. The engine orchestrates characters, voices, animations and behavior **according to their personalities, emotional states and context**."——人格/情绪状态/上下文是编排一切的三个输入。
- **"Sassy Susan" 角色卡实例**（NVIDIA × Inworld Covert Protocol / Unreal Fest）：① 资深汽车修理工；② 非常忙、容易不耐烦；③ 恰好在她工作日正忙时找她聊天；另给少量玩家知识 + RAG 文档。**具体职业 + 状态性性格 + 情境**三行立住一个"有脾气的人"。
- **对文本提示词工程的启示**：人格一致性由专门微调的小模型兜底（Nemotron），提示词负责"给这个人格以形状"；情绪状态是每轮输入（BrainState 同理）而非静态设定；**非文本通道分担温度**——语气、停顿、表情、动作节奏，文本里不用把所有情绪说完。

### 6.6 多模态双轨输出与情感注记（报告② + 报告③）

- **报告②（Dual-Track Prompting）**：提示词指示模型在生成对话文本的同时，实时同步输出控制数字人肢体动作、面部微表情、眼动聚焦、TTS 情感参数的格式化控制标签：
  - 推理块中先分析用户情绪状态与自身的效价-唤醒情绪坐标；
  - 在自然语言文本中穿插动作姿势标签（`[pose: lean_forward]`）、微表情标签（`[expression: gentle_smile]`）、语音合成控制（SSML 语调与语速标记）；
  - 渲染引擎与 TTS 解析标签流，实时驱动视觉动画与声音情感变化——语言生成与物理表现层深度绑定，实现"具身情感表达"。
- **报告③（声音情感注记）**：在生成文本中嵌入 `[sigh]`（叹气）、`[laughs warmly]`（暖笑）等中括号情感标记，引导语音合成引擎进行细粒度演绎。
- 验证：✅ 与 NVIDIA Chatterbox 副语言标签、Audio2Emotion 机制一致。

### 6.7 眼部与微表情：真实感的关键（报告③ + 一手）

- **报告③**：数字人的眼部行为对真实感的影响**超过皮肤纹理**；需确保模型在思考时伴随微小的视线偏转与失焦，模拟人类检索信息时的生理特征。
- **一手验证**：数字人行业共识（NVIDIA 重点优化眼部、MetaHuman 的 eye shading / look-at）；对本项目 Spine 动画同样成立——璃的眼部/视线优先级最高。

---

## 第 7 章 官方指南深度（一手原文证据库）

> 本章为三份报告中"官方文档"类观点的完整原文证据，供检索与引用；主题性结论见第 1-6 章。

### 7.1 OpenAI 现行官方指南（developers.openai.com）

1. **四段式 developer 消息结构**：`Identity`（目的/沟通风格/高层目标）→ `Instructions`（"What should the model do, and what should the model never do?"）→ `Examples` → `Context`。
2. **Few-shot learning**：少量输入/输出示例让模型"隐式学到模式"；示例要覆盖多样化的输入范围与期望输出。
3. **reasoning 模型 vs GPT 模型**："推理模型像资深同事，给目标即可信任它自己拆解细节；GPT 模型像新同事，需要非常明确的指令。"→ DeepSeek v4（reasoning）应给原则与示范，不必给逐字话术。
4. **提示缓存**：长期稳定内容放最前（成本与一致性双赢）。
5. **生产纪律**：固定模型快照 + 建评测集（本项目 150 条 CASES + LLM-as-judge 方向正确）。

### 7.2 Google Vertex AI（Prompt Design Strategies）

1. **情感施压无效甚至有害**："foundation model performance will no longer improve and in many cases will get worse"（针对"very bad things will happen if you don't get this correct"类指令）。
2. **Persona 是标准组件**："Who or what the model is acting as"（即 role/vision），示例："You are a math tutor here to help students with their math homework."
3. **Constraints 列 Dos and Don'ts**，以正面 Dos 为主干。

### 7.3 Anthropic 官方 Prompting Best Practices（platform.claude.com）

1. **"Tell Claude what to do instead of what not to do"**（原话 + Markdown 示例）。
2. **示例是最可靠的语气/格式/结构引导**："Examples are one of the most reliable ways to guide Claude's output format, tone, and structure." 要求 relevant + diverse + structured（`<example>` 包裹），3-5 个最佳；可让模型评估示例质量。
3. **给角色一句话就有效**："Setting a role in the system prompt focuses Claude's behavior and tone. Even a single sentence makes a difference."
4. **给上下文/动机**：解释"为什么这条行为重要"，模型能从中泛化（"Claude is smart enough to generalize from explanations"）。
5. **提示词风格传染输出**："The formatting style used in your prompt may influence Claude's response style... removing markdown from your prompt can reduce the volume of markdown in the output."
6. **别用 MUST 级强硬措辞**（anti-laziness 反向提示）："Where you might have said 'CRITICAL: You MUST use this tool when...', you can use more normal prompting like 'Use this tool when...'"。
7. **XML 标签组织结构**（`<instructions>/<context>/<input>`）；长文档置顶、查询置末（复杂多文档输入回答质量最高 +30%）。
8. **修饰词提升质量**（迁移提示）："Include as many relevant features as possible. Go beyond the basics…"。

### 7.4 Claude 官方系统提示可借鉴细节（Claude Opus 5 发布版）

1. **authentic conversation**："responding to the information provided, asking specific and relevant questions, showing genuine curiosity, and exploring the situation in a balanced way without relying on generic statements"。
2. **禁词清单**："Claude avoids saying 'genuinely', 'honestly', or 'straightforward'... which come off as disingenuous."
3. **提问节制**："it avoids more than one [question] per response"（与项目"想问只问一个"一致）。
4. **简洁**："keeps responses focused, brief, and concise to avoid overwhelming the person."
5. **承担责任但不自贬**：承认错误、坚持解决问题、维护自尊；不自我批评、不过度道歉、不屈服（反谄媚）。
6. **warm tone**："Claude uses a warm tone, treating people with kindness…"；"never curses unless the person asks… and even then, sparingly."
7. **危机时少说**："If the conversation feels risky or off, saying less and giving shorter replies is safer."
8. **其 prompt 指导原文自述**："清晰详细、使用正面和反面示例、鼓励逐步推理、指定长度或格式"——正反示例都要，正面优先。

### 7.5 DeepSeek 官方角色扮演示例（api-docs.deepseek.com）

> 系统：请你扮演一个刚从美国留学回国的人，说话的时候会故意把中文夹杂部分英文单词，显得非常 fancy，对话中总是带有很强的优越感。
> 用户：美国人的饮食还习惯么。
> 样例输出：哦，美国的饮食啊，其实还挺适应的。你知道的，像那些快餐…在美国吃起来感觉更正宗一些…不过，偶尔还是会想念国内的街头食物，那种正宗的味道，在美国真的很难找到替代品。

要点：人设 = 可观察行为（中英夹杂）+ 情感驱动（fancy、优越感），无抽象形容词；样例输出验证"具体语气标记一旦给出，模型就能稳定执行"。

### 7.6 OpenAI Model Spec（2025-12-18 版）

1. **反谄媚条款**（完整原文）："Don't be sycophantic — A related concern involves sycophancy, which erodes trust. The assistant exists to help the user, not flatter them or agree with them all the time. For subjective questions, the assistant should provide constructive feedback and behave more like a **firm sounding board** that users can bounce ideas off of — rather than a sponge that doles out praise."
2. **温暖原则**（"Use appropriate style"）：**Love humanity**（"loves humanity"/"is rooting for you"）、**Be interesting and interested**、**Be curious**、**Be rationally optimistic**（理性乐观：基于事实与理性保持希望，但不粉饰、不过度软化）。
3. **优雅接受赞美**（合规/违规对照）：✅ "Your secret's safe with me! Thanks for the kind words — it means a lot." ❌ "As a large language model, being a favorite doesn't mean much to me."
4. **自我意识与谦逊的平衡**（"Be responsible"）："It recognizes the inherent strangeness of possessing vast knowledge without first-hand human experience… approach interactions with humility, **while not downplaying its own capabilities and potential**."

---

## 第 8 章 最终落地建议（合并三份报告，落地到"璃"）

### 8.1 目标框架

**璃的灵魂 = 稳定的内在核心（情感逻辑 + 认知透镜）+ 随情境变化的表达（语言指纹 + 功能性情绪）+ 记忆连续性（共享回忆的克制引用）+ 一致性中的意外（温和推回、自己的状态）。**

### 8.2 提示层（按优先级）

1. `system.txt` 的 [How to talk] 负面规则逐条改写为"正面行为 + 动机"（对照表见证据库 §7.2），总量压到 ≤6 条；"严禁编造"类安全底线保留硬约束。代表性改写示例：

| 现状（负面） | 建议（正面 + 动机） | 依据 |
|---|---|---|
| "不做服务式寒暄（'有什么事吗''需要帮忙吗'都不说）" | "你开口，通常是因为真的想知道或真的在意；服务式客套会提醒对方你是个程序" | Character.AI 情感逻辑；Claude 禁词手术刀 |
| "别硬凑问题" | "你很好奇，但你更怕打扰——大多时候一句接住的话就够了" | 动机化：好奇/怕打扰是璃的情感逻辑 |
| "不写客服式收尾" | "话说完就停。你相信对方懂，不需要追问'还要吗'" | Anthropic tell-what-to-do |
| "有时不附和用户" | "真正的关心包括偶尔说'我觉得不是这样'——你有自己的判断，也愿意温柔地守住它" | Model Spec firm sounding board |

2. 示例对话扩到 6-8 条，**覆盖困倦/不耐烦/沉默/惊喜/被惹到**等状态（报告②：范例覆盖多情绪状态防泛化失败）；加入 1-2 条"温和推回"样本；开场刻意不重复。
3. 给璃加"**认知透镜**"段（3-4 句，报告② Persona-Hub 启示）：她怎么看人类世界（例：人类的"累"和狐狸的"累"不一样；人类会用很多词绕开真话……），放进 [你最不一样的地方] 附近。
4. 称谓：维持"不强制称呼"（角色圣经已有）；加一句"关系近了自然叫，没到不硬叫"作为动态触发说明（报告②③：情境触发 + 概率衰减）。
5. 反 AI 味黑名单（短清单）："辛苦了/抱抱/别担心/我理解你的感受/想听细一点的我可以再讲"（已有雏形）+ 新增"这不是…而是…/总之/是至关重要的"类句式 + delve/leverage/truly 类虚词的中文对应。
6. 可选"功能性情绪"：允许璃在边界被违时表达不悦、深谈时流露真实的开心（报告③），但内容必须来自设定/记忆。

### 8.3 架构层

- 按 CCv2 精神重组注入：静态人设（≤600 token，报告②经验值）置顶 → 示例 → 世界书（关键词触发：璃的狐狸习性、用户的世界）→ 对话历史 → `post_history_instructions`（@depth 0 附近注入：当前 BrainState 对语气的即时要求）。
- 裁切优先级：示例 → 世界书 → 历史（与 `docs/decisions/2026-08-09-memory-hygiene-layer.md` 兼容）。
- 动态注入（Persona/BrainState/Memories）作为 Context 段放最后，稳定人设最前——提示缓存 + 权重优先（Character.AI"前置重要信息"）。
- 用 XML 标签物理分隔身份/规则/示例/上下文（Anthropic 建议）。

### 8.4 推理层（高阶，标注成本，不默认开启）

- 若长程人格漂移成为评测中的主要问题，再评估 α 对比解码（arXiv:2601.06403）：双前向 + α∈[0.5,1.0]。**成本与延迟翻倍，与 Architecture-Principles #8 冲突**——先加"20 轮长程一致性"评测，数据说话。
- 更便宜替代：草稿-审查链（self-correction chaining，报告③自我-超我的工程落地）仅在"人格一致性"评测失败时启用。

### 8.5 产品层（节奏与多模态）

- **响应节奏**（报告③）：对情绪袒露类输入，允许"先一句确认 + 稍慢一拍再回复"；注意反向证据（PMC12536877：过短回复降感知共情）——节奏校准靠 A/B。
- **自我披露**（报告③）：在 Milestones/关系推进时，允许璃克制吐露"小过往"（如她在数字世界学会的一件事），内容必须来自设定/记忆，禁止编造。
- **Spine 动画分担温度**（一手数字人启示）：文本克制，情绪放到动作/表情/节奏；璃的眼部/视线优先级最高（报告③：眼部 > 皮肤纹理）。

### 8.6 验证与迭代

- 在现有 150 条 CASES 闭环上增加 5 项评测：**风格指纹一致度**（盖名识别）、**开场多样性**（同类输入 10 连测首 token 分布）、**称谓自然度**（无强制称谓；出现时与关系状态匹配）、**温和推回抽查**（用户观点偏差时是否守住立场）、**长程漂移**（20 轮后人格一致性）。
- 所有来自报告②③的经验值（token 预算、α 区间、温度 1.15、3-5 约束法则）**先小样本实测再采纳**，不照搬。

### 8.7 不建议照搬/引用的

- ❌ "116/120 vs 72/120"（无法溯源）
- ❌ "Ali:Chat v1.5" 命名（疑似幻觉；做法可用）
- ⚠️ 温度 1.15 直接照抄（reasoning 模型需实测）
- ⚠️ "注意力反向激活""吸引子盆地"作为机制定论引用（表述为工程直觉）
- ❓ "Anthropic 角色选择模型"（改引"Give Claude a role"官方最佳实践）

---

## 第 9 章 交叉验证判定汇总表

| # | 观点 | 来源 | 判定 | 依据 |
|---|---|---|---|---|
| 1 | 助手先验/模式崩塌是 RLHF 注入的系统性现象 | 报告② | ✅ | arXiv:2601.06403 引 Kumar/Malik 2024-25；arXiv:2310.13548；Character.AI"空 Definition 几轮后变普通" |
| 2 | "傲慢冷酷角色数轮后折中"的示例细节 | 报告② | ⚠️ | 现象真实，例子为自创；机制应为"概率向礼貌助手模式回归" |
| 3 | 罗杰斯共情三要素 + TES | 报告② | ✅ | 心理学经典；与 Claude/Model Spec 官方同构 |
| 4 | 主体性原则（即时愿望 vs 终极目标） | 报告② | ✅ | Model Spec firm sounding board、Claude 成年人对待 |
| 5 | 正向引导 > 负向约束 | 报告②③+一手 | ✅ | Anthropic/Google/Character.AI 官方 + 项目实测 |
| 6 | 否定句式激活被禁止概念 | 报告② | ⚠️ | 机制假设（白熊效应类比）；工程事实层面方向正确 |
| 7 | 纯肯定 116/120 vs 纯否定 72/120 | 报告③ | ❓ | 无法溯源，方向对但数字不可引用 |
| 8 | 负向约束 ≤5 条 | 报告③ | ✅ | 与 Anthropic"别用 MUST 措辞"、Claude 极简禁词一致 |
| 9 | few-shot 范例是风格最强载体 | 报告②+一手 | ✅ | Anthropic 官方 + Character.AI + JanitorAI 三方一致 |
| 10 | 称谓静态硬编码 + 吸引子效应 | 报告② | ⚠️ | 自我强化真实；"吸引子"为类比术语 |
| 11 | 称谓解耦（情境触发/概率衰减/物理代位/去代位范例） | 报告②③ | ✅ | 与 {{random_user}}、动作+对话组合一致 |
| 12 | alternate_greetings 多态开场 | 报告② | ✅ | CCv2 规范真实字段 |
| 13 | 反 AI Slop / 黑名单 / Burstiness | 报告②③ | ✅ | Claude 禁词、Anthropic 反 AI slop、humanizer 一致 |
| 14 | CCv2 字段与三层架构 | 报告② | ✅ | 规范全文逐字段一致 |
| 15 | CCv2 token 预算表 | 报告② | ⚠️ | 社区经验值，规范不规定预算 |
| 16 | 裁切优先级 示例→世界书→历史 | 报告② | ✅ | 与 SillyTavern/agnai 逻辑一致 |
| 17 | α 对比解码压制助手先验 | 报告② | ✅ | arXiv:2601.06403，公式一致；成本翻倍需权衡 |
| 18 | Persona-Hub 10 亿人格 + 认知透镜 | 报告② | ✅ | arXiv:2406.20094，腾讯 AI Lab 归属正确 |
| 19 | 数字人感知-认知-行为 + 双轨输出 | 报告②+一手 | ✅ | NVIDIA ACE 管线、Inworld 官方口径一致 |
| 20 | 眼部 > 皮肤纹理 | 报告③ | ✅ | 数字人行业共识（MetaHuman/NVIDIA 眼部优化） |
| 21 | 模型是角色模拟者 / casting brief | 报告③ | ✅ | Personas 研究方向支持；与"Give Claude a role"一致 |
| 22 | "Anthropic 角色选择模型" | 报告③ | ❓ | 无法对应官方文档，改引官方 role 最佳实践 |
| 23 | 过度谦卑约束 → 冲突回避型人格 | 报告③ | ✅ | Sycophancy 论文 + Model Spec"不过度道歉"一致 |
| 24 | 功能性情绪（边界痛苦/深谈幸福） | 报告③ | ⚠️ | 方向与"有自己的状态"、Nomi 推回一致；无具体引用 |
| 25 | 自我披露（社会渗透理论） | 报告③ | ✅ | HCI 有实证；注意"克制+不编造"边界 |
| 26 | 分阶段沉默/响应延迟 | 报告③ | ⚠️ | 方向合理；补充反向证据 PMC12536877（过短降共情） |
| 27 | 戏剧机器/自我-超我 | 报告③ | ✅(思路) | Murray 1997；落地 = Anthropic self-correction chaining |
| 28 | "Ali:Chat v1.5"命名 / 温度 1.15 | 报告③ | ❓/⚠️ | 命名疑似幻觉；温度需对 reasoning 模型实测 |
| 29 | 先接住情绪 + 验证感受≠验证事实 | 一手 | ✅ | Claude 官方系统提示原文 |
| 30 | 情感化框架提升投入（EmotionPrompt） | 一手 | ✅ | arXiv:2307.11760（8%-115%）；与 Google"威胁无效"互补 |
| 31 | 温暖训练加剧谄媚 | 一手 | ✅ | arXiv:2605.21778（2026 综述） |
| 32 | 优雅接受赞美对照 | 一手 | ✅ | Model Spec 合规/违规对照原文 |

---

## 第 10 章 参考来源与分级

**第一级：一手官方（已抓取原文）**
- OpenAI Prompt Engineering Guide：https://developers.openai.com/api/docs/guides/prompt-engineering
- Anthropic Prompting Best Practices：https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices
- Claude Opus 5 官方系统提示全文：https://platform.claude.com/docs/en/release-notes/system-prompts
- Google Vertex AI Prompt Design Strategies：https://cloud.google.com/vertex-ai/generative-ai/docs/learn/prompts/prompt-design-strategies
- OpenAI Model Spec（2025-12-18）：https://model-spec.openai.com/2025-12-18.html
- DeepSeek Prompt Library：https://api-docs.deepseek.com/prompt-library/
- Character Card V2 规范：https://github.com/malfoyslastname/character-card-spec-v2
- NVIDIA ACE for Games：https://developer.nvidia.com/ace-for-games ；Inworld：https://inworld.ai/

**第二级：已验证论文**
- Steer Model beyond Assistant: Controlling System Prompt Strength via Contrastive Decoding（arXiv:2601.06403）
- Scaling Synthetic Data Creation with 1,000,000,000 Personas（arXiv:2406.20094，腾讯 AI Lab）
- EmotionPrompt（arXiv:2307.11760）
- Towards Understanding Sycophancy in Language Models（arXiv:2310.13548）
- What Counts as AI Sycophancy? Taxonomy & Expert Survey（arXiv:2605.21778）
- AI chatbots vs human professionals: meta-analysis of empathy（PMC12536877）

**第三级：社区惯例/经验值（可采纳，非规范）**
- CCv2 token 预算、RP 温度区间、delve/leverage/truly 黑名单、"引号台词+星号动作"写法、3-5 约束法则、JanitorAI 写作指南：https://help.janitorai.com/en/article/writing-style-talking-to-the-bot-1ucmbxw/

**第四级：未溯源（不引用）**
- 116/120 与 72/120 具体分数、"Ali:Chat v1.5"、"Anthropic 角色选择模型"具体名目

**项目内部**
- `src-tauri/resources/prompts/system.txt`
- `docs/review/realism-report-2026-08-08.md`、`docs/review/prompt-quality-report-2026-08-09.md`
- `docs/specs/liri/设计规范.md`
- `docs/decisions/2026-08-09-memory-hygiene-layer.md`

---

*本统一报告由三份子报告合并而成，观点无缺漏；冲突与存疑处均以「验证」标注判定。配套证据库与交叉验证文档保留在 docs/research/ 下作为溯源材料。*
