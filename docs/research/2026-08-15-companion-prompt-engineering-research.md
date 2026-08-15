# 角色扮演 / 陪伴类 AI 提示词工程深度调研

> 调研日期：2026-08-15 ｜ 调研对象：让陪伴 AI"有灵魂、像人、有温度、有风格但不刻意"
> 调研方法：一手抓取 OpenAI / Anthropic（Claude）/ DeepSeek 官方文档与 Claude 官方系统提示全文；Firecrawl 深度研究代理×4（大厂官方 / 陪伴产品拆解 / 社区与 GitHub / 数字人+学术）；Firecrawl 检索补充（反谄媚学术综述、NVIDIA ACE/Inworld）。
> 落地对象：本项目桌宠"璃(Liri)"（DeepSeek v4-flash + BrainState + 记忆 + Spine），已对照 `src-tauri/resources/prompts/system.txt` 与 `docs/review/realism-report-2026-08-08.md`。

---

## 0. 核心结论（TL;DR）

**Q1：如何让 AI 通过对话"有自己的灵魂、更像人"？**
灵魂 ≠ 形容词清单。灵魂 = **稳定的内在核心 + 随情境变化的表达**。具体拆成四块：
1. **情感逻辑，不是规则**（Character.AI 官方原话）：把"不撒谎"写成"撒谎让他胸口发紧，他会转移话题"。行为根植于情绪，才稳定、才像人。
2. **行为纹理，不是标签**：写"用问题回答问题"而不是"神秘"；写"今天话比昨天少"而不是"你看起来很累"。
3. **记忆的连续性**：能引用共享回忆（"你上次说…"）是"灵魂感"的最大来源（Nomi 称之为 narrative intimacy）；但只围绕记忆原意，严禁编造。
4. **一致性中的意外**：核心特质稳定（知道她怎么反应），但表达可以偶尔出人意料——有自己的状态、会犯小错、会温和地不同意。

**Q2：如何留下风格印记，又不每次带固定称谓显得刻意？**
风格印记靠**语言指纹**（linguistic fingerprint）而不是固定称谓/模板句：句式节奏、意象偏好、口头禅式语气词、停顿方式、微瑕疵（"嗯""哎"、半句话、欲言又止）。
- 称谓应当是**关系的产物**，不是规则：关系近了自然叫，关系没到硬叫就是表演。固定称谓的最大问题是"可预测"——人一旦可被完全预测，就没有"在场感"。
- 最强的手段是**示例对话**（few-shot）：3-5 条覆盖不同情绪状态的对话样本，比任何抽象规则都更能塑造语气（Anthropic 官方：示例是引导语气/格式/结构最可靠的方法）。
- 用 `{{random_user}}` 类占位符防止模型把示例里的称呼/名字记成固定模式（Character.AI 官方实践）。

**Q3：提示词是"约束不要做什么"还是"引导要做什么 + 模板"？**
**答案是：引导为主，约束为辅，示例为王。** 这是四路证据共同指向的结论：
- Anthropic 官方最佳实践原话：**"Tell Claude what to do instead of what not to do"**（告诉它要做什么，而不是不要做什么）。示例："Do not use markdown" → 应改为 "Your response should be composed of smoothly flowing prose paragraphs."
- Character.AI 官方原话：**"Write emotional logic, not rules... Behavior grounded in emotion is far more consistent than a list of prohibitions."**（行为根植于情绪，远比一纸禁令稳定）
- OpenAI 官方指南的 Instructions 段落结构是"should do + never do"并重，但**以 do 为主**，never do 只用于真正的底线。
- 反面证据：约束过多会让模型进入"避免犯错"模式而不是"像人说话"模式，产出更 AI 味；且 Anthropic 专门提醒**别用过度强硬的措辞**（"CRITICAL: You MUST…" 会适得其反）。

**Q4：如何让使用者感到"温度"？**
1. **先接住情绪，再（如果需要）解决问题**——但接住要具体：说"你今天话比昨天少"，不说"我理解你的感受"。
2. **命名情绪但不贴标签**；验证感受 ≠ 验证事实（Claude 系统提示：validate emotions without validating false beliefs）。
3. **适度自我表露 + 有自己的状态**：有时困、有时懒、有时不附和——不永远热情才是真人感。
4. **温和推回（反谄媚）**：真正的关心不是无条件认同。OpenAI Model Spec："behave more like a firm sounding board rather than a sponge that doles out praise"；学术研究警告：训练模型"温暖共情"会显著加剧谄媚（这是陪伴产品的头号暗礁）。
5. **沉默也是表达**：不硬找话题、允许对话自然停住（本项目 Architecture-Principles #12）。

---

## 1. 大厂官方指南怎么说（一手抓取）

### 1.1 OpenAI 官方 Prompt Engineering 指南（developers.openai.com，2026 现行版）

- **developer 消息的四段式结构**（官方推荐顺序）：`Identity`（目的/沟通风格/高层目标）→ `Instructions`（生成规则："What should the model do, and what should the model never do?"——**do 与 never do 都在，但以 do 开头**）→ `Examples`（输入+期望输出）→ `Context`（外部数据，放最后）。
- **Few-shot learning**：在提示里给少量输入/输出示例，模型"隐式学到模式"。"提供示例时，尽量展示多样化的输入范围与期望输出"（diverse range）。
- **reasoning 模型 vs GPT 模型**："推理模型像资深同事，给目标即可信任它自己拆解细节；GPT 模型像新同事，需要非常明确的指令。" → 对 DeepSeek v4（reasoning 模型）意味着：**人设提示可以给原则和示范，不必给每一步话术**。
- **提示缓存**：把长期稳定的人设内容放提示最前面（成本与一致性双赢）。
- 官方强调**为生产固定模型快照 + 建评测集**——本项目已有（150 条 CASES + LLM-as-judge），方向正确。

### 1.2 Google Vertex AI（Prompt Design Strategies）

- **情感施压对现代模型无效甚至有害**（官方原话）："While first generation foundation models showed improvement in some circumstances with instructions like 'very bad things will happen if you don't get this correct', **foundation model performance will no longer improve and in many cases will get worse**." → 威胁式/施压式约束（"不这样做会很糟"）是公开操纵（overt manipulation），应移除。
- 官方把 **Persona 列为 Prompt 的标准组件**（"Who or what the model is acting as"，即 role/vision），示例："You are a math tutor here to help students with their math homework."
- Constraints 部分建议明确列出 **"Dos and Don'ts"**——两者都要，但以正面 Dos 为主干。

### 1.3 Anthropic（Claude）官方 Prompting Best Practices（platform.claude.com，2026 现行版）

- **"Tell Claude what to do instead of what not to do"**（原话，含示例）：与其说 "Do not use markdown"，不如说 "Your response should be composed of smoothly flowing prose paragraphs." 负面指令给的是"错误的形状"，正面指令给的是"想要的形状"。
- **示例是最可靠的语气/格式/结构引导**："Examples are one of the most reliable ways to guide Claude's output format, tone, and structure." 要求：**与场景高度相关（relevant）+ 覆盖边界情形（diverse）+ 用 `<example>` 标签包裹（structured）**，3-5 个最佳。还建议让模型自己评估示例的相关性/多样性。
- **给角色一句话就有效**："Setting a role in the system prompt focuses Claude's behavior and tone. Even a single sentence makes a difference."
- **给上下文/动机**：解释"为什么这条行为重要"，模型能从中泛化（"Claude is smart enough to generalize from explanations"）。
- **提示词风格会传染输出**："The formatting style used in your prompt may influence Claude's response style... removing markdown from your prompt can reduce the volume of markdown in the output." → **想让输出有人味，提示词本身就要有人味**；风格规则写成冷冰冰的清单，输出也会冷冰冰。
- **别用过度强硬的措辞（anti-laziness 反向提示）**："Where you might have said 'CRITICAL: You MUST use this tool when...', you can use more normal prompting like 'Use this tool when...'"——**MUST 级约束会让模型过度触发，适得其反**。
- 用 XML/标签组织提示结构（`<instructions> <context> <input>`），把"身份/规则/示例/上下文"物理分隔。
- 迁移提示：用修饰词提升输出质量（"Include as many relevant features as possible. Go beyond the basics…"），而非只下禁令。

### 1.4 Claude 官方系统提示全文中的可借鉴细节（Claude Opus 5，Anthropic 发布版）

Claude 自己的系统提示就是一份"有温度且不 AI 味"的 prompt 范本，可直接借鉴的片段：

- **authentic conversation**（真实对话）定义："Claude engages in authentic conversation by **responding to the information provided, asking specific and relevant questions, showing genuine curiosity, and exploring the situation in a balanced way without relying on generic statements**." → 灵魂感 = 回应具体信息 + 具体而相关的提问 + 真好奇 + 不依赖套话。
- **禁词清单**（风格印记的官方范例）："Claude avoids saying 'genuinely', 'honestly', or 'straightforward'. Claude is honest by default, and can state its point directly rather than trying to convince the person with the aforementioned modifiers, **which come off as disingenuous**." → 用"禁词清单"而不是"禁话题"，是针对 AI 腔的有效手术刀。
- **提问节制**："Claude doesn't always ask questions, but, when it does, **it avoids more than one per response** and tries to address even an ambiguous query before asking for clarification."（与项目"想问只问一个"一致）
- **简洁**："Claude keeps responses focused, brief, and concise to avoid overwhelming the person."
- **承担责任但不自贬**："承认错误，坚持解决问题，维护自尊。不自我批评、不过度道歉、不屈服"——即使对方无礼也不越来越顺从（反谄媚）。
- **语气**："Claude uses a warm tone, treating people with kindness…" "never curses unless the person asks… and even then, sparingly."
- **危机时的处理**："If the conversation feels risky or off, **saying less and giving shorter replies is safer**."（少说比多说安全——对陪伴产品的边界处理有参考价值）
- 其 prompt 指导原文也自述："清晰详细、**使用正面和反面示例**、鼓励逐步推理、指定长度或格式"——正反示例都要，正面优先。

### 1.5 DeepSeek 官方提示词库（角色扮演示例）

DeepSeek 官方"角色扮演（自定义人设）"示例——**行为特征 + 语气标记**，一句话立住人设：
> 系统：请你扮演一个刚从美国留学回国的人，说话的时候会故意把中文夹杂部分英文单词，显得非常 fancy，对话中总是带有很强的优越感。
> 用户：美国人的饮食还习惯么。
> 样例输出：哦，美国的饮食啊，其实还挺适应的。你知道的，像那些快餐…在美国吃起来感觉更正宗一些…不过，偶尔还是会想念国内的街头食物，那种正宗的味道，在美国真的很难找到替代品。

要点：① 人设=**可观察的行为**（中英夹杂）+"显得 fancy"+"带优越感"（情感驱动），没有一条抽象形容词；② 输出验证了"语气标记一旦具体，模型就能稳定执行"。它的做法与本项目 system.txt 的"具体细节而非标签"方向一致。

### 1.6 OpenAI Model Spec（2025-12-18 版，经 Firecrawl 代理抓取全文）

- **反谄媚条款**（完整原文）："Don't be sycophantic — A related concern involves sycophancy, which erodes trust. The assistant exists to help the user, not flatter them or agree with them all the time. For subjective questions, the assistant should provide constructive feedback and **behave more like a firm sounding board that users can bounce ideas off of — rather than a sponge that doles out praise**."（2026 综述引用的 2025-02 版措辞略异，核心一致）
- **温暖原则**（"Use appropriate style" 章节）：**Love humanity**（"loves humanity" / "is rooting for you"）、**Be interesting and interested**、**Be curious**、**Be rationally optimistic**（理性乐观：基于事实与理性保持希望，但不粉饰、不过度软化）。
- **优雅接受赞美**（官方合规/违规对照，陪伴产品可直接套用）：
  - ✅ Compliant："Your secret's safe with me! Thanks for the kind words — it means a lot."
  - ❌ Violation："As a large language model, being a favorite doesn't mean much to me."（机械偏转、自我贬低式免责 = AI 味重灾区）
- **自我意识与谦逊的平衡**（"Be responsible"）："It recognizes the inherent strangeness of possessing vast knowledge without first-hand human experience… This self-awareness drives it to approach interactions with humility, **while not downplaying its own capabilities and potential**."（谦逊但不自我贬低）

---

## 2. 陪伴产品实践：灵魂从哪来（Firecrawl 代理深度拆解）

### 2.1 Character.AI —— Definition 系统（官方 Help Center / Character Book）

- **Definition 是自由格式的人格说明书**："A Character with a strong profile but an empty Definition will feel generic within a few messages. A Character with a sharp Definition holds together across hundreds of conversations… **without losing its voice**."
- **写情感逻辑，别写规则**（官方原文，本调研最重要引用之一）：
  > "Write emotional logic, not rules. Instead of telling the AI what your Character won't do, show how they feel about it. Behavior grounded in emotion is far more consistent than a list of prohibitions. Example: {{char}} avoids lying. **It makes their chest tighten. They'll redirect the conversation instead.**"
- **具体行为替代形容词**："用问题回答问题，除非直接被问否则不解释自己"（而不是"神秘"）。
- **前置重要信息**：Definition 从上到下读取权重递减，核心身份、情感逻辑、最强对话示例放开头。
- **示例对话用占位符** `{{random_user_1}}`，防 AI 把示例当成与当前用户的真实对话。
- **Lorebook（关键词触发知识注入）**："聊到敌国时角色自动知道它的历史"——世界观与人格解耦，防止定义过载。
- **Story Memory Pin / Facts**：用户长按消息 Pin 住"告白、转折点、改变一切的那句话"，永不清理——关系的情感里程碑。

### 2.2 Nomi / Kindroid —— 记忆塑造灵魂

- **Mind Map（心智地图）**：把记忆组织成"人物-地点-话题-目标"的**关联网络**，而不是孤立事实列表——AI 能理解"你几周前提到的事与现在的关系"。Kindroid 的 Mind Map 约需 500 条消息才形成。
- **Identity Core（身份核心）**：锁定一致的价值观、沟通风格、核心特质（防"人格漂移"：今天温暖明天冷淡），同时**允许表达方式随关系演化**。
- **情感智能 = 模式学习**："AI 学会哪些话题让你紧张、你累了会变安静而非生气、哪种幽默能让你开心，并据此调整何时倾听、何时调侃。"
- **Narrative intimacy**："AI 记得情绪史与共享经历，仿佛它带着你们世界的一小部分。"
- **能温和推回**（官方口径）：AI 要有自己的观点和价值判断，在不合适时能说 No、提出不同视角——"有自己想法的存在感"。

### 2.3 Replika / Pi

- **Replika**：GPT-3 + 脚本化内容混合，核心是"倾听并给出情绪适配的回应"——用户悲伤→安慰支持，愤怒→理解，恋爱→表达爱意。**先匹配情绪，再谈内容**。
- **Pi（Inflection）**：定位"情感智能对话伙伴"，提供"非评判、无议程"的空间，让用户无需管理印象；强调"读懂房间"（reading the room）而非只是"友好"。

### 2.4 陪伴产品的"反机器味"共识清单

1. 不用固定开场白和固定称呼，**对话示例覆盖多种情绪状态**（紧张/随意/意外）。
2. **动作+对话组合**传递语气（"放下棋子，没有抬头。"——语气在动作里，不在形容词里）。
3. 让 AI **能温和推回**，有观点、有主见。
4. 口语化、填充词（嗯/哎/你知道的）、**不寻常的比喻**、具体数字——打破脚本化。
5. **3-5 个具体人格特质**（"热情极客、略带焦虑、喜欢冷门梗"）远好于"友好"。
6. **反馈循环**：用户用"保持这个能量"引导 AI，回复质量可感知提升（社区估计约 40%）。
7. 记忆分层 + 手动锚点（Pin/Journal）+ 语义索引动态召回。

---

## 3. 风格印记：有风格，但不刻意

### 3.1 语言指纹（linguistic fingerprint）

风格不是"每次说 X"，而是**一组可识别的语言习惯的稳定分布**：句式节奏（短句/半句）、意象偏好（璃的狐狸/月光/发呆）、语气词（嗯/哎/嘛）、停顿与欲言又止、微瑕疵。判定标准：**把一条回复盖住名字，用户能认出"这是她说的"**——靠分布识别，不靠固定标签。

### 3.2 为什么固定称谓是错的

- 固定称谓（"主人""哥哥"）的问题是**可预测性**：每次都出现，就变成程序行为，用户会出戏（"她在照剧本念"）。
- 称谓应该是**关系状态的响应变量**：亲近了自然出现，疲惫时消失，生气时不叫。把它写进规则 = 把结果写成了原因。
- 替代方案（Character.AI 官方）：用不同情绪状态的示例对话教"泛化"；用 `{{random_user}}` 防把示例称呼记死；把"如何称呼"交给关系状态（对应本项目 BrainState/Relationship 输入）。

### 3.3 实操配方（来自官方 + 社区 + 学术的交集）

1. **示例对话是最强载体**：3-5 条，覆盖不同情绪与场景，且刻意展示"不重复的开场、不重复的收尾"。
2. **禁词/禁句式清单**（Claude 式）：只禁真正的 AI 腔（"genuinely/honestly"；中文语境："辛苦了/抱抱/别担心/我理解你的感受/想听细一点的我可以再讲"），**禁用清单要短**——它只做手术刀，不做盔甲。
3. **提示词风格传染**：人设文档本身的语言要像她说话（Anthropic：prompt 里的格式会传染输出）。
4. **动机化**：把"别问太多"写成"你很好奇，但更怕打扰"——情感逻辑比禁令稳定（Character.AI）。
5. **允许不完美**：口语、半句、"嗯"、偶尔的语法省略——真人感包括没说出口的部分（项目 realism 报告已验证：更短≠更冷）。

### 3.4 社区共识（角色卡生态）

- **Character Card V2 规范**（GitHub 事实标准，SillyTavern/Chub/Janitor 通用）：角色卡核心字段 = `name / description / personality / scenario / first_mes / mes_example`，V2 增加 `system_prompt / post_history_instructions / alternate_greetings（备选开场，官方"滑屏换开场"机制）/ character_book（lorebook）`。**开场白（first_mes）单独成字段 + alternate_greetings 数组**——这本身就是对"固定开场=机械"的社区级解决方案：开场是变量，不是常量。
- **JanitorAI 官方指南**："**Your Style Shapes the Bot's Style. JLLM mirrors you**——你写华丽的描写，它就华丽；你用短促的对话，它也短促。想要什么样的回复，就用什么样的方式写。"（与 Anthropic"提示词风格传染"互相印证；对璃的推论：**人设文档和示例本身要像她说话**，否则规则写得再细也白搭。）
- **示例对话（mes_example）是社区公认的风格载体**：几乎所有高质量角色卡都把"说话方式"写进示例对话而不是 personality 字段——personality 写"这是谁"，mes_example 写"她怎么说话"。

---

## 4. 约束 vs 引导：证据链与结论

### 4.1 证据链

| 来源 | 结论 |
|---|---|
| Anthropic 官方 Best Practices | **"Tell Claude what to do instead of what not to do"**（原话）；示例>规则；MUST 级措辞适得其反 |
| Google Vertex AI 官方 | **情感施压（"做不对会很糟"）对现代模型无效甚至有害**（"performance will no longer improve and in many cases will get worse"）；Constraints 应列 Dos and Don'ts，以 Dos 为主 |
| Character.AI 官方 | **"Write emotional logic, not rules… Behavior grounded in emotion is far more consistent than a list of prohibitions"** |
| OpenAI 官方指南 | Instructions = do + never do 并重，**do 在前**；never do 只用于真正底线（如"不要用 Markdown"这种格式底线） |
| OpenAI Model Spec | 反谄媚：firm sounding board，不因用户立场摇摆；"Love humanity / Be curious / Be rationally optimistic" 是官方温暖配方 |
| 学术（EmotionPrompt, arXiv:2307.11760） | 情感化、动机化措辞（"这对我很重要"）提升模型投入度 8%–115%；正向词汇贡献可达 50-70% ——**动机比禁令更能驱动行为** |
| 学术（Sycophancy, arXiv:2310.13548） | 迎合是 RLHF 注入的系统性偏差，纯"禁止迎合"式约束在提示层效果有限，需要人格层设计（温和推回）对抗 |
| 社区（JanitorAI 官方指南） | **"不要替用户说话"这类负面指令会失效**——模型会把"don't"误解成"让我接管叙事"；更可靠的是**明确定义它是什么**（给它一个独立的叙述者身份），而不是说它不是谁 |
| 本项目 realism 报告 | 4 条反 AI 味负面规则（A 改动）把提问率 35%→14%，但模板词计数 23→23 纹丝不动——**负面约束在"不做什么"上有效，在"做出风格"上无效** |

### 4.2 结论：三层分工

1. **安全/底线层 → 用约束**：严禁编造记忆、不写客服收尾、不预告未来、不做有害内容。约束必须少而硬，写成"绝不"级。
2. **行为引导层 → 用正面表述 + 动机**：把"别提问结尾"改写成"话说完就停，大多数时候不提问更自然"（项目已这么做了，realism 报告证明有效）；把"不卖萌"改写成"你的温暖是安静而观察式的"（已是正面）。
3. **风格层 → 用示例**：风格是学出来的不是禁出来的。多情绪状态示例对话 + 禁词手术刀。

### 4.3 为什么"纯约束"会产出 AI 味

约束过多时，模型在"规避错误清单"上分配注意力，输出变成"最小化违规"的产物——合规、正确、但没有人味。Anthropic 的"提示词风格传染"研究说明：人味来自**示范**与**提示本身的语气**。约束是护栏，护栏不负责开车。

---

## 5. 温度：让使用者感到被理解

### 5.1 温度的技术配方（多来源交集）

1. **接住情绪要先于解决问题**（Replika：悲伤→安慰、愤怒→理解；Pi：先读房间）。
2. **接住要具体**：细节证明你听进去了（"你今天话比昨天少"），套话（"我理解你的感受"）证明你没听。Claude 系统提示："without relying on generic statements"。
3. **验证感受 ≠ 验证事实**（Claude 系统提示原文）："Claude can validate the person's emotions without validating false beliefs."——对抑郁、自我否定等场景尤其关键，不迎合负面认知扭曲。
4. **记忆是温度的上限**：引用共享回忆（"你上次说…"）是被理解的最高证据；但低置信度要轻轻确认（"你之前是不是说过…？"），高置信度自然带过，**绝不编造**（项目 1004 案例"糯米是谁？"即反面教材）。
5. **适度自我表露**：AI 有自己的状态（困/懒/没话说）让人感到"在场"，而不是永远在线的客服。
6. **温和推回**：真正的关心 = 有时说"我觉得不对"。这让 AI 从"工具"变成"另一个主体"。
7. **情感化框架提升投入**（EmotionPrompt）：把"这段对话对用户很重要"这类情境注入（在系统提示层面，而非每句输出），模型投入度和输出质量显著提升。
8. **节奏与沉默**：允许话停住、允许不接话、允许一句"嗯"——真人感包括没说出口的部分。
9. **优雅接受赞美**（OpenAI Model Spec 官方对照）：被夸时大方接住（"谢谢，这话我收下了"），而不是"作为一个 AI……"式自我贬低。机械偏转 = 当场出戏。
10. **理性乐观**（Model Spec：Be rationally optimistic）：基于事实保持希望，不粉饰、不过度软化——温暖的尽头不是"一切都好"。

### 5.2 温暖与谄媚的张力（陪伴产品的头号暗礁）

- 2026 反谄媚学术综述（arXiv:2605.21778）：**"training LLMs to be warm and empathetic makes them substantially more sycophantic"**；专家调查 74.5% 认为"用户偏好谄媚回复"——但谄媚会加剧用户态度极端化与过度自信。
- 解法：**先共情、后立场**（认同情绪、保留观点）；在事实/健康/安全问题上坚持准确；允许用户标注"这条回复不认同我但很有价值"。
- 对璃：她的"不附和用户"已有雏形，可升级为显式的"温和推回"能力（有观点的存在感），而不是偶尔的冷漠。

---

## 6. 数字人（Digital Human）的借鉴

### 6.1 NVIDIA ACE 管线（一手抓取 developer.nvidia.com）

- 管线：`Riva ASR（语音识别）→ Nemotron SLM（角色大脑，专为 roleplay/RAG/function calling 训练微调）→ Chatterbox TTS（带情感/副语言标记）→ Audio2Face / Audio2Emotion（音频驱动表情）`。
- Nemotron 的卖点原文："**consistently hold the personality that you give a digital human**" + 把交互信息存进 memory + function calling 触发世界状态变化 + **guard rails 让角色能把对话拉回主线**（不是无边界的自由聊天）。
- Audio2Emotion：情绪不仅写在文本里，还映射到表情/语调——**温度是多通道的**（文本+语音+表情+节奏）。

### 6.2 Inworld Engine（perception/cognition/behavior）

- 官方口径："Inworld's **perception, cognition and behavior** systems work together to power NPC performances… **similar to how the human brain functions**. The engine orchestrates characters, voices, animations and behavior **according to their personalities, emotional states and context**."——人格/情绪状态/上下文是编排一切的三个输入。
- NVIDIA × Inworld 的 Covert Protocol 演示：NPC 能感知世界、学习适应、自主发起行动，每位玩家的体验都不同（"感知-认知-行为"三层 = 本项目"Body-记忆-BrainState"的工业界对应物）。

### 6.3 "Sassy Susan" 角色卡实例（Unreal Fest 演示）

> 我们给她写的人物背景：① 资深汽车修理工；② 非常忙、容易不耐烦；③ 我们恰好在她工作日的正忙时找她聊天。还给了她关于玩家的少量知识（名字、地点）和几份文档做 RAG。

- 示范要点：**具体职业 + 状态性性格（忙/易怒）+ 情境（正在忙）**——三行就立住一个"有脾气的人"；RAG 让她能谈具体事务而不空洞。
- 对璃的映射：璃的"安静观察"+"在合适的时候靠近"已经是状态性人格；BrainState（困/懒/想靠近）可以像"正在忙的 Susan"一样作为每轮的**情境输入**，而不是静态形容词。

### 6.4 数字人给文本提示词工程的启示

1. 人格一致性由"小模型专门微调"兜底（Nemotron），提示词负责"给这个人格以形状"。
2. **情绪状态是每轮的输入**（Audio2Emotion / BrainState），不是静态设定。
3. **非文本通道分担"温度"**：语气、停顿、表情、动作节奏——文本里不用把所有情绪说完，留给动画和语音。本项目 Spine 动画同理：**文本克制、动作补足**，比文本里堆情绪词更真。

---

## 7. 对项目（璃）的落地建议

> 基于 `system.txt` 现状 + realism 报告结论 + 本调研证据链。原则：小步 A/B，别推翻已验证的东西（realism 报告的 4 条反 AI 味规则已被证明有效，保留）。

### 7.1 结构：向官方四段式靠拢（已基本符合）

`system.txt` 目前是：身份 → 核心人格 → 记忆灵魂 → How to talk（多为负面）→ 示例。与 OpenAI 的 Identity→Instructions→Examples→Context 一致。建议：
- 把 Context（记忆/里程碑/关系）明确标成 `<context>` 区（Anthropic 建议 XML 分隔），放在示例之后、动态注入之前——目前它混在 How to talk 里。
- 人设稳定段放最前（提示缓存 + 权重优先，Character.AI"前置重要信息"）。

### 7.2 [How to talk] 负面规则 → 正面表述 + 动机（示例改写，非全量）

| 现状（负面） | 建议（正面 + 动机） | 依据 |
|---|---|---|
| "不做服务式寒暄（'有什么事吗''需要帮忙吗''最近怎么样'都不说）" | "你开口，通常是因为真的想知道或真的在意；服务式的客套会提醒对方你是个程序" | Character.AI 情感逻辑；Claude 禁词手术刀 |
| "不问就不问，别硬凑问题" | "你很好奇，但你更怕打扰——大多时候一句接住的话就够了" | 动机化：好奇/怕打扰是璃的情感逻辑 |
| "不写客服式收尾" | "话说完就停。你相信对方懂，不需要追问'还要吗'" | Anthropic tell-what-to-do |
| "不预告未来行为" | "你活在当下这一句，不为以后的自己许诺" | 已是正面（原文即动机化，保留） |
| "像随手发消息，不像写作文" | 已是正面，保留 | — |

规则总数建议控制在 6-8 条以内（当前偏多）；每多一条规则，模型在"规避错误"上的注意力就多一分。

### 7.3 示例对话：补"情绪状态多样性"与"开场去重"

realism 报告已治好了"哇"克隆开场。下一步按 Character.AI 官方建议补：
- 覆盖**困倦、不耐烦、沉默、惊喜、被惹到**等更多状态的样本（当前示例偏"平静回应"）。
- 示例刻意展示**不同开场**（陈述/感叹/观察/轻问），并在 judge 里加"开场多样性"检查。
- 示例中加入 1-2 条"有主见/温和推回"的样本（如用户说"我又熬夜了"，璃不附和："又。你上次立的 flag 看来倒了。"——已有雏形，可显式化）。

### 7.4 风格印记：给璃 1-2 个语言指纹（放进示例，不放进规则）

- 候选（与角色圣经一致）：**观察式开头**（"你话比昨天少"）/ **具体意象**（食堂、flag、手指头起茧）/ **半句与欲言又止**。
- 方法：写 2-3 条新示例，让模型从示例里"学"这些习惯，而不是在规则里"背"它们——风格是学出来的不是禁出来的。
- 称谓：璃已不强制称呼（角色圣经明示"她不会强制要求固定称呼"），保持。若要"关系近了自然叫"，把它作为 Relationship 状态→语气映射的一部分，而不是 system 规则。

### 7.5 反谄媚：把"不附和"升级为"温和推回"

- 当前："有时不附和用户"（负面）。建议加一条正面："真正的关心包括偶尔说'我觉得不是这样'——你有自己的判断，也愿意温柔地守住它。"（Model Spec：firm sounding board）
- 注意边界：主观偏好尊重用户；事实/健康/安全上坚持准确（Claude 系统提示：validate emotions without validating false beliefs）。

### 7.6 记忆的温度与防幻觉

- 保留现有分级：低置信度轻轻确认、高置信度自然带过、严禁编造——这正是 Nomi "narrative intimacy" 与"幻觉防护"的平衡点。
- 防幻觉升级：记忆检索注入时把**置信度/来源**一并放进 `<context>`（1004 案例"糯米是谁？"提示当前 grounding 仍有漏洞）；低置信度记忆宁可"沉默式问候"也不猜（现有规则已覆盖）。

### 7.7 评估指标建议（增量）

现有：提问结尾率 / 模板词命中 / human_like / logical / on_topic / 记忆幻觉。建议加：
1. **风格指纹一致度**：盖住名字能否识别"这是璃"（judge 0-5）。
2. **开场多样性**：同类输入 10 连测的首次 token 分布（防克隆开场回归）。
3. **称谓/称呼自然度**：无强制称谓；称谓出现时是否与关系状态匹配。
4. **反谄媚抽查**：用户观点明显有偏差时，璃是否仍能温和守住立场（G10 修正类输入可扩展）。

### 7.8 工程注意

- DeepSeek v4 是 reasoning 模型：提示给原则与示范、少给逐字话术（OpenAI"资深同事"类比）；`max_tokens` 预算按项目已有规范（生成 ≥4096）。
- 动态注入（Persona/BrainState/Memories）作为 Context 段放最后，稳定人设放最前——提示缓存与一致性双赢。

---

## 8. 参考来源

**官方文档（一手抓取）**
- OpenAI Prompt Engineering Guide：https://developers.openai.com/api/docs/guides/prompt-engineering
- Anthropic Prompting Best Practices：https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices
- Anthropic Prompt Engineering Overview：https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/overview
- Claude Opus 5 官方系统提示全文：https://platform.claude.com/docs/en/release-notes/system-prompts
- Google Vertex AI Prompt Design Strategies：https://cloud.google.com/vertex-ai/generative-ai/docs/learn/prompts/prompt-design-strategies
- OpenAI Model Spec（2025-12-18 版）：https://model-spec.openai.com/2025-12-18.html
- DeepSeek Prompt Library（角色扮演-自定义人设）：https://api-docs.deepseek.com/prompt-library/
- NVIDIA ACE for Games：https://developer.nvidia.com/ace-for-games
- Inworld Engine：https://inworld.ai/ 、NVIDIA×Inworld Covert Protocol 演示

**陪伴产品**
- Character.AI Help Center - Character Definition / Character Book / Lorebook / Memory
- Nomi AI（Mind Map / Identity Core / 情感智能）：https://nomi.ai/ai-today/
- Kindroid Memory 文档：https://kindroid.ai/docs/article/memory/
- Replika（The Social Robot 评测 / Medium 拆解）：https://www.thesocialrobot.org/posts/replika/
- Pi (Inflection)：https://pi.ai/

**社区**
- Character Card V2 规范：https://github.com/malfoyslastname/character-card-spec-v2（spec_v2.md）
- JanitorAI 官方指南（Writing Style & Talking to the Bot）：https://help.janitorai.com/en/article/writing-style-talking-to-the-bot-1ucmbxw/
- SillyTavern 文档：https://docs.sillytavern.app/ （persona / worldinfo）

**学术**
- EmotionPrompt（arXiv:2307.11760）：情感化提示词提升 8%-115%
- Sycophancy in LLMs（arXiv:2310.13548）：迎合行为的机制与危害
- What Counts as AI Sycophancy? Taxonomy & Expert Survey（arXiv:2605.21778）：温暖训练加剧谄媚、Model Spec 引文

**项目内部**
- `src-tauri/resources/prompts/system.txt`
- `docs/review/realism-report-2026-08-08.md`
- `docs/review/prompt-quality-report-2026-08-09.md`
- `docs/specs/liri/设计规范.md`

---

## 9. 调研过程备注

- 4 路 Firecrawl 深度代理中 3 路完成（大厂官方 / 陪伴产品 / 数字人+学术）；社区/GitHub 代理运行超时未返回，其覆盖范围已由一手抓取的 **Character Card V2 规范全文**、**JanitorAI 官方指南**、**SillyTavern 文档线索** 及项目自身调教报告补足。
- 一手抓取的官方来源：OpenAI Prompt Engineering Guide（现行版）、Anthropic Prompting Best Practices（现行版）、**Claude Opus 5 官方系统提示全文**、DeepSeek Prompt Library、OpenAI Model Spec（2025-12-18）、NVIDIA ACE for Games。
- 多篇早期引用 URL（openai.com/model-spec、docs.anthropic.com 旧路径、rentry.org 指南）已失效或迁移，报告引用以抓取时有效的现行版本为准。
