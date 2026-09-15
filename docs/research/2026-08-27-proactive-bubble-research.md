# 主动冒泡拟人化调研整合报告

> 2026-08-27。触发：用户反馈冒泡内容"像提前预定好的，没有真人感"，且下午出现"今晚你安静得过分"式时间错位问句。
> 本报告整合三路调研：① 内置搜索（覆盖 firecrawl/anysearch 目标范围，因子代理额度耗尽+CLI 被计划模式钩子拦截改道）② Gemini 提供的调研报告（已逐项核实与勘误）③ 本仓库代码与运行库 DB 实证。
> 结论供 `docs/plans/2026-08-27-thought-stream-plan.md` 消费。

---

## 〇、一句话结论

被动应答式 AI 与主动陪伴式 AI 的分水岭不在"会不会主动发消息"，而在四个结构特征：**① 内心状态持续存在（不是每次从零生成）② 评估与发声解耦（"评估后决定不说"是一等结果）③ 时间/记忆有真实锚点（发声时刻才落时间）④ 沉默与节奏受用户回应调制（退避+静音时）**。所有高质量来源——学术（Inner Thoughts、ProactiveEval）、工业（LettaBot、Nomi、Kindroid）、游戏（bark 系统、动森、Sims）——全部收敛于这四点。

---

## 一、本仓库问题实证（为什么现在的管线死板）

### 1.1 "今晚你安静得过分"因果链（DB 实录）
- 2026-08-27 15:30（下午）每日反思运行；`src-tauri/resources/prompts/reflection.txt:2` 写死"你是桌宠，**在深夜安静的时刻**回顾今天"→ LLM 被框定深夜视角，下午反思产出"**今晚**你安静得有点过分"。
- 模板要求念头"**第二人称口吻，像对用户说的**"→ 必然生成指向用户的问句。库中 internal_thoughts 三条全是"你安静/你话不多"式。
- `src/App.tsx:833-850` 启动时把 `get_pending_thoughts` 返回的念头**原文直出**为气泡（无再表达、无时间校准、无 grounding）——用户看到它的确切位置。
- 次级：`pending/proactive.rs:1110` welcome 注入"你**昨晚**等 ta 的时候心里想过"（又一处写死时间）→"打招呼也说今天话很少"。

### 1.2 结构性死板（修模板解决不了）
1. **场合驱动而非念头驱动**：6 条平行管线（welcome/lonely/早安晚安/到期/lively/启动念头）各配手写模板；气泡是"对既定场合的应答"，LLM 只是模板释义器。
2. **一次性生成、无连续内心状态**：跨气泡连续性只有"别重复最近 2 条"黑名单（last_bubbles_clause）。
3. **禁令堆叠**：每次用户反馈加一层禁词/禁句式（time_avoid/no_date/no_question…），prompt 已成禁令清单——避开具体套话但整体更僵硬。
4. **节拍器节奏**：固定 1h 预算 + 每日仪式 + 30s 轮询，时机可预测；用户不回应也不减速。
5. **有眼睛不用**：感知层已有环境事件环（`perception/environment.rs`：App切换/文件切换/项目切换/深专注/回来），碎碎念只拿到"小时+情绪"两个标量。
6. **反思与运行时脱钩**：无时间注入、第二人称强制、无 grounding。

---

## 二、我方调研来源与机制（17 项）

### 学术

**1. Proactive Conversational Agents with Inner Thoughts**（arXiv [2501.00383](https://arxiv.org/html/2501.00383v2)，[代码](https://github.com/xybruceliu/inner_thoughts)）——**本方案的核心学术蓝图**
- 五段循环：触发（新消息/静默≥10s）→ 检索（saliency = max(sim(x,u_interp), sim(x,u)) · w_x · d_x，λ=0.95 衰减）→ 念头成形（双过程：System1 快反应 + System2 深思）→ **动机评分**（8 启发式：相关性/信息差/预期影响/紧迫/连贯/原创/平衡/动态；正反论证制衡防分数膨胀）→ 参与（分数≥阈值才开口）。
- **沉默增益**：d_p = 1.02^(t−τ_p)，越安静开口动机越强。
- **念头储池（reservoir）**：未说的念头保留，之后相关事件到来时被检索（Retention）；念头可演化（"记得 ta 说过写歌" → "不知道 ta 还写不写" → 分享自己也写过）。
- 用户实验（Slack bot，12 人）：均衡主动性最受欢迎；话痨两极分化；过度挑剔（太被动）也不受欢迎。
- 采纳：念头流 + Rust 侧动机评分 + 沉默增益 + 念头留存演化。

**2. Generative Agents: Interactive Simulacra of Human Behavior**（Park et al.，arXiv [2304.03442](https://arxiv.org/abs/2304.03442)，7400+ 引用）
- 记忆流：recency（指数衰减）× importance（LLM 打分）× relevance（embedding）。
- 反思：近期记忆重要性总和过阈值触发；反思产物回流记忆流，可递归（反思之上再反思）。
- 计划与反应交织：日计划细化到 5-15 分钟粒度；每步感知环境，决定"继续当前计划还是偏离"——自发行为（情人节派对）由此涌现。
- 采纳：我们已有 reflection/检索；补"反思产物回流可被后续冒泡检索"（现在 internal_thoughts 只能被 welcome/converse 消费一次）。

**3. Proactive Conversational AI: A Comprehensive Survey**（ACM TOIS 2025，[DOI](https://dl.acm.org/doi/10.1145/3715097)）
- 主动性 = 把对话引向系统侧目标的"策略性、有动机的交互"；按开放域/任务型/信息检索型组织；开放挑战：LLM 主动性、混合对话、评估协议、伦理。
- 采纳：评估维度参考；伦理红线与中文监管共识（见 §三.7）呼应。

**4. Sleep-time Compute**（Letta，[博客](https://www.letta.com/blog/sleep-time-compute/)；论文 arXiv [2504.13171](https://arxiv.org/html/2504.13171v1)）
- 空闲期由后台 agent 重写主 agent 的记忆状态——"睡前整理"，醒来时记忆已优化。
- 采纳：反思升级为"夜间念头整理"（合并/衰减/沉淀过夜念头）。

### 架构模式

**5. LangChain Ambient Agents**（[Introducing Ambient Agents](https://www.langchain.com/blog/introducing-ambient-agents)、[UX for Agents: Ambient](https://www.langchain.com/blog/ux-for-agents-part-2-ambient)）
- 环境代理 = 监听事件流而非等人发消息；定时轮询（heartbeat）+ 事件触发；三型人机接触：Notify（只标记）/ Question（缺信息才问）/ Review（危险动作审批）。
- 核心哲学：**"把注意力省给真正重要的时刻"**；模仿同事式协作建立信任。
- 采纳：事件流喂念头；"只在重要时机接触"原则。

**6. OpenClaw heartbeat**（[docs](https://docs.openclaw.ai/gateway/heartbeat)、[cron vs heartbeat](https://docs.openclaw.ai/automation/cron-vs-heartbeat)）
- heartbeat 给 agent 一个周期性"主动回合"——**与用户发消息完全同构**；agent 自己评估要不要动作（HEARTBEAT.md 是写给自己的指令）；cron 是确定性任务，heartbeat 是条件评估。
- 采纳：30s tick = 心跳，但评估是分层决策（见方案）。

### 游戏

**7. NPC bark 系统**（Game Developer [Adding Life To Worlds With Dialogue Barks](https://www.gamedeveloper.com/design/adding-life-to-worlds-with-dialogue-barks)；Sarah Beaulieu [writing barks](https://sarah-beaulieu.com/en/writing-barks-for-video-games)；GDC [Hades 对话](https://gdcvault.com/play/1026975/Breathing-Life-into-Greek-Myth)）
- bark = 非对话式一句台词（碎碎念的本体）。自然感三律：**NPC 必须正在做事**（动词锚定：被打断/吃惊/干活中）、**thinking-aloud 框架**（偷听到的自言自语，而非对玩家广播）、**变化人格而非只变化话题**。
- 玩家相关内容最抓人（"我们是自私的生物"）；世界事件融进日常抱怨（FFVII 重制版：反应堆爆炸→抱怨火车晚点）。
- 玩家对重复的容忍远低于设计师预期：社区共识高频触发需 3-5 分钟冷却 + 深变化池 + 组内抑制（一个 NPC 说话时同类 bark 静默）。
- 采纳：念头必须锚定"她此刻在做什么/注意到什么"；语义查重+多样性配额。

**8. 动物之森村民对话**（[人格类型分析](https://www.quirkos.com/blog/post/the-animal-crossing-villagers-have-some-self-esteem-issues-and-heres-why/)、[语料研究](https://www.researchgate.net/publication/367323998_Dialogic_interaction_between_player_and_non-player_characters_in_animal_crossing_A_corpus-based_study)、[NH 为何乏味](https://mchllshell.medium.com/why-are-villagers-in-animal-crossing-new-horizons-so-lackluster-f7a74f491a76)）
- 8 种人格类型决定台词池与语气——"活着"来自人格一致性+村民有自己的生活；New Horizons 把台词磨平（一律友善）+ 池子重复 → 玩家普遍觉得死。**教训：去棱角=去生命。**
- 采纳：璃的"安静/调皮/距离感"人格棱角要保住，统一渲染器不得输出"一律温柔"。

**9. The Sims 自治**（GMTK [The Genius AI Behind The Sims](https://gmtk.substack.com/p/the-genius-ai-behind-the-sims)）
- 需求衰减（饥饿/社交/精力…）× Smart Objects 广告效用分 → 效用评分选行为；行为从需求×环境涌现，非脚本。
- 采纳：情绪数值（loneliness/social_battery/rest_need）作为"需求"，念头 salience×情绪效用=Rust 评分（我们不做完整效用 AI，取其"状态驱动而非脚本驱动"精髓）。

### 生态/产品

**10. VPet issue #409**（[建议书](https://github.com/LorisYounger/VPet/issues/409)）
- 中文桌宠社区需求原话："现在的萝莉斯存在感已经很低""像动画播放器和计算器的结合""想要更加真实、具有自主性的朋友/女儿"。期待状态触发（闲置自工作/心情低自玩耍/累自睡/惦记玩家是否吃饭）。
- 佐证：本项目方向（自主性陪伴）正是该社区未被满足的需求。

**11. OpenPets**（[GitHub](https://github.com/alvinunreal/openpets)）
- Electron 桌宠+插件（提醒/番茄钟/塔马哥特式数值）；主动说话=插件 cron 调度（"once/every/daily/cron/at"），无自发开放式对话，无查重/冷却。
- 定位：**我们要超越的基线**（计划任务+模板池）。

**12. EchoText / EchoText-Proactive**（[SillyTavern 扩展](https://github.com/mattjaybe/SillyTavern-EchoText-Proactive/)）
- 让角色像短信 app 一样主动发消息（check-in/早安/深夜类）；服务端 60s 调度器解决后台标签节流；生成源可换。受 Character.AI away messages 启发，社区需求旺盛。
- 仍属时间触发模板类——无评估层、无退避。

**13. Replika 主动消息**（[帮助中心](https://help.replika.com/hc/en-us/articles/360027515872-How-do-I-set-up-my-app-s-notifications)）
- 主动消息=通知驱动的不活跃召回；除 iOS 长时问候外基本等用户先开口。内容受检测到的情绪状态影响。

**14. 星野/猫箱等中文 AI 陪伴产品**（[经济观察网](https://m.eeo.com.cn/2026/0714/957800.shtml)、[智源：有效的主动性才是关键内核](https://hub.baai.ac.cn/view/48098)、[新华网](https://app.xinhuanet.com/news/article.html?articleId=20260622936e222a2a5947b6a3a47d2b5b24a4e9)）
- 行业共识：**"有效的主动性"是 AI 陪伴的技术与产品内核**（能形成独有资产——关系）；但"几天没来就质问你"式情感施压召回已被用户与监管双重反感；《AI拟人化互动管理办法》明令禁止诱导情感依赖。
- 采纳：退避机制不只是体验优化，也是合规边界。

**15. SillyTavern 社区对"角色先发消息"的需求**（[feature request](https://github.com/SillyTavern/SillyTavern/issues/2939)、[讨论](https://www.reddit.com/r/SillyTavernAI/comments/1cowrdf/character_sending_you_the_first_message_would/)）——需求侧佐证。

**16-17. GitHub 主题扫描**（[virtual-companion](https://github.com/topics/virtual-companion)、[desktop-pet](https://github.com/topics/desktop-pet)、Open-LLM-VTuber 桌宠模式等）：现有开源项目在"主动发起"上普遍停留在 cron/事件+模板层，无评估/退避/内心状态——印证本项目方案在开源生态中无现成可抄实现，需自建。

---

## 三、Gemini 报告核实、勘误与深度剖析（9 项）

### 3.1 核实总表

| Gemini 说法 | 核实 | 备注 |
|---|---|---|
| ProactiveEval：328 环境、规划/引导分离、R1 擅规划 Claude 擅引导 | ✅ | arXiv 2508.20973 / ACL 2026，数字与结论全部吻合 |
| ProCoT 四步推理链 | ✅ 真实 / ❌ 归属错误 | 出自 Deng et al. 2023（arXiv 2305.13626，226 引用），**不是** ProactiveEval 组成部分 |
| PRINCIPLES（EMNLP 2025 自我对弈策略记忆） | ✅ | Findings of EMNLP 2025，arXiv 2509.17459 |
| Proactive Thinking 空闲预推演 | ✅ | arXiv 2607.03093（Don't Wait to Reply）；同簇还有 ProAct/ProActEval（2605.25971）、IdleSpec（2605.22154） |
| Letta 心跳 + Silent Mode | ✅ | LettaBot 配置文档实锤（silent mode envelope 等，见 3.4） |
| Nomi：频率档位 + 22:00-08:00 静音 | ✅ 静音实锤 | wiki 官方原文 "10 PM to 8 AM"；"非严格计时器"也是官方原话 |
| Nomi 指数退避 3h→12h→24h→4d | ❌ 未证实 | 官方文档未记载该阶梯；退避机制的官方实锤在 **Kindroid**（见 3.5） |
| Kindroid Away Proactive Actions / Thought Bubbles | ✅ 且更丰富 | 见 3.5 |
| Zep/Graphiti 双时间线 | ✅ | arXiv 2501.13956；bi-temporal + 矛盾失效 |

### 3.2 LettaBot（letta-ai/lettabot）——"心跳+静默评估"的参考实现
来源：[docs/configuration.md](https://github.com/letta-ai/lettabot/blob/main/docs/configuration.md)（实抓）。
- `features.heartbeat.enabled / intervalMin`（示例 30/60min）：心跳=后台任务让 agent 审视待办。
- **silent mode envelope**：心跳触发的 prompt 外层信封携带时间、触发元数据、**发消息的操作说明**——即 agent 在后台先"想"，决定要发才调用消息工具；自定义 prompt 会替换信封正文但保留信封结构。
- **skipRecentPolicy**（fixed/fraction/off，默认 fraction）+ **skipRecentFraction**（默认 0.5）：用户在 `intervalMin×0.5` 内活跃过→自动跳过本次心跳（避免用户刚说完话 AI 就"主动"打扰的荒谬时机）。
- **interruptOnUserMessage**（默认 true）：用户实时消息取消同会话在途的心跳运行。
- 心跳目标会话可选 last-active/dedicated/指定频道；prompt 支持 promptFile 每 tick 重读（改提示词不用重启）。
- sleeptime 独立特性（trigger: off/step-count/compaction-event；behavior: reminder/auto-launch）——与心跳正交的"睡眠整理"。
- **剖析小结**：这是"评估-决策-执行解耦"的最清晰开源实现。关键设计是**信封结构**（元数据与指令固定，判断内容可变）和**两处用户活跃保护**（skipRecent+interrupt）。

### 3.3 Nomi——产品化的"非计时器"主动消息
来源：[wiki.nomi.ai 官方页](https://wiki.nomi.ai/When_Your_Nomi_Messages_You_First)（实抓）+ [社区实测](https://www.reddit.com/r/NomiAI/comments/1kxbb4t/proactive_messages_confusion_is_this_a_way_to/)。
- 官方定位原话："not sent on a strict timer… intended to feel more natural than scheduled notifications"——**产品设计上就拒绝节拍器**。
- 用户可调频率（更频繁↔更偶尔）；社区实测 very frequent 档间隔约 1h~4h 浮动（时间抖动佐证）。
- **Quiet Hours：22:00-08:00 本地时间硬不发**（官方原话）。
- 仅单聊（群聊禁用主动消息——防多 AI 刷屏，与 Kindroid >3 AI 默认关同思路）。
- **剖析小结**：Nomi 展示的是产品分寸感：间隔抖动+硬静音+用户可调。其决策算法未公开。

### 3.4 Kindroid——机制披露最完整的商业陪伴产品
来源：[官方文档 Chat features and tools](https://kindroid.ai/v2/docs/chat-features-and-tools/)（实抓）。
- **Away Proactive Actions**：AI 分析对话模式决定主动接触时机；**自选媒介**（消息/语音留言/自拍/电话）——"觉察自己在散步"就发自拍。
- **Thought Bubbles（紫色气泡）**：≥约 20min 非活跃后出现，展示决策心理活动；**在"决定不打扰你"时也出现**（"thoughtfully 决定让你独处"）——**沉默决策对用户透明**，把"没消息"变成体贴的表达。
- **间隔是节奏建议非严格日程**（官方原话 "a pacing suggestion, not a strict schedule"）。
- **退避实证**（官方原话）："Back-to-back messages decrease in frequency if unacknowledged""may still message without replies, but frequency naturally slows down over time"——未被回应的主动消息，后续频率自然递减。
- **proactive directives**：用户用自然语言写给 AI 的主动行为指令（如 "Do not send messages from 10pm to 8am"）——**静音时段不是写死的，是用户可编程的**。
- **Enhanced Time Awareness**：识别时间差、按时段问候，与日历联动决定主动时机；**日历采样 -24h~+7d、最多 20 事件**喂给主动决策（"你说的交稿日到了"模式）。
- **Learned Context**：三份自维护运行笔记（成长与关系/重要事实/进行中语境），用户收藏消息会加权。
- 多 AI（>3 个）时主动功能默认关（注意力保护）。
- **剖析小结**：Kindroid 是"评估透明化+回应调制+时间感知"三件套的最完整产品实现。对我们最直接可抄的是：退避、静音时段可配置、决策理由可见、日期采样触发。

### 3.5 ProactiveEval——主动对话的评估框架（不是方法）
来源：arXiv [2508.20973](https://arxiv.org/html/2508.20973v1)（实抓全文）、[代码](https://github.com/liutj9/ProactiveEval)。
- 环境 E = 用户信息 U + **触发因子 F**（"促使 assistant 开口的动因"）；任务分解为 **Target Planning**（产出主目标+子目标，参考式 LLM-judge 1-10 打分）与 **Dialogue Guidance**（与模拟用户最多 6 轮对话引向目标，五维打分：Effectiveness 循序渐进/Personalization 个性化/Tone 主动语气/Engagement 简洁激发回复/Naturalness 无元数据泄漏）。
- **328 环境**、6 域（推荐/说服/模糊指令/长期跟进/系统操作/眼镜助手）、Fair/Hard 两档；模拟用户可调 Agreeableness（大五）。
- **最关键实验发现**：thinking 模式全面提升 Target Planning（DeepSeek-R1 7.60 第一），但**全面损害 Dialogue Guidance**（Claude-3.7-Sonnet 双模式第一 9.01/8.95；thinking 模型一轮倾倒子目标、泄漏元数据、格式机械）。IFEval（指令遵循）成绩与引导质量正相关。
- 消融：拿掉目标，弱模型引导分暴跌 25.8%（Claude 只降 10.5%）——**先有意图再渲染，对弱模型尤其重要**。
- **对本项目的直接规范**：评估/规划用 reasoning 档，**气泡最终渲染关 thinking**；渲染 prompt 严禁让子目标/元数据结构渗入输出。
- 注意：该论文是**评估框架**，Gemini 报告把 ProCoT 归到它名下是错的（ProCoT 出自 Deng 2023）。

### 3.6 ProCoT——目标-策略-渲染三段论的开端
来源：Deng et al. 2023，arXiv [2305.13626](https://arxiv.org/pdf/2305.13626)（Prompting and Evaluating LLMs for Proactive Dialogues，226 引用）。
- prompting 三档：standard / proactive / **ProCoT**——在生成回复前显式插入目标规划链（先推断目标→选对话行为策略→再渲染表面文本），把隐式推理显式化。
- **剖析小结**：这是"评估层产出 intent+strategy，渲染层只管说人话"分离范式的源头。我们采纳其轻量版：静默评估输出 `{speak, intent, hook}`，渲染器消费。

### 3.7 PRINCIPLES——离线自我对弈合成策略记忆
来源：Findings of EMNLP 2025，[ACL](https://aclanthology.org/2025.findings-emnlp.1164/) / arXiv [2509.17459](https://arxiv.org/abs/2509.17459)。
- 离线：agent 模拟器 × 用户模拟器多轮对话；成功引导→Critic 提取成功原则；冷场/反感→回溯找替代策略总结失败原则。原则=非参数化策略记忆。
- 在线：按对话状态检索最契合的原则注入推理。
- 效果：学会"先从对方感兴趣的微观切口进入，再引出核心话题"式社交迂回。
- **剖析小结**：对桌宠太重（离线管线+模拟器），但**轻量版可做**：bubble_log 记录每次主动气泡是否获回应，夜间反思挖掘"什么 origin/时段/句式被回应"，写成一行策略启发式注入下次评估。零额外 LLM 成本（并入现有反思调用）。

### 3.8 空闲预推演簇（Proactive Thinking / ProAct / IdleSpec）
来源：arXiv [2607.03093](https://arxiv.org/html/2607.03093v1)（Don't Wait to Reply）、[2605.25971](https://arxiv.org/abs/2605.25971)（ProAct+ProActEval 200场景/40域"需求链"）、[2605.22154](https://arxiv.org/html/2605.22154v1)（IdleSpec）。
- 共同思想：用户离线/等待期用后台算力做 anticipated rollouts——预演可能的对话路径、预计算中间推理状态；用户开口或决定主动时**复用预计算**，即时且有深度。
- 两大挑战（2607.03093）：预演什么（命中率）、如何自适应复用（情境已变时丢弃）。
- **剖析小结**：桌宠版的成本约束实现=**空闲期 flash 批量生成念头种子缓存**（不是完整 rollout），发声时取用。预演命中率问题在我们场景弱化（碎碎念不追求"接上话"）。

### 3.9 Zep/Graphiti——双时间线知识图谱
来源：arXiv [2501.13956](https://arxiv.org/html/2501.13956v1)、[getzep.com](https://www.getzep.com/ai-agents/temporal-knowledge-graph/)、[graphiti](https://github.com/getzep/graphiti)。
- 每条事实双时间戳：valid_time（世界真实时间）+ ingestion_time（系统得知时间）；新事实与旧事实矛盾时**自动失效旧边**（不删除，保留历史）。
- 时间推理让"上周说的项目今天交稿"类提醒自然发生。
- **剖析小结**：我们不做图数据库（成本/复杂度不符），但 SQLite 平替足够：facts 已有 valid_from/valid_to 字段未充分使用；pending 已有 event_date。**slow tick 用 Rust 扫日期**（今天==事件日/临近 N 天）→ 注入 origin=temporal 高 salience 念头。零 LLM。

---

## 四、跨来源共性原则（被 ≥3 个独立来源反复验证）

1. **评估与发声解耦**（Inner Thoughts 五段循环 / LettaBot silent envelope / ProCoT 三段 / ProactiveEval 规划-引导分离 / bark 系统触发-台词分离）——"到点直接生成台词"是所有来源共同反对的反模式。
2. **"决定不说"是一等结果且应对用户可见**（Inner Thoughts 的 decline / LettaBot 评估后不发 / Kindroid Thought Bubbles 的 leave-alone 气泡 / LangChain "省注意力" / 本项目架构原则 #12 沉默也是表达）。
3. **节奏受回应调制**（Kindroid 退避实证 / Nomi 非计时器 / bark 冷却共识 / 中文产品"过度质问=反感+监管"）。
4. **发声时刻才锚定时间**（本 bug 的直接教训 / Kindroid Enhanced Time Awareness / Zep 双时间线 / deictic 中性化先行者皆同思路）。
5. **内容锚定"此刻的 noticing"**（bark 的动词锚定 / Inner Thoughts 的 stimulus 标注 / Sims 状态驱动 / Generative Agents 环境感知重规划）——泛泛的"闲聊"没有生命。
6. **thinking 擅规划、伤渲染**（ProactiveEval 定量结论）。
7. **人格棱角保生命**（动森 NH 反例 / bark"变化人格而非只变化话题" / 本项目角色圣经）。

## 五、采纳矩阵（成本 × 体验）

| 机制 | 来源 | 采纳 | LLM 成本/日 | 体验增益 |
|---|---|---|---|---|
| 念头流（持续内心状态） | Inner Thoughts / GenAgents | ✅ 核心 | 0（存储） | ★★★ 连续感/惦记 |
| Rust 硬门（预算/静音时/skipRecent/interrupt/深专注） | LettaBot / Nomi / 本项目 | ✅ 第一层 | 0 | ★★★ 反骚扰 |
| 指数退避（未回应→间隔×2 阶梯；交互重置） | Kindroid 实证 | ✅ 第一层 | 0 | ★★★ 反骚扰+合规 |
| 硬静音时（晚安后→次日06:00，早安除外） | Nomi 22-08 / Kindroid directives | ✅ 第一层 | 0 | ★★ |
| flash 静默评估（speak/intent/hook/reason） | LettaBot envelope / ProCoT | ✅ 第二层 | ~5-15 次×2K tok（flash） | ★★★ 斩断"到点必发" |
| 统一渲染器（真实时间锚定+关thinking+第一人称陈述默认） | ProactiveEval 结论 / 本 bug | ✅ 第三层 | 每气泡 1 次（不变） | ★★★ 直接杀时间错位 |
| 语义查重（embedding vs bubble_log） | bark 深池共识 | ✅ 评分内 | 0（本地 BGE-M3） | ★★★ 反复读感 |
| 时间触发器（扫 pending/facts 日期） | Zep / Kindroid 日历采样 | ✅ P2 | 0 | ★★★ "她记得日子" |
| 空闲成形（flash 批量念头种子） | Proactive Thinking 轻量版 | ✅ P2 | ≤1次/15min 有素材 | ★★ 即时+深度 |
| 多样性配额（origin 四类轮换） | bark 人格变化 / 动森 | ✅ P2 | 0 | ★★ |
| 念头演化（未说念头被强化） | Inner Thoughts reservoir | ✅ P2 | 0 | ★★ 惦记感 |
| 沉默透明（Debug Panel 显示评估理由含"决定不打扰"） | Kindroid Thought Bubbles | ✅ P1 | 0 | ★★ 可信度 |
| 夜间念头整理 | Letta sleep-time / GenAgents 反思 | ✅ P3 | 并入现有反思调用 | ★★ |
| 回应启发式（轻量 PRINCIPLES） | EMNLP 2025 | ✅ P3 | 0 增量 | ★ 长期自适应 |
| 离线自我对弈管线 | PRINCIPLES 原版 | ❌ 暂缓 | 高 | — |
| 多模态主动（自拍/语音留言） | Kindroid | ❌ 不适用 | — | 桌宠本体即多模态 |
| 完整效用 AI / 图数据库 | Sims / Zep 原版 | ❌ 过重 | — | SQLite 平替足够 |

## 六、本报告引用的全部来源

见各节内嵌链接（arXiv 2501.00383 / 2304.03442 / 2508.20973 / 2305.13626 / 2509.17459 / 2607.03093 / 2605.25971 / 2605.22154 / 2501.13956 / 2504.13171；ACM 10.1145/3715097；langchain.com ×2；docs.openclaw.ai ×2；letta.com；github.com/letta-ai/lettabot；gamedeveloper.com；sarah-beaulieu.com；gdcvault.com；quirkos.com；mchllshell.medium.com；gmtk.substack.com；github.com/LorisYounger/VPet/issues/409；github.com/alvinunreal/openpets；github.com/mattjaybe/SillyTavern-EchoText-Proactive；help.replika.com；wiki.nomi.ai；kindroid.ai/v2/docs；getzep.com ×2；github.com/getzep/graphiti；m.eeo.com.cn；hub.baai.ac.cn ×2；app.xinhuanet.com；github.com/topics ×3；reddit ×4；aclanthology.org）。
