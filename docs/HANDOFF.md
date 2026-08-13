# HANDOFF — 跨会话交接

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **进度以 `cargo test` + harness 为准**；本文件是带上下文的快照，**可能滞后于代码**。
> **维护规则**：每次会话结束前，更新 `§当前任务` 和 `§最近一轮` 两段。
> 最后更新：**2026-08-13（续²⁴·全面测试验收+多轮修复+记忆卫生✅——B/C/D/E 组全验 + Live2D 全移除 + Forget 消歧义/用户字眼/英文fact/瞬时desire/相同summary反问等修复 + 15 条脏 fact 治理。详见 §当前任务 续²⁴）。上轮 续²³·AIRI 风格视线驱动✅。**续⁸ 自主冒泡灵性重构仍在位**（频率30min + 记忆30/灵性70）。**

## 项目一句话
见 [`CLAUDE.md`](../CLAUDE.md)。Kill List 三闭环驱动开发：活着 Body → 记住你 Memory → 懂你 Soul。

## 当前进度（以测试为准）

| 闭环 / 层 | 状态 | 锚定测试 |
|---|---|---|
| 闭环1 说→记住→跨会话召回 | ✅ | `cargo test --test memory_recall` |
| 闭环2 到期主动提起 | ✅ | harness + 实跑：3分钟后主动冒泡提醒 |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 闭环3 "她记得我"体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| 库单测 | ✅ 194 passed | `cargo test --lib` |
| Body 视线 360°（上下） | ✅ | 实跑验收通过（autoFocus:false + ny 取反） |
| 生命感 回来主动招呼 | ✅ 实跑通过 | loop_runner presence 转换 → welcome_back_bubble |
| 生命感 情绪连续外显 | ✅ build 过 / 待实跑 | emotionDriver → Live2DCanvas 连续参数插值（P10 emotionBridge）|
| 生命感 气泡生命力(节奏+glyph) | ✅ 实跑通过 | bubblePacing 打字节奏随情绪(关键词驱动) + bubble-glyph 无文字气泡（#12）|
| 生命感 昼夜节律接入 | ✅ build 过 / 待今晚实跑 | circadian sleepiness 接入微行为权重（深夜 yawn↑/look_around↓，Tier3 #7）— PID 95248 挂着过夜 |
| 生命感 Foley 音效 | ✅ 实跑通过 | 真实 Foley 素材 10 接入 + 启动 hi + 权重静默优先 + cooldown + 亲密度分档；sleep 已接（B3，待实跑）（Tier1 #3）|
| 对话 流式回复 | ✅ 实跑确认 | ipc::Channel 逐字（emit/listen 命令体内投递延迟+listener 立即 unlisten 全丢→Channel 正解）；用户长回复实跑确认逐字 |

**阶段**：三闭环全部端到端跑通（含真实运行）。**原则 #10：优先生命感不优先功能**——别急着加工具性能力。提醒功能是闭环2 的入口补全（生命感：她会主动找你），非工具性能力。

## §当前任务（接手者先看这）

> **2026-08-13（续²⁴）更新 · 全面测试验收 + 多轮修复 + 记忆卫生 ✅ 已收尾（release 已 rebuild）**。用户"继续测试"驱动——按 verify-checklist 全量验收 + 实测反馈逐项修复。**六条主线**：
>
> **① B/C/D/E 组测试验收（全验）**：B 组（circadian/sleeping/sleep音/睡着抑制 nudge）CDP 实跑 ✅，B5 emotionBridge **N/A**（续¹⁹ Spine 无 emotion→表情映射）；C 组（记忆编辑 forget_fact 双向验/摸头降 loneliness 0.0496→0/Scheduler 11 jobs/D14 害羞气泡链路）✅，C3 疲惫眼 **N/A**（同 B5）；D 组（D2/D3 loneliness 代码层+端到端因 CDP 脆弱留日常/D5/D6/D15 代码层 47 单测绿/D10 grounding_guard 4 生成器全接）；E 组 D11 语义漂移（on 0.8507>off 0.7807 ✅）+ D12 人格 judge（On 9.40>Gross 1.50>Subtle 2.40，rule 对 Subtle 0/10 盲 ✅）。
>
> **② Live2D 全移除**（commit `1e3cb0f`，-10,688 行）：删 Live2DCanvas/emotionDriver/behaviorDriver/attention/PetCharacter + public/live2d 3.4MB + npm 依赖（removed 8 pkgs）+ index.html core script。App.tsx 渲染分支塌缩为裸 SpineCanvas（无回退）。连带删 attention 链路（tsc 报未使用暴露的 Live2D 同代孤儿）。
>
> **③ Forget 消歧义修复**（`03a55c3`+`3b88c9c`）：① 反问说"用户"→ disambig_prompt 改"你"；② 触发过敏感 → 抽 `pick_winner_or_ambiguous` confidence gap（top1-top2≥0.15 直接删，<0.15 才反问）；③ 相同 summary 荒谬反问（"喜欢篮球还是喜欢篮球"）→ top-2 summary 相同且非 Pending → 合并取高分不反问（fact+pending 相同仍反问，守 #1）。
>
> **④ 实测四问修复**（`64d4e44`）：D8 Work 白名单加 zcode/opencode（focus 不再 0min）；D5/D13 禁"用户"（extractor.txt PHRASING + welcome_back prompt 改"对方"）；D13 fallback 4 条随机；D6 反 AI 味（system.txt 加"不预告未来行为"）。
>
> **⑤ extractor 文风/规则**（`4efbd2f`+`3b88c9c`）：summary 便签风 2-8 字（禁"表达了…的喜爱"书面腔，正反例示范）；LANGUAGE 强化（中文消息必须中文输出，英文算违规，技术借词除外）；**瞬时 desire 不进 fact**（今天/这周/今晚/最近 开头的临时状态只进 episode）。
>
> **⑥ 记忆卫生治理**（数据层，备份 `desktop_pet.db.bak-en2zh`）：13 条活跃英文 fact 翻译中文（thinks cats are cute→觉得猫可爱 等，保留深蹲100kg/mesh 术语）；过期删除瞬时 fact 3 条（今天想吃牛肉/今天打算练腹肌/最近很忙）。**DebugPanel 修"喜欢篮球看不到"**（recent_facts 去 LIMIT 20 显示全部，`07c5721`）+ **fact 加 created_at 显示**（`3b88c9c`）。
>
> 📋 **待办（下一会话起点）**：① 无阻塞项——release 已 rebuild 最新（含全部修复），桌面快捷方式已生效；② D2/D3 loneliness 端到端、D8 25min 深度专注、D13 Alt+Space 留日常自然触发（代码层+单测已证）；③ D6 Last Turn 区 F12 发消息后查看（代码层确认全 route 填充，疑似观察时机）。详见 §最近一轮 (续²⁴)。
>
> **续²⁵ 计划文档对齐 + 完成度审计 ✅（commit `deb9b3f`，⏳ 待 push——用户代理未开）**。用户"当前按计划还有哪些没做"。逐项核验 implementation-plan.md（P0-P17 + A1-A6）对照代码：**31 个计划模块文件全在，P0-P17 主干 100% 完成**，lib 305 单测绿，三闭环跑通。**真正缺口仅 1 个**：P10.2 Spine emotion→表情映射（续¹⁹ 转向后 Spine 走动画 timeline 不做运行时映射，需美术 look_up/look_down 配合，留后续）。**用户决策砍除 3 项**（计划文档已标）：① P12.1 窗口边缘坐姿（坐标题栏双腿晃荡）；② P11.2 注意力 Focused（对视/害羞）+ Ignored（偷看）两态——Peripheral 已由续²³ Spine gaze 替代；③ P9 标注 Live2D→Spine 转向。**二期/三期 roadmap** 见设计文档 §14（未启动）：二期 Mind=Persona 完整版/Reflection/Consolidation/Lifecycle（注：这些实际已做，设计文档的 MVP/二期划分与实际实现进度有偏差）；二期 Body=番茄钟陪伴/跨显示器/性能降级；二期三期 Soul=Rituals 仪式/Memory Serendipity 弱关联/Relationship Landmarks/告别动画/Shared History/行为学习/VITS 语音。⚠️ **push 待办**：`deb9b3f`（计划砍除）未推（代理未开），下次开代理 `git push`。
>
> **续²⁵·补 二期/三期优先级排序（用户钦定开始实施）**。按"陪伴北极星 + 可行性（无美术/外部依赖优先）+ 用户感知价值"排序，**第一梯队（先做）**：① **Rituals 循环仪式**（早安/晚安/周日总结）— 设计原话"最容易被忽略也是最能产生感情的"，纯后端定时+LLM，高频触达；② **Memory Serendipity 弱关联惊喜**（~5% 主动关联弱相关记忆"突然想起你上次说的…"）— "她真的在想你"核心体感，续²¹ 多样性基础可扩展，纯后端检索；③ **Relationship Landmarks 主动里程碑**（认识满月/百天/第一次深聊庆祝）— landmark 字段已有，后端时间检测+LLM。**第二梯队**：④ 番茄钟陪伴（Body，偏工具，D8 已做反面）；⑤ 告别动画（需美术 Spine 新动画）；⑥ Shared History 翻历史（需前端 UI）。**第三梯队**：⑦ 行为学习 novelty penalty；⑧ VITS/TTS 语音（重依赖）；⑨ 跨显示器；⑩ 性能降级全屏检测。**本轮开始实施 ① Rituals**（详见 §当前任务 续²⁶）。

> **2026-08-13（续²³）更新 · AIRI 风格视线驱动 ✅ 用户确认没问题（含五连坑排查记录）**。用户"头部绕鼠标转动，只在一定范围内生效且必须是头部转动加身体微侧，鼠标的围绕中心也是头部，幅度都不用太大。可以参考 AIRI"。**纯代码实现**（骨骼旋转，零新素材）：`SpineCanvas.tsx` 加 `pointerRef` prop（App 全局光标轮询已有）+ 每帧 head 骨世界坐标→画布坐标，光标距头顶 `GAZE_RANGE=320px` 内生效、径向衰减、范围外平滑回正（AIRI ignored-return）；头 ±10° 绕颈旋转 + 身体(spine)±3° 微侧；指数平滑 τ=0.12s（挂钟时间不受昼夜变速影响）；睡眠时不跟随。**用户三轮反馈的五个坑全记录在 §最近一轮 (续²³)**——最终形态：**只保留水平旋转通道**（下巴必须固定，上下俯仰留给美术 look_up/look_down 动画）。调参入口：`SpineCanvas.tsx` 顶部 `GAZE_*` 常量。CDP 诊断句柄：`window.__gazeDiag`（凝视数值）/`window.__spine`（spine 实例）/`window.__ctDiag`（origin/scale）。详见 §最近一轮 (续²³)。

> **2026-08-13（续²²）更新 · Live2D 全移除——Spine 为唯一渲染 ✅ 代码+静态全绿+实跑确认（release rebuild 已完成 ✅ + 音效治理同轮入库）**。用户"Live2D 相关代码全部删掉"。璃最终走 Spine+PixiJS，Live2DCanvas 仅作加载失败回退（永不触发）；续¹⁹ 架构转向后 emotionVector 链路在 Spine 路径无消费方——纯死代码移除。Explore agent 全量扫描确认 emotionVector/EmotionVector/toEmotionVector/DEFAULT_EMOTION **只被 App.tsx(计算)+Live2DCanvas.tsx(唯一消费)引用**，删除零副作用。
>
> **删除文件（6 个）**：`src/Live2DCanvas.tsx`(352行)、`src/animation/emotionDriver.ts`(140行)、`src/animation/behaviorDriver.ts`、`src/animation/attention.ts`、`src/PetCharacter.tsx`(SVG 原型占位)、`public/live2d/`整目录(3.4MB,21 tracked+2 untracked PNG)。
>
> **连带清理（删 Live2D 暴露的同代孤儿）**：删 Live2DCanvas 后 tsc 报 `attention` 未使用——发现 `attention`/`PetCharacter`/`attention.ts`(computeAttention/AttentionState/PetRect/computeHeadAngle) 整条 gaze 链路是 Live2D 同时代遗留且**全无消费方**（PetCharacter 无人渲染、attention state 只传给它），一并清理（删 Live2D 的自然延伸，非新决策）。`pointerRef` 保留（gaze 基础设施，mousemove 仍更新它）。
>
> **改动文件**：① `App.tsx`——删 Live2DCanvas/emotionDriver/attention imports + toEmotionVector 函数 + spineFailed/transientExpression/emotionVector/attention 4 个 state + transientTimerRef + 6 处 setter 调用 + 渲染分支三元塌缩为裸 `<SpineCanvas/>`（删 onLoadError）+ mousemove 里 PetRect/computeAttention 块 + 4 处 Live2D 注释改 Spine 中性表述；② `SpineCanvas.tsx`——删 onLoadError prop(接口+签名+catch 调用)+ Live2D 注释清理；③ `index.html`——删 live2dcubismcore script tag；④ `package.json`——删 pixi-live2d-display-lipsyncpatch 依赖 + keyword/description 的 live2d 字样（npm install removed 8 packages）；⑤ `spineIntent.ts`——清过时注释。
>
> **验证**：tsc exit0 / vitest 34 passed / npm run build exit0(1006 modules) / **dev 实跑用户肉眼确认璃正常显示**（PID 540 无报错）。
>
> 📋 **待办（下一会话起点）**：~~① **release rebuild**（前端大改+删依赖，`npx tauri build --no-bundle`）~~ **✅ 已完成 14:1x（含本轮音效治理，全量验证 tsc/vitest 34/cargo lib 301 绿）**；② **CSP `wasm-unsafe-eval` 复核**——原本给 Live2D Core，Spine/pixi-spine 是否还需要，需 release build 验（保守起见本轮保留未动，踩坑#7）；③ 后端 `transient_expression` 字段前端已停读（soul 反思仍 emit，无害），Spine 路径下"短暂表情强化"暂无前端体现（续¹⁹ 既定：Spine 表情走动画 timeline）。详见 §最近一轮 (续²²)。
>
> **续²²·补 Forget 消歧义两处修复 ✅（lib 304 绿）**。用户 D5 实跑反馈：① 反问出现"用户"字眼（璃应说"你"）；② "火锅"匹配到"喜欢火锅"fact+"吃火锅经历"episode 却都触发反问，过敏感。**修复①**（converse.rs `disambig_prompt`）：system hint 3 处"用户"→"你/对方"，显式叮嘱"不要用'用户'"。**修复②**（forget.rs）：抽纯函数 `pick_winner_or_ambiguous`——≥2 候选按 confidence 降序，top1-top2 ≥ `AMBIGUITY_GAP=0.15` →明显赢家直接删 top1 不反问（"一个兴趣一个经历在语义说明了喜欢就不该问"=gap 量化语义指向）；gap<0.15 才真歧义反问。**注意**：char_overlap-only（无 embedding）下中文短词 overlap 普遍<0.7 门、forget 几乎不触发；真实 gap 依赖 embedding 环境。3 新单测（single→Winner/clear gap→Winner/close→Ambiguous），forget 共 21 测绿。待 release rebuild。

> **2026-08-13（续²¹）更新 · 记忆浮现多样性 ✅ 全部收尾（含并行会话合并 + release rebuild + 重启）**。用户"记忆浮现按置信度排序，每次都是星际穿越/糯米，太死板"。**三个根因**：① 强化死循环——每次回忆 strength+=0.03 封顶1.0、日衰减×0.998≈无，主导记忆钉死封顶永远赢；② 锚点选择=置信度 argmax（facts.iter().find(可锚定)=最高置信度第一个）；③ 零多样性机制（无冷却/无探索加分）。**修复**：① reinforce 改边际递减 `+0.03*(1-strength)`；② 评分加 novelty=exp(-recall_count/5)（权重 0.4语义/0.2strength/0.15novelty/0.15recency/0.1情绪）；③ 三条浮现路径（proactive generate/welcome_back/lonely）锚点改加权抽样：episode top-8 softmax(score/0.6)+last_recalled_at 12h 冷却（全冷却放宽）、fact 按 1/(1+mention_count) 抽样；对话路径保持 top-1 相关性优先；到期提醒绝对优先。**零新增 LLM/embedding 调用**。**测试**：lib 301 绿（+8）/ golden 29 绿。**并行会话已合并**：另一会话的记忆导出功能（export.rs JSON/MD + 右键菜单三级导出）代为入库 `3b318ca` 并 push；其遗留 debug 实例已清理。**release 已 rebuild（13:32）+ 桌面快捷方式重启，当前唯一实例**。⏳ **待实跑**：观察浮现多样性（Debug Panel 看 anchor 变化；仍死板调 retrieval.rs 顶部 SURFACE_TEMPERATURE↑ / SURFACE_COOLDOWN_HOURS↓）。详见 §最近一轮 (续²¹)。

> **2026-08-13（续²⁰）更新 · 气泡尾巴锚点固定璃头顶右侧 ✅ 已收尾（CDP 实机验证 + release rebuild + 干净重启）**。用户"以底部尾巴为锚点固定到头顶右侧，任何情况都不改变；当前在左侧"。**根因**：锚点硬编码在左侧 + CSS `translate:-50%`（半宽位移随文字宽度漂）+ `.bubble-pet` 覆盖规则（摸头气泡跳 40% 左侧）。**修复**：`PetBubble.tsx` 锚点盒 `left 150→188 / bottom 530→512`（尾巴尖=盒左下 22px、底下方 7px → 尖端窗口 (210,255)=璃头顶右侧，后发团右上）；删 translate + 删 bubble-pet 规则。**验证**：CDP 实测摸头气泡尾巴尖渲染 (211,256) 差 1px；tsc/vitest 34 绿；release rebuild。**顺带**：上轮未提交的 PetBubble 表面+liriAssetPatch+spineIntent 清理一并入库 commit `4687e3a`。**用户回访：左右 OK、要求上移 20px → bottom 512→532（尖端 210,255→210,235），CDP 实测 (211,236) ✅ commit `86eb721` + rebuild + 重启**。详见 §最近一轮 (续²⁰)。

> **2026-08-12（续¹⁹）更新 · Spine 表情架构转向 —— 状态→调动画，代码绝不碰 attachment ✅ 代码完成（⏳ 待美术资产补 + release rebuild）**。用户反馈"现在的嘴绝对不是呼吸状态的嘴，一直张开"+"smile 动画张大嘴笑幅度比现在大2倍"+"我的动画里做了相关内容，不需要再从骨的状态拆解"。**CDP 深查坐实根因（纯美术资产问题，不是代码 bug）**：
> - **idle 嘴一直张开** = setup pose 把 `嘴`/`张大笑嘴`/`小笑嘴` 三个 slot 的默认 attachment 全设成显示状态（应是 null）。`张大笑嘴` 被 body_breath 在 t=0 null 救回，但 `嘴` 和 `小笑嘴` 没有任何动画碰它们 → 永远显示 → 叠在 `脸`（含闭合嘴）之上 = 看着张开。
> - **smile 张大嘴看不见** = smile 动画**只在 deform 里改 `嘴`/`张大笑嘴` 的 mesh 顶点**，**没有任何 slot attachment 时间轴**来 show 这些 attachment。deform 只在 attachment shown 时可见；body_breath 又把 `张大笑嘴` 钉死 null → deform 不可见 → smile 视觉只靠眼睛 slot 切换（笑眯眼）。
> - 用户美术意图（实跑确认）：idle 只有 `脸`（含闭合嘴）显示；smile 动画里才有 `嘴`/`张大笑嘴` 的 attachment 切换 + deform；`小笑嘴` 暂时不涉及。
>
> **架构原则转向（用户钦定）**：**状态/情绪 → 播放对应动画（叠加 track）；动画 timeline 自己管 slot attachment（美术在 Spine 里做）；代码绝不 setAttachment 改 slot**。之前 phase3（emotion→半睁眼）+ phase1 smile 嘴覆盖 + 续¹⁸ forceSyncSlot 都是**错误前提下的产物**（假设 idle slot 要运行时覆盖）——破坏了美术 timeline，导致空眼/双层/嘴常开。
>
> **本轮代码改动（已 commit）**：
> - 删 `spineIntent.ts` 的 `applyEmotionFace`/`fatigueLevel`/FaceState 的 eye/mouth 字段/阈值常量（phase3 emotion 映射全删）；FaceState 只剩 `smileDuration`（串行通道计时用）
> - 删 `triggerSmile` 的手动 `嘴/张大笑嘴.setAttachment`（phase1 嘴覆盖全删）+ `endSmileMouth`；`endAction` 改空 hook（动画自清 slot）
> - 删 `SpineCanvas.tsx` 的 `applyEmotionFace` 调用 + 整个 forceSyncSlot 循环（续¹⁸ 渲染热路径逻辑全删）+ `emotionVector` prop + `emoRef`
> - 删 `App.tsx` 给 `SpineCanvas` 传的 `emotionVector` prop（Live2DCanvas 的保留）
> - **保留**：串行动作通道 + 呼吸对齐 + 4 个 action 全部纯调动画（playAction 只 setAnimation 不碰 attachment）
> - `tsc exit0 / vitest 24 ✅ / build ✅`
>
> 📋 **待办（下一会话起点）· 美术资产补 + release**：
> 1. **美术在 Spine 里改两处**（用户做，几分钟）：
>    - **setup pose**：把 `嘴`、`张大笑嘴`、`小笑嘴` 三个 slot 的默认 attachment 设为 **null**（idle 只有 `脸` 显示）
>    - **smile 动画加 slot attachment 关键帧**：`嘴` slot（t=0 show `嘴` → t=3.93 null）、`张大笑嘴` slot（t=0 show `张大笑嘴` → t=3.93 null）——让 deform 可见
> 2. 美术补完 → 用户把更新后的 `liri.json`/`liri.atlas`/`skeleton.png` 拷到 `public/spine/liri/` → dev 验证（idle 嘴闭合 + smile 时大张嘴 + 笑眯眼）
> 3. **release rebuild**（前端有改动）
> 4. 后续动画（疲惫半睁眼常驻、害羞、惊讶等）按"状态→叠加 track 调对应动画"模式接，不碰 attachment
>
> 详见 §最近一轮 (续¹⁹)。

> **2026-08-12（续¹⁸）更新 · Debug 窗口死锁修复 + Emotion 编辑器动画测试链路（诊断→修复→回退）**：用户"继续修 debug 白屏"+"用面板测试动画看不到效果"。**① 白屏真根因（已修✅）**：`open_debug_window` 是 **sync command 在主线程直接 build()** → build() 等 WebView2 回调但主线程消息循环被阻塞 → **死锁**（日志证据：build 前日志出现、build 后无输出、所有 invoke 全挂起）。修：**改 async command**（tokio 线程执行 build，主线程保持消息循环）→ 用户确认 F12 窗口正常 ✅。**② 表情不动根因（pixi-spine 渲染机制，已定位）**：pixi-spine 渲染走**缓存显示对象** `slot.currentSprite/currentMesh`，**只在 Spine.update() 内按 `slot.getAttachment()` 同步**——`setAttachment()` 只改数据不更新渲染 → 下一帧 update() 又把 sprite 同步回动画值 → 视觉永不变。修复 `forceSyncSlot`（复制 update() 的 region/mesh 分支）后**三态生效**（dev 截图+视觉模型验证：normal 睁眼 / tired 半睁 / happy 笑眯+微笑嘴）。**踩坑记录**：① region 分支 sprite 缓存 key 是 **`attachment.name`**（非 id——mesh 分支才用 id）② region 无 `computeWorldVerticesOld` 方法（多余调用报错，删）。**③ 用户反馈双层图层+启动即眯眼 → 已回退**：显示半睁眼时未隐藏默认眼（互斥缺失→双层叠加）+ 默认 fatigue 0.55>阈值 0.5（启动即半睁）。**当前状态：渲染层 `git checkout` 回退 366ffc8（眼睛正常），DebugPanel 增强保留**。详见 §最近一轮 (续¹⁸)。

> 📋 ~~**待办（下一会话起点）· 重新实现表情映射（3 个修复点已定位）**~~ **（续¹⁹ 已废弃——架构转向，不再用运行时 attachment 覆盖）**：原计划的 ① 互斥隐藏默认眼 ② 阈值 0.5→0.65 ③ forceSyncSlot name key 全部基于"运行时覆盖 slot"的错误前提，续¹⁹ 已彻底删掉这套机制，改由美术在 timeline 里做。**DebugPanel 本次增强（保留）**：Face State 分区（后端 snapshot 本地计算 fatigue/halfOpen/smiling，**不跨窗口 emit**——跨窗口事件是透明事故嫌疑）+ 滑块拖动即时生效（250ms 节流自动 Apply）+ EmotionEdit 后端加 rest_need 字段 + 滑块加 rest_need。

> **2026-08-11（续¹⁵）Debug Panel 独立 OS 窗口 —— ⚠️ 代码+release 已成 / 实跑白屏（用户明示暂不修，留 follow-up）**。续¹⁴·补 的内嵌可拖浮窗(300px)仍挡 Liri 下半身——根因：主窗 400×760 透明，`position:fixed` 被窗口边界裁剪、拖到哪都重叠。唯一彻底解=独立 Tauri 第二窗口。落地 8 处：①`commands.rs::open_debug_window`(WebviewWindowBuilder label=debug，已存在则 show+focus；360×720 resizable) ②`lib.rs` invoke_handler 注册 ③`capabilities/default.json` windows `["main"]`→`["main","debug"]`(授权 debug 窗 invoke) ④`main.tsx` 按 `?window=debug` 分支渲染 `DebugStandalone` ⑤新 `DebugStandalone.tsx`(onClose=关 debug 窗/onQuit=quit_app/anim 占位) ⑥`App.tsx` F12/Ctrl+Shift+D→`invoke("open_debug_window")`+删 showDebug 全家(state/import/forceCapture/内嵌渲染) ⑦`DebugPanel.tsx` 删自绘拖拽全套 ⑧`styles.css` `.debug-panel` 还原全屏(`fixed inset:0`)+`.debug-toolbar` 删 `cursor:move`。AnimFSM 分区主窗独占(前端 state 不跨窗)，余照常轮询后端。**cargo check 34.44s ✅ / tauri build --no-bundle 52.98s ✅ / commit 7f5e912 + push(6dcbe90..7f5e912)**。**⚠️ 实跑白屏**：F12 弹独立窗但内容全白。疑似 `WebviewUrl::App("index.html?window=debug")` query 未被保留→main.tsx 分支没命中(或 DebugStandalone 渲染了但 `if(!snapshot)return null` 因 invoke 失败永空)。修复方向：改 `getCurrentWindow().label==="debug"` 判据 + WebviewUrl 纯 `index.html`。详见 §最近一轮 (续¹⁵)。

> **2026-08-11（续¹⁴）Spine driver phase3-A 情绪→半睁眼持续映射 —— ✅ 代码+release（⏳ 待实跑确认疲惫半眯眼）**。续¹³ driver phase1 接完后做 Phase 3（emotion→表情 slot，#10 情绪连续外显——Live2D 早全维度接、Spine 此前**零**接，是 Spine 路径最大缺口）。App 早已算好 emotionVector，此前只传 Live2D、SpineCanvas 没收。**MVP 只接 fatigue→半睁眼左/右一维**（最无争议/无耦合/最显著）：spineIntent FaceState 加半睁眼 slot+att（initFace 独立 try 捕获，缺失只降级 emotion 眼 #6，smile 嘴不受影响）；fatigueLevel 镜像 emotionDriver Live2D 眼公式；applyEmotionFace 每帧 spine.update() **后**写 slot——几乎所有 idle 都 key 半睁眼=隐藏，必须 update 后覆盖才稳定显示（不闪）；blink/smile busy 时让位（blink 自切半睁眼/闭眼，smile key 笑眯眼，叠加会乱），ear/tail 不 suppress（不碰眼，emotion 眼继续）。阈值 fatigue>0.5（明显累才半眯，克制）。**tsc exit0 / vite build 2.84s / tauri build --no-bundle exit0(1m01s) / commit 366ffc8**。**⏳ 待实跑**：Debug Panel Emotion 编辑器拉低 physical_energy 或拉高 rest_need → Apply → 璃半眯眼；blink/smile 瞬间让位后恢复。**skip**（spec 既定但留 follow-up）：笑眯眼+小笑嘴(mood>0.55 常驻——与 Liri 安静 15% 人格张力 + smile transient 嘴 override 耦合)、眉毛(stress→下垂，需补骨骼程序化位移/动画)。详见 §最近一轮 (续¹⁴)。

> **2026-08-10（续¹³）Liri Spine driver 层 phase1（串行通道 + 呼吸节拍对齐治跳变）—— ✅ 代码+release（⏳ 待用户最终确认体感）**。续¹² 全身上屏后接 driver 层。用户三轮反馈收敛出**跳变根因**：node 解析 liri.json 坐实**所有 idle 动画（ear/tail/arm/hair）都 key 整条脊柱链+head**（非仅命名部位）→ ear/tail 在 `body_breath` 呼吸**中途**插入时 spine 从"呼吸中间态"瞬间跳到"idle 首帧(setup)"= 跳变；loop 接缝/clearTrack 硬切是次因。**最终方案（用户点破"呼吸轮回中性才允许动作加入"）**：① **单一串行动作通道**——blink/ear/tail/smile 共享 `busy` 标志，一次只播一个（含 0.3s `setEmptyAnimation` 平滑收回），绝不重叠；② **呼吸节拍对齐**——ear/tail（key spine）只在 `body_breath` 每轮 `complete`（身体回 setup 瞬间）触发，首帧即 setup→零跳变；blink/smile 只 key 眼 slot 不碰脊柱→不跳，保持独立计时（眨眼~5s/笑~12-18s）但受 busy 互斥；③ idle `loop=false`（消 loop 接缝跳）+ `setEmptyAnimation(fade)` 收尾（消 clearTrack 硬切）。**双时钟**：`deltaMS`(circadian 缩放) 喂 `spine.update`（动作播放随昼夜变速，#10），`elapsedMS`(挂钟) 驱动间隔（"多久动一次"昼夜稳定——上版用缩放 dt 致深夜间隔被放大成~1min）。**笑容嘴部覆盖**：smile 只 key 眼，手动 `嘴→null`+`小笑嘴→附件` 持续 smileDuration。落地：新 `src/animation/spineIntent.ts`(翻译层) + `SpineCanvas.tsx`(串行调度+breath complete listener) + `App.tsx`(Spine 分支传 behavior)。tsc exit0 / release rebuild exit0（48.95s）。**⏳ 待用户确认**：跳变根治否/串行节奏自然否/频率(眨眼~5s·耳尾~10-16s·笑~12-18s)OK 否（用户"今天就到这"未给本轮反馈）。详见 §最近一轮 (续¹³)。

> **2026-08-10（续¹²）Liri Spine 全身显示 —— ✅ 两个 release-only bug 已修 + 用户目视确认全身**。续¹¹·补² rebuild 后用户实跑暴露两 bug（**dev 隐身**——dev tauri 自动放宽 CSP 故 dev 永远正常，踩坑#7 同类）：① **重启空白** = CSP 缺 `worker-src`（PIXI/pixi-spine 建 `blob:` worker 被阻→PIXI Application 崩→画布空白，后端/React 正常极难排查）→ `tauri.conf.json` CSP 加 `worker-src 'self' blob:;`。② **只显上半身** = pixi-spine `getBounds()` 返回 scale=1 缓存 vertices（`update()` 时烘焙，之后 `scale.set()` 不重算）→ 原 centering 信任谎言 bounds 把璃推到 world y∈[400,940]、可见区[0,600]只露头肩；**修复**：scale=1 时量 `b1=getBounds(true)` 手动做缩放 centering（`spine.y=H/2-(b1.y+b1.height/2)*fit`），worldBounds 手算 y∈[30,570] 全入画布，click hit bounds 同从 `b1×fit` 手推。CDP 数值诊断坐实（WebView2 `--remote-debugging-port=9222` + `Runtime.evaluate` 量真实 bounds；`analyze_image` 此例不可靠——上半身截图两次误判"完整"，**数值诊断优先于视觉模型**）。`npx tauri build --no-bundle` exit0 + 用户确认全身。**下一轮（driver 层）**：分层 idle 轨道（ear/hair/tail/arm_idle loop）+ 表情 slot 映射（emotion→半睁眼/笑眯眼/小笑嘴；transient→smile track2）+ 视线（neck/head 骨骼追指针）+ FSM behavior→动画映射 + 测试面板；`Live2DCanvas` 占位待删。详见 §最近一轮 (续¹²)。

> **2026-08-10（续¹¹·补²）Liri 设为默认渲染 + 加载失败回退 —— ✅ 代码+release（续¹² 已验：璃全身上屏非 Haru + body_breath 呼吸）**。用户"打开后还是旧桌宠（Haru），没切换"。根因：`USE_SPINE` flag-gated 默认 false（#6 优雅退化），且用户开桌面快捷方式 = release exe（续¹¹ 未 rebuild，仍 Haru 路径）。但 **Liri 是最终角色、Live2D 为占位待迁移**（memory），Tauri 窗口无地址栏靠 `localStorage.spine=1` 切换对用户不友好 → **直接翻默认**：① `App.tsx` 删 `USE_SPINE` flag（URL/localStorage 双触发全删），改 `spineFailed` state（默认 false=走 Spine）② 渲染分支 `{!spineFailed ? <SpineCanvas/> : <Live2DCanvas/>}` ③ `SpineCanvas.tsx` 加 `onLoadError` prop，asset 加载 catch → `setSpineFailed(true)` → **自动回退 Haru**（永不空白，console 留 `[Spine] model load failed` 报错可诊断）。**tsc exit0 / release rebuild exit0（1m19s，desktop-pet.exe）**。**待实跑**：打开桌面快捷方式 → ① 璃模型上屏（非 Haru）② body_breath 呼吸播放 ③ 若仍 Haru = Spine 加载失败，F12 Console 看 `[Spine] model load failed` 报错（最可能 pixi-spine@4.0.6 对 spine 3.8.75 兼容 / atlas-skeleton.png 解析），把报错贴出诊断。**下一轮（驱动层，续¹¹ 既定）**：分层 idle 轨道 + 表情 slot 映射 + 视线 + FSM→动画映射。

> **2026-08-09（续¹¹）Spine 链路里程碑1 —— ✅ 加载+显示+动画接线完成（tsc/vite build 绿 / 待 dev 实跑）**。用户"Spine 素材出来了，别再无限打磨动画，先把 Spine资产→加载→窗口显示→动画调用 整条链路跑通"。侦察发现 **资产已导出**（非仅工程文件）：`D:\Spine pro 3.8.75+K'D\...\成品\Liri_Project\{liri.json,liri.atlas,liri.png}` spine **3.8.75**。Node 一次性脚本解析 JSON 确认美术**几乎踩中全部 GPT 建议**：① 动画命名已是分层式 10 条（`body_breath`/`blink`/`ear_idle`/`hair_idle`/`tail_idle`/`smile`/`tail_happy`/`wink_L`/`wink_R`/`arm_idle`）② **表情=slot attachment 切换**（atlas 有 左/右眼·闭眼·半睁眼·笑眯眼 + 嘴/小笑嘴/半张笑嘴/张大笑嘴 + 眉毛 独立图，`smile`/`blink`/`wink` 靠 `slots` 时间轴切可见性）③ 骨骼干净（root→pelvis→spine→…→head；尾巴 6 节链；耳/发/裙/飘带 2-3 节链）。**关键约束**：① 现有 `emotionDriver`/`behaviorDriver` 吐 Live2D Cubism 参数 ID（`ParamEyeLOpen` 等），Spine 用不上——**意图层（FSM/circadian/EmotionVector/behavior）复用，参数翻译层需重写为 slot/track**（留驱动轮）② FSM 14 behavior vs Spine 10 动画非 1:1，MVP 接有对应动画子集 ③ 运行时必须 `pixi-spine@4`（`@pixi-spine/runtime-3.8`+loader-uni；官方 spine-pixi 只支持 4.x 格式装不了 3.8）④ **纹理名不匹配**：atlas 引用 `skeleton.png` 但磁盘是 `liri.png` → 改名 `skeleton.png` 对齐 atlas（重导出一致）。**落地**：① 资产拷 `public/spine/liri/{liri.json,liri.atlas,skeleton.png}` ② `pixi-spine@4.0.6` 装（8 包无 peer 冲突）③ 新 `src/SpineCanvas.tsx`——PIXI app + `Assets.load("/spine/liri/liri.json")`（pixi-spine loader-uni 自动解析同名 liri.atlas→skeleton.png）+ `new Spine()` + `setAnimation(0,"body_breath",true)` + 居中缩放 + circadian `speedModifier` 每帧写 `app.ticker.speed` + bounds 上报（loose/tight 镜像 Live2DCanvas）+ click 上下分屏代 hit area（头/身，待真 polygon hit）④ `App.tsx` 加 `?spine=1` URL flag 三目（USE_SPINE 默认 false→Live2D Haru 路径**零影响**，fallback 全保留）⑤ 生成 GPT 要的两份 spec：`docs/specs/liri/{skeleton_structure,animation_spec}.md`（骨骼/slot/动画/track分层/mix/FSM映射全来自实测 JSON）。**验证**：`tsc --noEmit` exit0 / `npm run build` exit0（604 模块 2.35s）。**待实跑**：`npm run tauri dev` → 地址栏加 `?spine=1` → 确认 ① 璃模型上屏 ② body_breath 呼吸播放 ③ 控制台无 load/atlas/texture 报错。**release 未 rebuild**（Spine 走 flag 默认关，Haru 默认路径不受影响；待链路实跑验过再 rebuild）。**下一轮（驱动层）**：分层 idle 轨道（track1 ear/hair/tail/arm_idle loop）→ 表情 slot 映射（emotion→半睁眼/笑眯眼/小笑嘴；transient→smile track2）→ 视线（旋转 neck/head 骨骼追指针）→ FSM behavior→动画映射 → 测试控制面板（按钮触发各动画）。

> **2026-08-09（续¹¹·补）⚠️ Spine 待实跑 —— 用户反馈"打开后还是旧桌宠（Haru），没切换"**。**非代码 bug**：`USE_SPINE` 默认 false（flag-gated，Live2D 零影响 by design #6）。最可能原因：① 开的是**桌面快捷方式 release exe**（续¹¹ 明确 release 未 rebuild，仍是 Haru 路径）② 或 dev 未开 `?spine=1`/`localStorage.spine=1`。**明天先验**：`npm run tauri dev` → DevTools Console 跑 `localStorage.setItem("spine","1")` 回车后刷新（Tauri 窗口无地址栏，URL flag 不可用，已加 localStorage 双触发）→ 确认 ① 璃上屏（非 Haru）② body_breath 呼吸 ③ Console 无 load/atlas/texture 报错。若开了 flag 仍 Haru = 真 bug（但 SpineCanvas 加载失败应显空白而非 Haru，概率低）。链路验过再 `taskkill //IM desktop-pet.exe //F` + `npx tauri build --no-bundle`。**已提交未 push（2957cf6）**。

> **2026-08-09（续¹⁰）选择性遗忘：多轮消歧义 + fact/pending 语义匹配 —— ✅ 已收尾（lib 293 + check --tests + release 17:20 已 rebuild）**。08-05 episode/fact/pending 遗忘 MVP 是**单轮、零状态、最高分赢家通吃**——两个体验缺口：①「忘掉咖啡」同时命中 fact「咖啡」+ episode「和糯米喝咖啡」时直接猜删一个（可能删错，#1 不可违背）②「忘掉早睡的事」因 char_overlap 字面不重叠匹配不到 fact「想早睡总是熬夜」→ 生硬"不记得"。**模块 A 多轮消歧义**：`forget_best_match` 改三态 `ForgetOutcome::{Deleted{summary}, Declined, Ambiguous{candidates}}`（替 `ForgetResult`）——≥2 候选**不删而反问**（landmark 已被 episode 腿过滤，候选皆可删）；新 `PendingForget{query,candidates,created_at}` 跨轮 slot（抄 `ConverseCtx.pacing` 的 `&Mutex<Option<..>>` 范式）；纯函数 `resolve_candidate`（序数词表 `ordinal_index`：第N个/前者/后者/最后/1/A/甲乙 + `cjk_to_digit` → 索引；否则 char_overlap 取最高≥0.4）+ `is_off_topic`（无序数且全候选 char_overlap<0.2 → 判换话题）。**关键控制流**：第二轮"第一个"不进 Forget gate（Forget 是动词驱动）→ converse **在 ingest 之前** `resolve_pending_forget` 拦截——take-and-clear 一次锁（>90s stale drop）→ resolve 命中 `execute_candidate` 删 + 跳过 ingest（合成 Silence-route outcome，二轮不被存为新记忆）；off-topic → 正常 ingest；仍不明 → 重问一次（slot 已清，防循环）。三条路径（Resolved/Reask/Proceed）汇合到既有 chat 生成回复。**模块 B fact/pending 语义匹配**：`find_fact_candidate`/`find_pending_candidate` 加 `embedding: Option<&EmbeddingService>`——char_overlap 粗筛 top-5 → `semantic_rerank` 现场 embed_batch + `cosine_similarity`（未归一，`((cos+1)/2).clamp(0,1)` 映射匹配 retrieval::compute_semantic）→ 0.7 门；embedding 任意 hiccup 退回 char_overlap（#6）。**模块 C harness 同步（踩坑#4）**：`ForgetCandidate` 加 `#[derive(Debug,Clone)]`（ForgetOutcome/PendingForget 的 Vec 成员要求）；`IngestionOutcome.forget` 类型 `ForgetResult`→`ForgetOutcome`（字段名不变）；3 harness（conversation/memory_recall/prompt_quality）ConverseCtx 构造点加 `pending_forget: &Mutex::new(None)`；prompt_quality case 1009 经真模型验证为**单候选**（见续¹⁰「修正」）→ 保持 ForgetAck；`Expect::ForgetAsk` 启发式保留备用。**修正（9bc3dac）**：① BGE-M3 无关基线 ~0.5 raw → 映射 0.75 > 0.7 门致假阳性——`semantic_rerank` 改**只提升 char_overlap>0 的条目**（字面锚点），1002/1005/1007 误 Ambiguous 已解；② 1009 episode leg `retrieve(top_k=1)` 被 offer 地标挡住 → 早睡 episode 没被看到 → 单 fact → ForgetAck（种子假象 + 既有局限，生产无地标干扰则正常双候选）。**lib 293（forget 18 测含 6 新）/ check --tests ✅ / prompt_quality G10 全 9 例 hard-check 0/9**。→ release 待 Fix B 后重 rebuild；待实跑见 D15。**当前无进行中任务**。

> **2026-08-09（续⁹）记忆卫生层 —— ✅ 已收尾（全测试绿 + 数据治理已执行，release 待 rebuild）**。用户"1先观察 2治理，且不能只清这一次脏数据——设计更好结构防复发；设计完自复盘3次（多角度：合理否/会否引新问题/有无更优解）；先调研可复用框架别急着造；设计复盘后自主执行并测试"。**firecrawl 调研**：mem0（REJECT 闸 + ADD-only 软废弃，V3 已砍 LLM judge 翻车+成本）/ MemGPT-Letta（blocks+caps+后台 sleep-time worker）/ Zep-Graphiti（bi-temporal 知识图谱，判 overkill）。读码定位**三类结构性缺陷**：**A 抽取无校验**（store 全信 extractor + LLM 自打 confidence，"太阳东升西落"conf0.98 入库）/ **B 读路径强化**（`retrieve()` 每次读都副作用写 `reinforce()` → recall_count 飙 382/445/446、strength 饱和钉 1.0、富者愈富）/ **C 去重视区**（known_facts 只拉 preference 类，跨类糯米碎片化→重抽）。**三次复盘关键纠正**：B"无衰减"为**假**——`decay_strength`(×0.998/天) 已在 `loop_runner:309` 每日运行，故砍掉新衰减子系统。**两层确定性卫生（LLM 只提议、Rust 校验，#1）**：Part1 新 `mind/memory_gate.rs`（category 白名单 + 噪声 key/value deny，store 写库前过滤；中文 trivia 靠 key `knowledge_question` 抓，6 单测）；Part2 `retrieve()` 删 reinforce 副作用→纯读 + 新 `reinforce_top(db, episodes)`（仅 converse + proactive genuine-recall 显式调用，零签名变更，避坑#4）；Part3 converse known_facts preference-only → `get_all_active(30)`。**不做**（复盘收敛）：知识图谱 / LLM judge 二次校验 / 新衰减 / importance 地板 / gate kill-switch（均见 ADR rationale）。**一次性数据治理**：expire 10 噪声 facts + 19 非地标 episode strength snap 回 importance（保留 current_reading + 糯米 relationship/preference 副本），DB 备份 `.bak-hygiene`。**测试全绿**：lib 287 / golden 29 / memory_gate 6 / **闭环2 ✅ pass**（途中发现并修**续⁸ 既存 bug**：lively 70% 概率早返回跳过到期 pending → `proactive.rs::generate` 一行守卫 `pending_due.is_empty() &&` 掷 lively 骰，到期提醒现确定性触发，70/30 多样性对无 pending 场景保留）。ADR `docs/decisions/2026-08-09-memory-hygiene-layer.md`（含三次多视角复盘全文）；治理脚本 `scripts/migrate_memory_hygiene.py`。commit `7f4af17`（卫生层）。→ 详见 §最近一轮 (续⁹)。

> **2026-08-09 续⁸ 自主冒泡：频率修复 + 灵性重构（记忆30/灵性70）—— ✅ 已收尾（lib 280 全过 + release 重建 exit0）**。用户反馈：① 频率太高（几分钟一冒）② 内容单一（全和糯米有关，要像真人突然找你聊天，可自言自语/撒娇）。firecrawl 调研 + AskUserQuestion 定（频率=30min 可配 / 比例=记忆30:灵性70）。**频率根因（bug）**：`commands.rs:470` 硬编码 `now-31min` 绕过 trigger_proactive 的 30min 门控 → 5min 轮询每次过 → 高频。**内容根因**：`proactive.rs::generate` 固定 query + 强制 memory anchor + "只聊这件事" + 无锚点沉默 → 永远糯米。**修复**：① 频率——AppState 加 `last_proactive_bubble: Mutex<Option<DateTime>>`，check_proactive 读真实值传 trigger_proactive（新 `min_interval_secs` 参数，config `proactive.min_interval_secs` 默认 1800），过门控即占位（conservative 宁少勿突兀，生成失败也不重复触发）。② 灵性——generate 入口 `rand` 加权（≥30 走 lively）：**memory(30%)** query 轮换池 5 条 + 无锚点降级 lively 而非沉默；**新 generate_lively(70%)** 不调 retrieve（省 embedding）、空 RetrievalResult 让 grounding_guard 自然禁编造用户记忆、注入**本地时段+情绪**驱动 prompt（自言自语/撒娇/碎碎念）。两编译坑已修（ThreadRng 非 Send→rng 收敛块内 drop；chrono Timetrait→format("%H")）。**lib 280（+3 测）/ check --tests ✅ / release 重建（1m10s+2.64s 前端）**。→ 待实跑：① 冒泡≈30min ② 内容不再全糯米、出现自言自语/撒娇（Debug Panel action=lively_bubble）。详见 §最近一轮 (续⁸)。

> **2026-08-09 接手完成 续⁷ 收尾**（用户"读 handoff、用 codegraph 了解代码、继续完成昨天未完成的聊天回复问题"）。codegraph + 源码逐处复核续⁷ 三处改动确在位（非仅信旧记）：converse.rs:415 `ThinkingConfig::disabled()`+`reasoning_effort=None` / grounding.rs:290 空记忆显式标记 + :293 非空 footer + :690 测试断言 / system.txt round-2 8 样例。exe 未运行→**rebuild 成功**（10:40:44，exit0/0警告/23.3MB）。② 向用户完整诚实报告（速度/性格/幻觉根因 + G6 trade + 方差）已交付。⏳ **当前无进行中任务**。**速度：用户确认可接受（max 4s/mean 2.7s 达标）→ gate/extractor 并行优化不做，留 backlog**（已分析两轮[续⁷ option A + 2026-08-09 AIRI 调研]，结论固化：gate 与 extractor 互相独立却串行[converse.rs:99→mod.rs:48]，`tokio::join` 并行预期砍 ~0.5–1s 首字延迟，代价是 Question/Discard 路由白跑一次 flash extract，需要时直接做不必再调研；AIRI 的本地零 RTT 复制不了[API-bound]）。仍待：用户实跑验收手感 + 决定 G6 follow-up。

> **2026-08-08（续⁷）速度+性格+幻觉根因 —— ✅ 已收尾（代码 lib 277 全过 + 已提交 `13e7dc8`；release 2026-08-09 10:40 重建）**。6 轮 A/B 跑完、代码改完验证。**接手三步已全做**：① **rebuild release** ✅（`npx tauri build --no-bundle`）② **向用户完整诚实报告** ✅：速度已解决（main 关思考，FULL max4s/mean2.7s/0超5s → **option A 不做**）/ 性格回归（round-2 soul block+8样例，human 4.07）/ 空记忆幻觉**已修**（grounding 显式标记，fresh 组全 0）/ **已披露 G6 越界 6/10 = 性格同源 trade**（样例教"上次说"framing = 用户要的"连过去"性格，不可全除）/ ~8pp run-to-run 方差。③ **可选（待用户定）**：用户在意 G6 → 软化 ex2/ex3 出处 framing（**削弱性格，需权衡**）或流式 chat 路径运行时阻断（流式已流出 token 无法撤回，本质受限）。**完整 6 轮 arc + 根因 + 代码改动清单见 §最近一轮 (续⁷)**。

> **2026-08-08 自主批次推进中**（用户授权："挨个推进 2,3,4,5,6 [审计清单里 5 个未实现/未接线项]，每完成一项自主验证、更新 HANDOFF + 新增待测试，不报告不询问；并砍掉走路相关计划"。逐项推进，每项 cargo test --lib / check --tests / tsc 绿后勾选并提交；release exe 在批次末统一 rebuild）：
> - [x] **Item 2 接线 is_deep_focus（P14.3）**：审计发现 `commands.rs:352,446` + `proactive.rs` 全硬编码 `is_deep_focus:false` → `trigger_proactive` Rule1（深度专注抑制）永不为真、空转。新 `perception/focus.rs`：纯函数 `update_continuous`（同一 Work app 累积 / 切换 Work app 重置 / 非 Work 重置）+ 后台 30s 采样线程（镜像 cursor::start）发布 `CONTINUOUS_WORK_SECS`/`IS_DEEP_FOCUS` 全局 atomic；阈值 25min（计划 P14.3）。两生产点接真实值（`get_perception` + `check_proactive`，均按 `enable_window` 门控 #6）；消费端 `trigger_proactive` Rule1 现在真生效。DebugSnapshot + DebugPanel 加 Focus 分区（#11 可观测）。**lib 261（+6 focus 纯函数测）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D8。纯后端+前端，release 需 rebuild。
> - [x] **Item 3 推进 A2 Scheduler**（架构对齐版，**兑现 08-07 deferral ADR 留的"可观测/可扩展"开口，不引入被否决的 trait-Tick 多态**）：新 `lifecycle/scheduler.rs`——进程级注册表 `Vec<JobStat>`（11 任务：5 core aliveness 常开 + memory_decay/closeness_drift 常开 + 4 能力 reflection/consolidation/relationship_review/lifecycle_cleanup 可关），`record(name,enabled,status,msg)` 上报 ok/skipped/error（skipped 不盖时间戳）+ `snapshot()` 读出 + `should_run(flag)` 纯决策。`loop_runner` 全 11 个执行点接 `record`（medium: homeostasis/pending_check/emotion_push/presence_watch/lonely_nudge；slow: memory_decay/closeness_drift 永远 ok + cleanup/reflection/consolidation/review 按 config 门控）。`config [scheduler]` 加 4 个 enable flag（默认全 on，#6 优雅退化）。新命令 `get_scheduler_stats` + DebugPanel **Scheduler** 分区（11 行心跳：✅/⏭️/⚠️/⏸️ + 节拍 + 最近时刻 + 消息，#11 可观测）。新 ADR `2026-08-08-scheduler-observability.md` 取代旧 deferral（核心否决仍立，只补开放方向）。**lib 267（+6 scheduler 纯函数测）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D9。纯后端+前端，release 需 rebuild。
> - [x] **Item 4 Grounding B 档运行时阻断**：07-31 主动开口幻觉 A 档（prompt rule 8 软约束）已修，此为 B 档运行时后备。两段：① **`check_groundedness` 加中文 claim 模式**（你说过/你之前提到/你最喜欢…10 个高精度模式）——原 EN-only 对中文回复零命中，且修了**隐藏 panic**：`+40 字节`窗口尾在 CJK 多字节码点中间切片会崩，抽 `ceil_char_boundary` 步进到字符边界。② **运行时阻断**（仅非流式主动气泡）：新 `proactive::grounding_guard`——首遍 `check_groundedness` 标记 → 追加"这是编造，重说一句，不确定就只表达此刻感受"系统消息 `llm.chat` 重试一次 → 仍编造则**抑制**（None，不冒泡），用户永不见幻觉。三生成器（generate/generate_welcome_back/generate_lonely_bubble）同款尾部全接（replace_all）。**流式 chat 路径不守卫**（已流出的 token 无法撤回），其 grounding 保持 warn-only 可观测（Debug Panel grounding_violations）。**lib 270（+3 grounding 纯测：中文幻觉/中文 grounded/CJK 窗口不 panic）/ check --tests ✅**。→ 待实跑见 D10。纯后端，release 需 rebuild。
> - [x] **Item 5 推进 A1 全局 BrainState**：Task#9 的 ConverseCtx 统一了 converse 的*外*参（9→1）；本轮补*内*层——新 `mind/brain_state.rs::BrainState<'a>`（text/emotion/relationship/pending_due/retrieval 五借用字段，构造即指针拷贝零 clone），`planner::plan` 签名从 5 散参 → `&BrainState`（body 用 5 行别名桥接，字节不变），converse 构造一次 `brain` 传入。**采纳边界=planner**（旗舰纯决策）；prompt builder / budget allocator 各取子集，强制单一 mega-state 反而捆绑不需要的字段（项目已否决的投机抽象，见 §A2 ADR）→ 留干净 follow-up。**踩坑#4 命中并修**：改 plan 签名断 golden(7)+questioning(3) 共 10 harness 调用点 → 全包 `BrainState::new(...)`（两 harness 加 `use BrainState`）。planner.rs 4 import 降为 `#[cfg(test)]`（仅测试用）。**lib 270 / check --tests ✅ 无警告 / planner 11 单测全过**。纯重构无行为变化，无需手感验。
> - [x] **Item 6 personality_drift_score 语义版**：规则启发式层（GROSS 漂移：话痨/卖萌/依赖）只抓"明显的"，对"简短、无 emoji、却冷淡/粗暴"的语气漂移盲视。补**语义漂移层**（cosine over embeddings）：`evaluation.rs` 加 `LIRI_PERSONA_REFERENCE`（4 句典型璃语气，温柔/好奇/安静 archetypal）+ 纯 `cosine_similarity(a,b)`（f64 累积防精度流失、零向量兜底 0 非 NaN、mismatched 长度取 min）+ `semantic_drift_score`（cosine 经 `SEMANTIC_FLOOR=0.4` 映射 [0.4,0.95]→[0,1]，与规则层 overall 同标度）。**架构 #1 纯函数**：模块只做 cosine 数学、永不碰 embedding 模型/DB，调用方喂向量 → 合成向量单测 CI 跑（5 测：identity/orthogonal/zero-vector 不 NaN/monotonic/clamp）；真实 BGE-M3 由 `tests/evaluation.rs` 新 Layer 3 端到端测接（镜像 embedding_ab_harness 的 `EmbeddingService::new+load().expect` 模式）。**实跑信号确认**：on-persona「嗯，这么晚了。早点休息吧。」cosine **0.851** vs off-persona「行吧，随便你，我无所谓。」cosine **0.781**——两句**规则层都给 1.0（盲）**，语义层区分出 0.07 gap，断言 on>off 通过。**lib 275（+5 semantic 合成向量测）/ check --tests ✅ / `--test evaluation` 6 规则测 + 1 semantic E2E 实跑 ✅**。→ 待实跑见 D11。纯后端，release 需 rebuild。
> - [x] **砍掉走路相关计划 + 代码**：核验发现走路**不只是计划**——`src/animation/spatial.ts` + `App.tsx` 有正在运行的「走回窝」代码。AskUserQuestion 确认后代码一并砍。删 `spatial.ts` 整文件 + `App.tsx` 拆全部接线（import/spatialRef/实例化/setNest/物理循环走回块/isWalking state+className）+ `styles.css` 删 walking 规则；计划/设计文档（implementation-plan 12.2 整节 + Walk 状态 + walk.wav + FSM 图 + design 走路行）全标「已砍除 2026-08-08」移除。**tsc ✅ / vitest 24 ✅ / build ✅**。详见 §最近一轮 (2026-08-08) 走路砍除小节。release 需 rebuild。
> 详见批次末 §最近一轮 (2026-08-08) 汇总。

> **2026-08-08（续）自主推进中**（用户授权："2，3 按顺序跑，跑完用之前的策略——每项自主验证 + 更新 HANDOFF + 新增待测试 + commit，不报告不询问"。2=B5 语义评估深化[LLM-as-judge + ≥30 golden 集]，3=散落小项 + 架构债收尾）：
> - [x] **B5-深化 三层人格评估 benchmark**（B5 重第三线落地）：规则层(续⑧) + 语义 cosine 层(Item6) 是廉价可 CI 跑的两道线、各有盲区；补**重第三线 LLM-as-judge**——读人格圣经给 persona_fit 0-10 + 命名漂移维度，是唯一能抓「客服腔/鸡汤/动作描写」语气漂移的线。新 `tests/personality_judge_harness.rs`（永久评测资产）：`PERSONA_JUDGE_PROMPT`（璃 6 维度 + NOT 清单）+ `judge_persona`（`chat_reflection` 0.1/2048 踩坑#3 + JSON 提取 + **3 次指数退避重试**——30 连发撞 rate limit，无重试会静默零分"假通过"）+ 30 golden 集（On 10 / Gross 10[chatty×3/cloying×3/clingy×4] / Subtle 10[cold/mech×2/preachy×2/over_pos×2/action/套宠物]）+ 三层聚合断言 + **judge 可靠性闸**（失败>3 即 fail）。**实跑（全 30 真实评分 0 失败 65s）**：judge On **10.0** vs Gross **1.3** vs Subtle **2.0**；规则层对 Subtle **0/10 盲**、cosine 0.66 vs 0.59。**check --tests ✅ / 实跑 ✅**。→ 待实跑见 D12。**纯测试资产无生产变更，release 无需 rebuild**。详见 §最近一轮 (2026-08-08 续)。
> - [x] **3a Alt+Space 全局唤醒（P11.4）**：真·系统级全局快捷键——任何 app 前台按 Alt+Space 都把桌宠召出来对话。新依赖 `tauri-plugin-global-shortcut` v2.3.2 + `lib.rs` plugin（handler：`w.show()`+`set_focus()`+`emit("show-input")`）+ setup 里 `register(Shortcut::new(ALT, Space))`（失败仅 warn，非致命）；前端新 `show-input` listener（镜像 restore-from-tray：`setAwayMode(false)`+`setInputVisible(true)`+rAF focus 输入框）。**cargo check ✅ / tsc ✅ / lib 275 ✅**。→ 待实跑见 D13。**⚠️ 权衡**：Alt+Space 会**全局接管** Windows 窗口系统菜单键（键盘开 Move/Size/Minimize/Maximize 失效，所有窗口）——设计文档钦定此键，若嫌扰可在 setup 改 `Shortcut`。后端 `.register()` 是 Rust 直调不走 IPC，**无需 capabilities 权限**。release 需 rebuild（新依赖 + 前后端）。
> - [x] **3b 害羞慢现气泡（后端 mood 标签）**：设计 §6.3 把「害羞」列为情绪→气泡样式表里 开心/调皮/平静/难过/担心/疲惫 的同级条目（"慢慢浮现, 先半透明"），§6.2 又说低亲密度（陌生）→ 拘谨。**后端落 mood 标签**：`emotion/state.rs` 新 `derive_mood_label_with_closeness(state, closeness)`——以 `label_for_mood_full` 为单一真相源算 base label，再在 **closeness < `SHY_CLOSENESS_THRESHOLD=20.0`**（镜像 lonely-nudge / planner-Rule4 的 `closeness>=20` 门，取反）时把**中性/正向**标签（平静/开心/调皮）覆盖为「害羞」，但**不掩盖真实 distress**（担心/疲惫/难过照常——她和陌生人也会担心/累/难过）。**不改 `derive_mood_label` 签名**（踩坑#4：5 调用点 + 测试零波及，纯加法新 fn）。`converse.rs` 两处 emotion 落库点（silence:224 / normal:460）改调新 fn——closeness 从已读的 `relationship`（:176）取，标签写进 DB，loop_runner 30s 重发的是这份持久化标签，故害羞会自然驻留到下次对话。set_emotion 调试命令保留原 `derive_mood_label`（debug 覆写应字面）。**前端**：`bubbleClassForMood` 加 害羞→`bubble-shy`；`styles.css` 新 `bubble-shy` + `@keyframes bubble-shy-reveal`（1.2s 慢浮现，30% 处 opacity 0.35「先半透明」，终态 opacity 1 可读——对比 happy/playful 的 0.3s 弹出，shy 是迟疑试探的慢揭幕）。**lib 277（+2 shy 单测：低 closeness 中性→害羞 / 不掩盖 distress）/ check --tests ✅ / tsc ✅**。→ 待实跑见 D14。**纯后端标签 + 前端样式，release 需 rebuild**。详见 §最近一轮 (2026-08-08 续³)。
> - [x] **3c idle_weights JSON 化（数据驱动）**：`microBehavior.ts` 的 `IDLE_BEHAVIORS` 8 条微行为（weight/cooldown/emotion_modifier/min_closeness/sleepy）原本是硬编码 const 数组、数据和逻辑混在一个 .ts。抽成纯数据资产 `src/animation/idle-behaviors.json`，`microBehavior.ts` 改 `import ... from "./idle-behaviors.json"` + `as IdleBehavior[]` 类型断言（tsconfig `resolveJsonModule:true` 早开），`pickNextBehavior`/`applySleepyWeight` 逻辑零改动。**好处**：调权重/冷却/昼夜倍率只改 JSON 不碰逻辑（数据↔逻辑解耦，便于后续手感微调）。**纯前端行为不变重构**：vitest 24（含 7 microBehavior 测，A5 yawn/look_around 日夜比断言仍过——证 JSON 数据字节等价）/ tsc ✅ / vite build ✅（JSON import 打包正常）。**release 需 rebuild**（前端）。无需手感验收（代码层单测已覆盖，见 §最近一轮 续⁴）。
> - [x] **3d 架构债 BrainState 扩到 prompt builder+budget（B6 follow-up）—— 经评估主动关闭**（ADR: `docs/decisions/2026-08-08-brainstate-prompt-budget.md`）。Item 5 把 `BrainState` 采纳边界定在 planner，留此为"干净 follow-up"。复核五个目标函数（`build_system_prompt`/`build_qa_system_prompt`/`allocate_and_compress`/`allocate_qa`/`compress_system_prompt`）的实际签名与字段消费：① 它们都吃 `(retrieval, emotion, intent)`，而 **`intent` 是 planner 的 *输出***（`plan(&brain)→Intent`），不能入 BrainState（循环依赖）→ 强行扩留个 `(brain, intent)` 半 bundle 比现状更别扭，**省不掉 intent 参数**；② BrainState 的 `text`/`relationship`/`pending_due` 三字段这五个函数**一个不用** → 扩进去正是 `brain_state.rs` 注释 + §A2 ADR 已否决的「投机 mega-state」；③ 纯化妆重写 + 踩坑#4 级（5 函数签名 + 多 harness 调用点），零用户/正确性价值。**决策：不扩，follow-up 关闭，采纳边界终态=planner。** 也评估了方案 B（窄类型 `PromptCtx{retrieval,emotion,intent}`）：比方案 A 干净但不捆绑问题、边际收益不抵新类型 + 截断 retrieval 碍事，现状 3 参紧签名已自解释。同步更新 `brain_state.rs` 顶部注释指向 ADR。**纯决策无代码行为变更**，无需 rebuild。详见 §最近一轮 (2026-08-08 续⁵)。
> - [x] **批次末 rebuild release exe**：3a/3b/3c 改了前后端 → `npx tauri build --no-bundle`（踩坑#6，先确认 desktop-pet.exe 未运行）。**exit 0**，产物 `D:\cargo-target\desktop-pet\release\desktop-pet.exe`（11:40:31 新鲜，51.8s Rust release + 2.1s 前端，CSS hash `index-HCg0t6XF.css` 含新 bubble-shy）。桌面快捷方式同路径免改。**3a-d 全部完成 + release 已重建 = 本批（2，3）收尾。** 待用户实跑 D12-D14（B5 benchmark / Alt+Space / 害羞气泡）。

> **2026-08-08（续⁶）真人感 prompt 调教（用户驱动，已收尾）**：用户"回复不够真人感、不需要每问都加提问"。四步闭环：① `client.rs` `thinking:{type:disabled}` 关 gate/extractor 思考（提速+根治空 content 踩坑#3，commit 8aa0d61）② harness 扩到 150 例+真人感指标+`CASE_FILTER`（eec094c）③ 基线 150 条诊断：提问结尾率 35%，G12分享 80%/G11琐碎 60%/G3闲聊 50% 严重超标，G5 喜讯"哇"克隆开场 5/10 ④ 改 prompt A/B/C（b5afac6）：system.txt 话术 engage"可不问"+4 条反 AI 味（禁客服收尾/禁情绪标签/允许自己的状态/像随手发消息）；样例 4→6 条仅 1 问；`grounding.rs` format_intent engage"then ask ONE"→"may ask ONE… often no question"。**复测**：提问结尾率 35%→**14%**，G3 50%→10%、G12 80%→30%（−50pp）、G5 哇开场 5/10→0/10、"想听细一点的我可以再讲"消失。**诚实权衡**：human_like 4.24→4.11（judge 一致"稍显简短"=变短非变冷）；模板词 23→23（构成迁移哇→恭喜，喜讯道恭喜属正常非 AI 味）；G14 碎念残留 40% 提问皆对天然邀请追问的输入（在吗在吗/啊啊啊），压低反损自然。**对比报告** `docs/review/realism-report-2026-08-08.md`；评测快照 `-baseline`/`-post`。**release exe 已 rebuild 17:48**。→ 待用户实跑验收手感；若嫌 G5 偏冷可微调 A3"一个字"措辞（见报告可选微调）。

> **2026-08-07 自主批次推进中**（用户授权长程自主："按优先级推进所有后续内容，每项自测后更新 HANDOFF，不询问；待实跑项统一整理"）。逐项推进，每项自测（cargo test --lib / check --tests / tsc）绿后勾选。**release exe 在批次末统一 rebuild**（中间项都以库单测 + check 编译通过为正确性证据；批次末 Task #14 前一次性 `npx tauri build --no-bundle`，避免每项重构都重编一次前端嵌入）：
> - [x] **Task #8 鲁棒性加固**：① main 空回复重试——converse `chat_stream` 把 `on_token` 改 `mut`、传 `&mut on_token` 复用，content 空时重试一次（镜像 extractor 重试；flash reasoning 吃光预算 finish_reason=length 空 content 的坑#3 瞬态）。② harness 启发式误报——Acknowledge/ForgetAck 关键词表加现实同义措辞（记着/记心里/放心吧/帮你记 + 不提/不会再/抹掉/清掉），治 705/1002「语义对无关键词」误报。**lib 259 / check --tests ✅**。纯后端 + 测试，release 需 rebuild。
> - [x] **Task #9 B6 BrainState**：converse 9 参 → `ConverseCtx<'a>` 统一快照（8 个引用字段 + `on_token` 留作独立泛型 `FnMut`——回调是流式旁路非状态，塞进 struct 会让整体变泛型）。函数体用 8 行别名桥接（`let text = ctx.text;`…），400 行 body 字节不变，最低风险。6 处调用全改：commands.rs（生产）+ memory_recall(×3)/conversation_harness/prompt_quality_harness。harness 里的 `get_context()` 临时 Vec 绑定本地避免跨 await 临时生命周期问题。**check --tests ✅ + lib 259 ✅**。纯机械包装，行为不变。
> - [x] **Task #10 B7 Scheduler —— 经评估主动搁置**（ADR: `docs/decisions/2026-08-07-scheduler-deferred.md`）。原计划 §A2 假设 Body 跑在 Rust（`ticks_1s` 动画/物理），但实际遵循原则 #5：Body 在前端，Rust 无 1s 动画 tick。审计 Rust 定时器仅 medium(30s)/slow(1h)/cursor(ms 感知)/两个 one-shot 启动——`start_life_loop` 已是唯一注册中心，无多态无注入需求，引入 trait object 是投机抽象（#9/#10）。高风险重写时序核心、零用户价值。搁置，何时复议见 ADR。
> - [x] **Task #11 记忆可视化编辑**：Debug Panel 从只读→可编辑。后端 3 新命令（复用既有 DB accessor，不写裸 SQL）：`forget_fact(id)`（`facts::expire_by_id` 软删，保审计轨/revive 路径）、`delete_episode(id)`（`episodes::delete` + `vectors::delete` 同步向量，拒删地标）、`set_emotion(EmotionEdit)`（`update_fields` + 重导 mood_label + 即时 emit `emotion-update` 让脸马上变）。pending 取消复用既有 `resolve_pending_event`（不另起路径）。`DebugFact` 加 `id` 字段。前端：Facts/Episodes/Pending 每行 ✕ 按钮（fact/episode 带 confirm 防误删）+ Emotion 编辑器（5 滑块 Apply）。2s 轮询 + mutate 后即时 refresh。**check --lib ✅ + lib 259 ✅ + tsc ✅**。→ 待实跑：F12 打开面板手动测编辑（见 verify-checklist）。
> - [x] **Task #12 loneliness 收尾**：① lonely-nudge 加 Sleeping 守卫——`App.tsx` 监听器加 `if (fsmRef.state===Sleeping) return`（镜像"该睡了"nudge 的同款守卫，睡着不冒"想你了"，原则 #12）。② `pet_head` 互动降孤独 -0.1（摸头是注意力的反面=孤独缓解；poke 是逗弄不减；~0.1 抵 15min idle 增长，一次摸头明显安慰但不让缓慢累积失效）。**tsc ✅ + check --lib ✅**。→ 待实跑：深夜 Sleeping 时确认不冒 lonely 气泡 + 摸头后 loneliness 回落（见 verify-checklist）。
> - [x] **Task #13 死代码清理**（核实后修正前提）：① **`trigger_proactive` 并非死代码**——`commands.rs:451` 生产调用它（前次"6 调用全测试"的判断过时/错误），**保留不动**。② **删 `emotion/homeostasis.rs` 整文件**（`apply_drift`+私有常量+`drift_toward`+4 测试）——生产用 `db::emotion::apply_homeostasis_time_aware` 自带一套 `TAU_*`/`drift_toward`，homeostasis.rs 全程零生产调用；**且其 `TAU_STRESS=3600` 与生产 `7200` 已分叉，留着会误导**（典型双实现坑）。同步删 `emotion/mod.rs` 的 `mod`/`pub use` + golden `GC_018`。③ **`tick_needs` 保留**——虽是测试专用包装，但正确委托给生产用的纯函数（不分叉、不误导），删它低价值中风险（需改写 needs.rs 共享文件的测试），留 + 注释说明。**check --tests ✅ + lib 255（原 259 −4 homeostasis 测试）✅**。
> - [x] **Task #14 统一待实跑清单**：扩写 `docs/verify-checklist.md`（原有 Body/circadian/sleep A5/A4/B3/A6 不动），新增「本批次验收」一节 D1-D7：D1 Debug Panel 记忆编辑（forget fact/delete episode/cancel pending/emotion 滑块）、D2 loneliness 主动找你、D3 loneliness 睡着抑制、D4 摸头降孤独、D5 Forget 流程、D6 QA 直答、D7 rest_need 疲惫眼——全部用本批次新增的 **Emotion 编辑器秒级触发**（原本需等几小时）。附「不易快速验收」表（关系 review/空回复重试/surfaced thought/B6 重构）。顺带给 Debug Panel Brain 行加 Lonely 显示（D2/D4 观察 loneliness 用）。**tsc ✅**。交付：用户照此清单 dev 模式手动验手感。
> 详见各任务 §最近一轮 条目（批次末汇总）。

> **2026-08-07（续）更新 · 激活 loneliness——璃会"想你"**：用户"读 handoff、用 codegraph 了解代码、继续开发"。AskUserQuestion 在 4 方向里确认走 **激活 loneliness**（服务"陪伴"北极星，未受阻低风险）。codegraph 核验发现 **loneliness 是最后一个死情绪字段**——`apply_homeostasis_time_aware`（生产 homeostasis）只更新 mood/energy/social/stress/rest_need，从不更新 loneliness（08-04 修了 rest_need，loneliness 漏了），冻结在种子值 → planner Rule 4「loneliness>0.6 + closeness≥20 → 主动陪伴」永远到不了。两段落地：① **核心（镜像 rest_need 修法）**——`needs.rs` 抽 `tick_loneliness` 纯增长规则 + 接进 `tick_needs`（DRY）；`apply_homeostasis_time_aware` 调它 + SQL UPDATE 加 `loneliness=?7`（renumber ?8）；② **主动气泡（镜像 welcome-back/proactive 模式）**——新 `generate_lonely_bubble`（镜像 generate_welcome_back：retrieve 锚 + Intent goal=accompany/action=lonely_nudge + 1 句温柔 prompt「别黏人别问问题逼答」+ LLM 4096 坑#3）+ `lonely_canned`（react.rs mood 分档降级 #8）+ `lonely_bubble` 命令 + 注册；`loop_runner::check_lonely_nudge`（门控 loneliness>0.6 + closeness≥20 + presence Active + 非对话中 + 30min 线程本地 cooldown → emit "lonely-nudge"）；App.tsx listener → invoke → showBubble。**closeness≥20 门控保证早期关系不主动找你**（Liri 非依赖人格安全阀）。**全程不改 fn 签名**（踩坑#4：新 fn + 新 action 字符串 + SQL 参数）。**lib 259 / check --tests / tsc / build / vitest 24 全绿**。**待实跑**：dev 攒 closeness≥20 + 离开 ~1.7h（loneliness 到 0.6）→ 看她主动冒"想你了"气泡；或回来后她回复带 accompany 暖意（planner Rule 4）。**release exe 已重建**（npx tauri build --no-bundle，D:\cargo-target\desktop-pet\release\desktop-pet.exe，前端+后端都改）。详见 §最近一轮 (2026-08-07 续)。**当前无进行中任务**。

> **2026-08-07 更新 · 关系进展摘要（Hermes 后台 review）落地**：用户"读 handoff、用 codegraph 了解代码、继续开发"。AskUserQuestion 在 4 个方向（关系进展摘要 / 激活 loneliness / 记忆可视化编辑 / 架构债 BrainState）里确认走**关系进展摘要**（服务"懂你"Soul 闭环深化）。每 15 个新 conversation episode，后台 reflection 模型回顾产出 1-2 句"你们关系最近状态"总结（璃视角、free text），注入为 always-on `[Relationship]` 区块——让她即使当前话题检索不到相关记忆，也带着对关系整体的理解。**3 新文件 + 6 改文件，全程不改 fn 签名（踩坑#4）**：新表 `relationship_reviews`（migration v3 + `db/relationship_reviews.rs`）+ 新 `soul/review.rs`（镜像 reflection.rs：纯谓词 `should_run_review` + `run_review` + `maybe_run_review_if_due`）+ RetrievalResult 加 `relationship_review` 字段走现成注入管道（`retrieve` 填充 → `format_memories` 输出 `[Relationship]`）+ slow_tick 调度 + budget RELATIONSHIP=80 + system.txt 指引。**踩坑#4 变体已修**：RetrievalResult 加字段后同步所有显式构造点（lib retrieval/budget×4/grounding×2/planner×2 + harness golden×7/evaluation/questioning；converse 用 `::default()` 自动 None）。**lib 257 / golden 30 / evaluation 6 全绿，check --tests ✅**。**待实跑**：dev 攒≥15 记忆后 slow_tick 触发 → DB 看 `relationship_reviews` 有行 + 对话语气带关系理解。**release exe 需 `npx tauri build --no-bundle`**（system.txt include_str! + 后端 + migration v3）。详见 §最近一轮 (2026-08-07)。**当前无进行中任务**。

> **2026-08-05（续⑤）更新 · 100 条提示词质量评测 4 轮迭代完成（98/100 通过，0 真乱扯）**：用户"自己写一套测试，100 条对话多方面测试提示词回复质量，汇总表格审查"。新增 `tests/prompt_quality_harness.rs`（**永久性评测资产**，100 条 × 10 组：G1 知识/G2 技术/G3 闲聊/G4 情绪/G5 喜讯/G6 记忆(种子DB)/G7 提醒/G8 边界/G9 关系/G10 修正遗忘；走完整 converse 链路 + 启发式硬检查 + LLM-as-judge 评分，写报告 `docs/review/prompt-quality-report-YYYY-MM-DD.md`）。**4 轮迭代修复链**（每轮 100 条实跑验证）：R1 发现 extractor 空输出整轮崩（4/100）→ 修 extractor 重试+降级；R2 发现 gate/correction 类别空洞 reasoning 爆预算（gate.txt 排除规则副作用）→ 修 gate/correction 重试+降级 + gate.txt 给排除项明确归宿 store_full + QA 模式加防编造句；R3 发现 extractor 算错日期（"明天"→2026-01-02）→ 修 extractor 注入本地今天日期+星期（{today} 占位，不改签名）；R4 启发式调优（合理澄清反问不再误报）。**最终：98/100 硬检查通过，0 真乱扯，知识问答 20/20 满分直答，记忆组 10/10 引用（"你记得我在忙什么吗"→"记得，你在找实习"），日期全对（下周二→2026-08-11）。** 剩余 2 fail 均启发式误报（705 语义已确认但无关键字 / 1002 同上）+ 1 偶发空回复（407，1/100 LLM 空输出）。judge 标"幻觉"3 条全为不知种子/注入机制的误判。**lib 248 passed**。**release 已重建**（本轮改动 gate/extractor/correction/gate.txt/extractor.txt 均 include_str! 或后端）。待办：407 类 main 空回复可加重试（低优先）。

> **2026-08-05 更新 · 选择性遗忘扩展至 fact/pending + FTS5 可行性证伪**：用户"读 handoff、用 codegraph 了解代码、按优先级继续开发"。**① FTS5 证伪（决定性）**：HANDOFF 把 FTS5 全历史检索标为"最高 ROI follow-up"。写 throwaway probe 测 bundled SQLite 三分词器对中文 2 字查询 '火锅' 的 MATCH——**FTS5 可用但 trigram/unicode61/ascii match count 全 0**（trigram 需≥3 字 / unicode61 不分 CJK / ascii 只认 ASCII；旧记"sqlite-vec 自带 fts5_cjk"**错误**——fts5_cjk 非标准、sqlite-vec 不捆绑 FTS5 分词器）→ **FTS5 对中文不可行，从 backlog 移除，勿再尝试**（除非引入 jieba 可加载扩展 / Rust 分词，远超干净 follow-up）。**② 转向选择性遗忘 fact+pending**（08-04 续 episode MVP 的 deferred scope："fact/pending 遗忘未做"，结构镜像 episode）。新 `forget_best_match` 调度器扫 episode/fact/pending 三路、各自 0.7 置信度门、取最高分执行一条（episode 硬删+向量清 / fact 软过期 `expire_by_id` / pending `mark_resolved`）；用户不说忘哪种 → 扫三种挑最佳；歧义时软动作（fact 过期可恢复）自然压过硬删。新 `char_overlap`（bigram 重叠系数 `|A∩B|/min`，修 Jaccard 把"忘掉咖啡"/"咖啡"稀释到 0.33 的问题→1.0）。**验证全绿**：lib **247 passed**（240+7）/ `cargo check --tests` ✅。**待实跑**：dev "忘掉X"（X=偏好/提醒）→ 确认回"好，我忘了"+ 后续不召回（Debug Panel 看 fact valid_to / pending status）。**release exe 需 `npx tauri build --no-bundle`**（纯后端 + gate.txt include_str!）。详见 §最近一轮 (2026-08-05)。

> **2026-08-04（续④）更新 · 审查并修复 opencode 续③ QA 直答代码的 4 处问题**：用户"代码库新增的是 opencode 写的，针对回复没逻辑的问题，看看 handoff 检查代码"。审 opencode 续③（QA 直答路由 + Hermes compress_conversation + Milestones）后发现并全修：① **[中] QA 模式丢失身份层**——`converse` qa_mode 用 `RetrievalResult::default()` 把 persona/relationship/user_profile 连同记忆一起丢了 → `build_qa_system_prompt` 的 `[Persona]` 退化为通用 fallback，璃的知识直答叫不出用户名字/丢关系。修：qa_mode 仍跳过 episodes/facts（防跑偏），但**补加身份 DB 读**（persona/relationship/user_profile，廉价无 embedding）→ 直答保留璃身份。② **[小] QA Debug budget 错**——prompt_debug 用正常 budget(2005)，但 QA 无 [Memories]。修：新 `qa_system_prompt_budget()=505`（PERSONA+EMOTION+INTENT+SCAFFOLD），qa_mode 用它。③ **[小] qa_mode 未强制 action**——罕见 planner silence 会吞掉问题答案。修：qa_mode `intent.action="normal"`（问题必答）。④ **[小] QA 仍跑 grounding check**——空 retrieval 只会误报。修：qa_mode 跳过 check_groundedness。**确认无问题的部分**：compress_conversation 重写逻辑正确（user 永留/驱逐最老 assistant/时序复原）、gate 4096（坑#3 已修）、Question 跳 extractor 合理。**验证**：lib **240 passed**（238+2 新：qa budget 值 + QA 保留身份）/ `cargo check --tests` ✅。**待实跑**：dev 问知识题确认璃叫得出你名字（fix#1）。**release exe 已重建**（`D:\cargo-target\desktop-pet\release\desktop-pet.exe`，08/04 22:30，`npx tauri build --no-bundle`，含本会话全部改动：#10 rest_need/speedModifier + 选择性遗忘 + QA 4 修复；桌面快捷方式同路径免改）。

> **2026-08-04（续③）更新 · QA 直答路由 + 提示词正向重写 + Hermes 记忆优化落地**：用户反馈知识问答体验差（"harness 是什么"被硬套宠物话题、回复生硬）。三部分完成：
> ① **Question 直答路由**（治"硬套"）：gate 新增 `question` 分类（gate.txt + `GateRoute::Question`）→ ingest 跳过 extractor（省一次 LLM 调用）→ converse QA 模式：跳过记忆检索（RetrievalResult::default()）、清空 intent memory anchor/engage 指令、跳过 pacing、跳过念头注入 → 新 `build_qa_system_prompt`（人格+情绪+中文直答指令，**无 [Memories]/[Grounding Constraint]**）+ `budget::allocate_qa`（QA 版 allocate_and_compress，签名不动避坑#4）。
> ② **system.txt 正向重写 + mes_example**（治"生硬"）：14 条禁令清单 → `[How to talk]` 正向说话方式 + **4 条中文示例对话**（知识直答/分享跟进/记忆自然引用/闲聊）。保留 persona 契约回归网（evaluation.rs）全部字样：6 维人格/话痨卖萌依赖/严禁编造/璃。改了一个 stale 断言：`test_empty_memories_section` 的 `[Memories]` 检查改 `- [Fact]`（system.txt 正文现在也提标签字样）。
> ③ **Hermes agent 落地**（调研 NousResearch/hermes-agent 225k⭐，记忆最佳实践）：**用户消息永不压缩**（`compress_conversation` 重写——user 消息 verbatim 全保留、超预算先挤 assistant 回复，修"用户倾诉被截断失真"）+ **关系账 [Milestones] 分组**（landmark episode 单独区块注入、不重复进 [Memories]，Hermes 双账本思想适配陪伴场景）。其余 Hermes 优化已天然满足（压缩/辅助走 flash、consolidation 容量跳过重试）或记 follow-up（FTS5 全历史检索、关系进展摘要、记忆可视化编辑）。
> **会话前半段**：Debug Panel 退出通道（面板全窗口覆盖挡住右键 → 加粘性工具栏 ✕关闭面板/⏻退出桌宠，走 handleQuit→quit_app）+ 快捷键重构（新 `src/shortcuts.ts`：`e.code==="KeyD"` 防中文输入法截获 key="Process"、Esc 无条件关面板）+ gate/correction `max_tokens` 2048→4096（踩坑#3 复发：flash reasoning 吃光 2048 预算 content 空 JSON 崩）+ 主对话模型切 `deepseek-v4-flash`（AppData config）。
> 验证：lib **238 passed** / golden 30 / harness 编译 ✅ / 前端 tsc ✅。**release 已重建**（npx tauri build --no-bundle，exe 18:07→最新，桌面快捷方式无需动）。

> 📋 **待办（下一会话起点）· QA/新提示词 runtime 实跑**：① `npm run tauri dev` 问知识问题（"什么是X"/"帮我解释报错"）→ 确认直答不套宠物、F12 面板 Last Turn 显示 route=question；② 分享类消息（"我今天…"）确认示例风格生效（简短+一个真问题）；③ 聊天几次后确认旧记忆仍自然引用（[Milestones] 里程碑出现）。**Hermes 高价值 follow-up**：FTS5 全历史检索（零成本毫秒级回忆，sqlite-vec 库自带 fts5_cjk 中文分词，替代部分 embedding 召回）、"关系进展摘要"（后台每 N 次对话异步总结，对应 Hermes 后台 review）、记忆可视化编辑（Debug Panel 只读→可改）。

> **2026-08-04（续）更新 · 选择性遗忘 episode MVP**：用户"开做选择性遗忘，做完跑 50 条功能测试，遇问题自检修复"。实现**用户主导的主动遗忘**（lifecycle_cleanup 的用户控制版）：用户说"忘掉X"→ gate 路由 `Forget` → 复用 retrieve 语义匹配最佳 episode → **置信度门在 `score_breakdown.semantic`（0.7，非 total score——total 混了 strength/recency，强近期无关记忆也能高分→删错）** + landmark 保护 → Rust 删 episode 行 + `vectors::delete` → converse 注入确认提示（"好，我忘了"，**禁复述**；无匹配则诚实"不记得"）。新 `mind/forget.rs` 模块（镜像 correction.rs）+ `db/episodes::delete`（保护 landmark）+ gate Forget 变体 + gate.txt 类别 + IngestionOutcome 加 `forget` 字段 + converse 提示。**全程不改 fn 签名**（踩坑#4：只加枚举变体 + struct 字段 + 内联分支）。**8 新单测**全绿。**自愈**：跑 golden 时 C 盘满（0.5GB，os error 112）→ 诊断 `src-tauri/target/release` 是 07/28 陈旧残留（release 早走 D 盘，活动 exe 在 D 08/03）→ 删之腾 2.31GB → golden 增量编译过。详见 §最近一轮 (2026-08-04 续)。

> 📋 **待办（下一会话起点）· 选择性遗忘 runtime + 扩展**：① **runtime 实跑**：`npm run tauri dev` 攒几条记忆后说"忘掉我说的X"→ 确认她回"好，我忘了"且后续不再召回（Debug Panel 看 episode 删了没）。② **MVP 边界（可选 follow-up）**：当前只删 top-1 episode、阈值 0.7 需真实样本调、无多轮消歧义（低置信直接"不记得"而非反问"你说的是…"）、fact/pending 遗忘未做。详见 §最近一轮。

：用户选"#10 生命感收尾"方向（非字面最高优先的 B6/B7 架构债——那是对运行中代码的推测性重构，违反"不重构没坏的东西"）。本轮补全两个长期标"低优先/follow-up"但服务北极星#10、且补全**已半接线系统**的缺口。**① rest_need 后端暴露+激活**——审计发现 `tick_needs`/`apply_drift`（emotion/needs.rs、homeostasis.rs）**只在自身测试里被调用、生产从未调**（生产走 DB 层 `apply_homeostasis_time_aware`，只漂移 mood/energy/social/stress，从不碰 rest_need）→ 单纯"暴露"会显示恒定种子值、毫无效果。故同时激活：新 `tick_rest_need(r,e,t)` 纯函数（低能量增长 + **恢复项 exp 衰减**，修原 tick_needs 单调只增永不恢复的设计缺陷）+ `tick_needs` 复用它 + 接进 `apply_homeostasis_time_aware`（UPDATE 加 rest_need 列）+ `EmotionResponse`/From/emit 三处加字段 + 前端 `EmotionData`/`toEmotionVector` 读取。效果：低能量时 rest_need 增长 → emotionDriver 半眯眼真的可见（之前恒 0）。**② circadian speedModifier 接动画速度**——`circadian.ts` 早输出 speedModifier（Morning 1.2 / DeepNight 0.4）但**零消费方**（只有 sleepiness 喂了 fsm）。Live2DCanvas 加 `speedModifier` prop → per-frame `focusTickerFn` 设 `app.ticker.speed` → 库的 idle 呼吸/眨眼/motion/physics 全局随昼夜变速（深夜真的变慢）。**验证全绿**：lib **227 passed**（226+1 恢复测试）/ `cargo check --tests` ✅ / `tsc` exit 0 / `vitest` 24 / `build` ✅（2.60s）。**待实跑**：dev 看 ① 低能量半眯眼（需攒状态或 CDP 注入 high rest_need）② 深夜 ticker.speed=0.4 全局变慢（`__pet.setHour(3)` 即时切换）。详见 §最近一轮 (2026-08-04)。**release exe 需 `npx tauri build --no-bundle` 才生效**（前端+后端都改了）。**当前无进行中任务**。

> 📋 **待办（下一会话起点）· runtime 实跑 #10 两项**：`npm run tauri dev` → ① 低能量半眯眼：Debug Panel 或 CDP 把 rest_need 拉高，肉眼确认眼睛半闭（emotionDriver EYE_REST_GAIN 生效）② 深夜变慢：`__pet.setHour(3)`（dev-only 钩子，重写 getHours 模拟 DeepNight）→ 观察呼吸/眨眼/motion 明显变慢（ticker.speed=0.4），`setHour(10)`（Morning）→ 略快。静态全过，仅剩渲染确认。验完勾掉。

：用户"继续 B4,B5 推进"。**B4-余余 两分区补全**（#11 Explainability 收尾）：① **AnimFSM**——fsm.ts 加 `getHistory()` getter 暴露末 5 微行为 history；App 传 `anim={state:behavior, history}` 给 DebugPanel；新 AnimFSM 分区显示当前态+recent history（"她现在在干嘛"）② **Prompt-token**——budget.rs 加 `system_prompt_budget()`（=2005）；converse 加 `PromptTokenDebug{system_tokens,input_tokens,budget,conversation_turns}` 挂 ConversationResult（**续③ 同款不改 fn 签名**，silence=None/normal=Some，在既有 system_tokens log 处复算）；commands 镜像 `DecisionPromptToken` 投影进 DecisionTrace；DebugPanel Last Turn 加 "Prompt: sys N/budget M | input K (N turns)"。**B5 Golden 评估框架**（审计确认原无 evaluation.rs/personality_drift_score/CI）：新 `src/mind/evaluation.rs`（DriftKind Chatty/Cloying/Clingy + DriftReport + `personality_drift_score` 规则启发式 + 7 单测）+ `tests/evaluation.rs`（**Liri 人格契约回归网** 4 测：6 维度/狐灵身份/NOT-list/严禁编造，锁续② 落地的人格 + 2 drift 端到端）。**验证全绿**：lib **226 passed**（219+7 eval）/ `cargo check --tests` ✅（evaluation.rs 编译 + 既有 harness 无破）/ `--test evaluation` 6 passed / `tsc` ✅ / `vitest` 24 / `build` ✅（1.89s）。**B4 前端两分区待 dev 实跑确认渲染**（静态全过；要看 AnimFSM/Prompt 分区需 `npm run tauri dev` 发消息开 Debug Panel）。详见 §最近一轮 续⑧。
> 📋 **待办（下一会话起点）· B4 两分区 runtime 实跑**：`npm run tauri dev` 发一条消息 → F12（或 Ctrl+Shift+D）开 Debug Panel → 肉眼确认 ① **AnimFSM** 分区显当前 state + recent history ② **Last Turn** 内显 `Prompt: sys N/budget M tok | input K (N turns)`。静态全过（compile/types/build/单测），仅剩渲染确认；后端 PromptToken→snapshot 链路续③ 已验活着。验完勾掉。

> **2026-08-03（续⑦）更新 · sleep 内容首次有测试**：用户"sleep相关的内容是不是还没有做测试"——确认 A4/A5/B3 全标"待实跑"、**前端零测试**（Rust 219 vs 前端 0）。补：① **加 vitest**（devDep + vitest.config.ts，node env，`npm test`/`test:watch`）② **抽纯逻辑**——`sleepLogic.ts::shouldAutoSleep`（从 App.tsx auto-sleep 条件抽出 A4 触发谓词）+ `microBehavior.ts::applySleepyWeight`（A5 公式 `w*=1+(sleepy-1)*sleepiness` 抽出，pickNextBehavior 复用）③ **24 前端单测**：circadian(10)/sleepLogic(7)/microBehavior(7)，覆盖 A5 输入（DeepNight 0.9/Morning 0.1 + 5 时段 + 边界）、A4 触发（DeepNight-only/非已睡/非 think-talk/idle 严 >阈值 各分支）、A5 效果（yawn 夜↑~3×/look_around 夜↓/白天 no-op/clamp）。**验证**：`npx vitest run` **24 passed** / `tsc --noEmit` ✅ / `npm run build` ✅（1.97s）。详见 §最近一轮 续⑦。**+ runtime CDP 验证（同轮）**：`npm run tauri dev` + `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` → Node WebSocket 连 CDP 驱动 `window.__pet`——A5 setHour(3)→DeepNight/0.9 & setHour(10)→Morning/0.1 ✅、A4 sleep()→"sleeping" & wake()→"hum" ✅、B3① 睡着 probeNudge×10 零气泡 ✅（awake sanity 1/15 证 nudge 本身没坏）、截图分析 sleeping=**闭眼**/awake=**睁眼** ✅。**唯一仍待人验：B3② sleep 音效**（`sound.sleep()` 入睡播放，我听不到——dev 仍开着，可右键→DevTools→`__pet.sleep()` 亲耳听）。**当前无进行中任务**。

> **2026-08-03（续⑥）更新 · 清测试**：用户"是不是还有几个测试没做？先清测试"。**发现 `cargo test --test golden_conversations` 有 2 个 stale 测试失败**（lib 219 一直绿，掩盖了集成测试回归）：① **gc_003_emotion_consistency**——断言"高 stress+焦虑→silence"，但 planner Rule 2 **故意改成** anxiety→`care/normal/gentle`（打破 焦虑→stress→silence 反馈环；planner.rs:112-127 注释详述 + 单测 `test_anxiety_routes_to_care` 钉契约；golden 测试漏同步）② **gc_012_first_run_seeds_persona**——断言 `trait_key=="gentle"`，但 续② Liri 迁移后 `seed_persona` 播种中文维度 `温柔`（firstrun.rs:32-39）。**两者均改测试不改生产**（生产是有意行为，测试 stale）。修复后 golden **30 passed** ✅。**全确定性测试绿**：lib 219 + golden 30 + questioning pacing 1 + embedding_ab 1 = **251**。**闭环1 `memory_recall` real-LLM 复验通过**（多文件 Rust 改动后确认核心链路：seed 持久化→noise 抑制→跨会话 recall "糯米" 全 ✅，46s）。详见 §最近一轮 续⑥。**当前无进行中任务**，下一会话按 B4-余余（AnimFSM 前端 / Prompt 动态 token）→ B5（Golden 评估，待 Liri 稳定）推进。

> **2026-08-03（续⑤）更新 · 两 follow-up**：① Settings 下载按钮 Qdrant 401→Xenova（hf-mirror）+ REQUIRED_FILES 加 `model.onnx_data` + download_all 处理 `onnx/` 子目录 external data；② **暂时离开 = 最小化到系统托盘**（lib.rs 建 TrayIcon 左键 click 恢复 + emit restore-from-tray；commands `hide_to_tray`；App handleAwayMode 调 hide + listener 清 awayMode；**原只设 awayMode 标志窗口根本没隐藏**）。tray-icon feature 早启用只差接线。验证：219 passed + tsc + check --tests + release rebuild(19:20) + 启动 sanity（进程活+vectors14 幂等）。**tray 交互（右键离开→托盘→点击恢复）+ 下载按钮待用户手动验证**（OS 层）。详见 §最近一轮 续⑤。

> **2026-08-03（续④）更新 · embedding 接入 + 检索质量翻倍**：用户"下载 embedding 装 D 盘 + 对比测试看提升"。已完成：① BGE-M3 装入 `D:\models\bge-m3`（Xenova/bge-m3；原 Qdrant/bge-m3-onnx 已 401）② config→D 盘 ③ **修 ort rc.12 加载 bug**（Level3→All；记忆 `ort-rc12-embedding-load-bug`）④ **加 backfill**（历史 episode 自动向量化，真实 DB 0→14 验证）⑤ benchmark（语义 Hit@3 33%→67%、avg sem 0.035→0.741≈21×）⑥ release rebuild + 端到端。详见 §最近一轮 续④ + 记忆 `bge-m3-model-location`。**当前无进行中任务**。**follow-up**：download.rs 的 HF_BASE_URL 仍指失效 Qdrant（Settings 下载按钮坏，手动下载已绕过）；B5 Golden 评估。

> **2026-08-03（续③）更新 · 深度审计 + #11 可观测簇**：用户要求"对计划未完成部分做深度审计 + 列优先级更新 HANDOFF + 按优先级继续开发"。审计对照 `implementation-plan.md` P0-P17/A1-A2 逐项核验**代码实际状态**（非仅信 HANDOFF 旧记录），结论见 [§审计 (2026-08-03 续③)](#审计-2026-08-03-续③深度审计--代码级核验)。**重排结论**：三闭环已全完成、生命感主轴在维护态，**Liri/Spine（真正的 #10 下一步）受阻于 Liri.spine 资产未交付** → 当前最高 ROI 且**未受阻**的是 **#11 Explainability 簇**：① **B4b conversations 死表（确认真 Bug）**——生产路径从未调用 `conversations::insert`（grep 0 命中 / callers 仅测试），07-31 幻觉因此无法回溯她原话；② **B4 Debug Panel 缺 5/9 分区**——Retrieved/Intent/Reflect 是"她为什么这么说"诊断链的核心，07-31 幻觉若有这些早定位了。本轮**已完成** **B4b（死表修复）→ B4-MVP（Retrieved+Intent+Reflect 三分区）→ B4-余 Cost（LlmClient 今日调用+token 计数）**——`cargo test --lib` **219 passed** / `cargo check --tests` ✅ / `tsc --noEmit` ✅ / `npm run build` ✅（详见 §最近一轮 续③）；AnimFSM(前端 fsm 上抛)/Prompt-动态 token 留 follow-up。**B1b 推迟**（07-31 A 档后无复发，条件触发）。**B6/B7 推迟**（在跑的架构债，重构风险高，非用户态）。**✅ 已实跑确认（2026-08-03 dev 真实 LLM）**：发一句话 → Ctrl+Shift+D（笔电 F12 被绑休眠，本轮加的备用键）看 Last Turn/Retrieved/Reflect/Cost 四分区全有值；conversations 0→2 行；**Cost 首次暴露单轮 3 次 LLM 调用（gate+extractor+main）**；Retrieved 的 sem≈0 暴露当前环境未加载 embedding（既有问题）。详见 §最近一轮 续③。release exe 需 `npx tauri build --no-bundle` 才生效。
> **2026-08-03（续②）更新 · 重大方向**：用户确认最终角色 = **璃 Liri（小狐灵）**，动画走 **Spine+PixiJS（不用 Live2D）**——当前 `Live2DCanvas.tsx` 是占位、将来迁移；FSM/emotionDriver/behavior→参数映射等技术无关层沿用。已落地：① 形象设计三文档拷入 [`docs/specs/liri/`](specs/liri/)（设计规范/动画设计/制作规范，原在桌面）② **人格配比落进 system prompt**——`system.txt` 身份+`[Core Personality]` 块改成 Liri（温柔35/好奇20/聪慧20/安静15/调皮5/神秘5 + 狐狸观察者本性 + NOT 话痨/卖萌/依赖/永远积极）；`firstrun.rs::seed_persona` core 维度同步改 Liri（中文 key，confidence=确信度非权重%；**仅对新装生效**，当前库仍是旧种子）。Liri 5 条行为原则大多已被现有 system.txt 规则覆盖（②不假记忆=rule8「严禁编造」）。验证：`cargo test --lib` **216 passed** ✅。**待 rebuild 进 dev/release 生效**（system.txt 是 `include_str!` 编译进二进制）。**待办**：① 当前用户库 core persona_traits 还是旧种子（gentle/patient/...），可选 reseed（`DELETE FROM persona_traits WHERE trait_type='core'` → 重启自动用 Liri 维度重种）；② 动画层 Spine 迁移是新方向（替换 Live2DCanvas，待 Liri.spine 资产）。详见 §最近一轮 (2026-08-03 续②) + 记忆 `liri-character-spine-direction`。
> **2026-08-03（续）更新**：完成 **B3 Sleeping 配套收尾**（纯前端，2 文件 ~10 行，原则 #1/#5/#6/#10/#11）。① **睡着抑制 nudge**——`App.tsx` DeepNight/LateNight nudge effect 加 `fsmRef.state===Sleeping` 守卫，睡着不再冒「早点睡」梦话（fsmRef 稳定无 stale-closure）② **接 sleep 音效**——`soundManager.ts` 加 `"sleep"` AssetKey + ASSET_PATH（`/audio/voice/sleep.mp3`，素材早已在）+ `sleep()` 方法（**mirroring `greet()`**：一次性状态进入 cue，**非**加权随机交互；mute 经 `ensureCtx` 尊重 #6），`App.tsx` 入睡 `forceState(Sleeping)` 后调 `sound.sleep()`（`fsm.state!==Sleeping` guard 保证每次入睡只响一次）③ **LateNight 不入睡只 yawn**——**已满足，零改动**：auto-sleep 本就 DeepNight-only（`App.tsx:239`），LateNight 经 sleepiness 权重调制多 yawn（Tier3 #7 早已做）。**验证**：`tsc --noEmit` ✅ / `npm run build` ✅（482 modules，2.16s）。**待实跑（已免改系统时间）**：为验收 A4/A5/B3 等需 DeepNight 的项，新增 **dev-only `window.__pet` 验收钩子**（`App.tsx`，`import.meta.env.DEV` 守卫，prod build grep `__pet`/`setHour` 0 命中✓）—— webview 内重写 `Date.prototype.getHours` 模拟时段（不改真实时钟、无 UAC）+ `forceIdle` 倒拨交互时间绕开 10min 入睡等待 + `sleep/wake/probeNudge` 直接触发。**操作清单见 [`docs/verify-checklist.md`](verify-checklist.md)**：`npm run tauri dev` → F12 → Console 用 `__pet.setHour(3)` 等。**当前无进行中任务**，下一会话按 **B4（P16 Debug Panel 补全）** → B5（Golden 评估框架）→ B8 推进；B1b（Grounding B 档运行时阻断）条件触发——实跑若仍偶发主动开口幻觉再升级。
> **2026-08-03 更新**：从 `D:\桌宠`（opencode 在本仓库副本上的工作）**合并** **B1 Consolidation 反向更新 Facts** + **B2 完整物理（自由落体/任务栏弹跳/1/3 飘落悬停）** + A4/A5 实跑方法论成果（CDP 自动化 + `Date.prototype.getHours` 重写模拟时段）。详见 §最近一轮 (2026-08-03)。两副本 base 完全一致（同 HEAD `50c45d2`，C/D 工作树在 grounding/reflection/Sleeping 等文件**逐字节相同**），故合并 = 纯增量复制 5 改文件 + 2 新文件（`gravity.ts`/`consolidation_harness.rs`），**零冲突**。验证：`cargo check --tests` ✅ / `cargo test --lib` **216 passed**（C 原 208 + B1 新增 8）/ `tsc --noEmit` ✅。清理了 harness 一处死变量（`ep_before`）。**当前无进行中任务**，下一会话按 B3（Sleeping 配套）→ B4（Debug Panel）→ B8 推进。
> **2026-07-31 18:01 更新**：三闭环 + 生命感主轴完成。**① 待验收代码层已全部闭环**——`cargo test --lib` 207 passed / `cargo check --tests` ✅ / `tsc` ✅ / `build` ✅，已 rebuild 进 18:01 release exe（含 A1/A2/A4 工作树 Rust 改动）。A1-A6 代码层 ✅、A7 勘误降级（未实现，单气泡覆盖）。**余下仅 GUI 运行时实跑**（A4/A5/A6 可立即验证；A1/A2/A3 需攒状态）——见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选) ①。**当前无进行中任务**，下一会话按 B1→B8 推进或先实跑 A4-A6。**主动开口幻觉已 A 档修复（19:10 rebuild，详见 §最近一轮）；残余：prompt 软约束无运行时阻断，B 档待命。**
**气泡 release rebuild 闭环（实跑确认 ✅ 2026-07-31）+ consolidation max_tokens 修复 + Reflection 触发器 Tier2 #5 + Sleeping 入睡机制（build 过 / 待实跑）。** 气泡：release exe 落后 dev 2 天，rebuild 后用户实跑确认居中。consolidation：生成任务 max_tokens 2048→4096（踩坑#3 复发）+ 空 content 防御。Tier2 #5：Reflection 事件驱动触发器（TurnThreshold 30 条对话记忆 / MajorEvent importance>0.85，1h 冷却，Daily→MajorEvent→TurnThreshold）。Sleeping：DeepNight(2-6) 无交互≥10min 自动入睡（forceState），交互（戳/摸/拖/对话/双击）markInteraction 唤醒 + 刷新 lastInteraction（天然 10min 清醒冷却）。后端 `cargo test --lib` 207 passed / 全 harness 编译 ✅；前端 `tsc`+`build` ✅。**下一步**：实跑 #4 converse thought / circadian 深夜 / 实跑 Sleeping（改系统时间 2-6 点+等 10min）/ 多气泡堆叠 / Tier2 #6。注：consolidation(≥100 episodes)/Reflection 触发器日常不易快速触发；Sleeping 需改系统时间到 DeepNight 验证。**全部已 rebuild 进 release exe（07-31 13:03），桌面快捷方式已含**；气泡已实跑确认，其余待择机实跑。

## §审计 (2026-08-03 续③)：深度审计 + 代码级核验

**任务**：用户要求审计计划（`implementation-plan.md` P0-P17/A1-A2）未完成部分、列优先级、按优先级继续开发。方法：**不轻信 HANDOFF 旧记录**，对照 codegraph + 源码逐项核验"声称未完成"是否属实、是否有遗漏。

**核验方法**：`codegraph_status`(103 文件/1442 节点) + `codegraph_explore` 看关键符号源码 + `codegraph_callers` 验调用方 + `Grep` 验生产路径 + Read plan P16/P17/A1-A2 验收标准。

**核验结论表**：

| 项 | HANDOFF 旧记 | 代码核验结果 | 证据 |
|---|---|---|---|
| **B4b conversations 死表** | backlog 普通项 | ❌ **确认真 Bug（#11 可追溯受损）** | `Grep conversations::(insert\|get_recent\|get_max_turn)` 于 `src-tauri/src` = **0 命中**；`codegraph_callers(insert)` 显示 `conversations::insert` 仅被测试 `test_insert_and_get_recent` 调用。plan P5.3 步骤 5 明确要求"原始对话日志写 conversations 表"。影响：无法回溯她原话（07-31 幻觉即因此无法定位）。 |
| **B4 Debug Panel** | "缺 5 分区" | ⚠️ **确认 6/9 分区** | `DebugPanel.tsx` = Brain/Counts/Facts/Episodes/Pending/Timeline。plan P16 还要 Prompt token / Retrieved score / Reflect / AnimFSM / Cost。后端 `DebugSnapshot`(commands.rs:689) 无对应字段。 |
| **B5 Golden 评估** | "框架不完整" | ❌ **确认无框架** | `tests/` 有 `golden_conversations.rs`（数据，42 符号）但**无 `evaluation.rs`**（plan P17 点名）。无 `personality_drift_score`、无 CI。Liri 人格刚落 system.txt → 缺回归网。**→ 已修复（2026-08-08 续）**：三层评估[规则/cosine/LLM-judge] + 30 golden 集全落地，见 §最近一轮 (2026-08-08 续)。 |
| **B6 A1 BrainState** | "架构债" | ⚠️ **确认债** | `converse()` = 10 参数（plan A1 要 `fn(brain:&BrainState)`），违反原则 #2 信号"参数>3"。在跑、重构触踩坑#4。 |
| **B7 A2 Scheduler** | "架构债" | ⚠️ **确认债** | `loop_runner.rs` = `std::thread::spawn`+`sleep`（medium 30s / slow 1h），非 plan A2 的 Scheduler trait。在跑。 |
| **B1b Grounding 阻断** | "条件触发" | ⏳ **确认条件成立、未触发** | `check_groundedness`(grounding.rs:235) 仅挂 converse、只 warn、`claim_patterns`(:256) 全英文（中文漏检）、未挂 proactive/welcome_back 输出端。07-31 A 档 prompt 收紧后**无复发报告** → 维持观察，不升级。 |
| **Liri/Spine 迁移** | 续¹³ | ✅ **里程碑1+全身修复 + driver phase1（串行+呼吸对齐）** | `SpineCanvas.tsx` 接 Spine3.8+PixiJS（runtime-3.8/loader-uni），`App.tsx` spineFailed→Live2D 回退。release 两 bug 已修（续¹²），全身已验。**driver phase1 已落地**：`spineIntent.ts` 翻译层 + 单一串行动作通道 + 呼吸节拍对齐治跳变（见续¹³，⏳ 待确认体感）。下一步：Phase 3 emotion→表情 slot 持续映射 + Phase 4 凝视追指针 + Phase 5 测试面板 + FSM-behavior→动画映射补全，`Live2DCanvas` 占位待删。 |

**遗漏排查（HANDOFF 未单列但核验发现）**：
- **A7 多气泡堆叠**：旧 backlog 已正确降级（App.tsx 单气泡覆盖语义，非堆叠）✅。
- **③散落 follow-up**（Alt+Space 全局键 / ~~走路脚步声 loop~~（2026-08-08 随走路计划砍除）/ 害羞慢现 / rest_need 后端暴露 / speedModifier 接动画 / idle_weights JSON 化 / 选择性遗忘）均为小项，核验仍属未做，不升优先级。

**重排优先级（驱动：北极星 #10 + 阶梯 活着→记住→懂你→工具砍 + #8 成本 + #11 可观测 + "是否受阻"）**：

三闭环全完成 → 生命感主轴在维护态。真正的 #10 下一步（Liri/Spine 视觉角色）**受阻于资产**。故当前**未受阻的最高 ROI = #11 Explainability 簇**（B4b 死表 + B4 决策链分区）——它直接服务"她为什么这么说"的诊断，07-31 幻觉这类问题有它早定位了；且 B4b 是真 Bug。

| 优先级 | 项 | 理由 | 本轮 |
|---|---|---|---|
| **P1** | **B4b conversations 死表** | 真 Bug、小、外科手术式、解锁 #11 可追溯 | ✅ 本轮 |
| **P1** | **B4-MVP 决策链分区（Retrieved+Intent+Reflect）** | #11 核心、诊断幻觉/漂移、中等工作量、未受阻 | ✅ 本轮 |
| P2 | B4 余项（AnimFSM 前端 / Cost LLM 计数 / Prompt 动态 token） | #11 补全，但需前端 plumbing 或 LlmClient 插桩 | ⏳ follow-up |
| P2 | B5 Golden 评估框架 | 锁 Liri 人格防漂移；重（需真 LLM、≥30 对话、CI） | ✅ **完成（2026-08-08 续）** 三层[规则/cosine/judge] + 30 golden 集 |
| P3 | B1b Grounding B 档 | 条件触发（A 档后无复发） | ⏳ 观察 |
| P4 | B6 A1 BrainState / B7 A2 Scheduler | 在跑的架构债、重构风险高 | ⏳ 顺带改 |
| P3 | Liri/Spine 迁移 driver 层 | phase1✅（串行 idle + 呼吸对齐治跳变，⏳ 待确认体感），下一步 emotion→slot/凝视/测试面板/FSM 映射补全 | 🟡 phase2-5 待做 |
| P5 | B8 二期 Shared World 等 | 二期愿景 | ⏳ 未来 |

**Scope 边界**：本轮只做 B4b + B4-MVP（三分区）。B4 余三项各有独立 plumbing 成本（AnimFSM 需前端 fsm 状态上抛、Cost 需 LlmClient 插桩、Prompt 动态 token 需记 last usage），单独立 follow-up 避免 scope 膨胀（原则 #9 刚够用）。

---

## §最近一轮 (2026-08-13 续²³)：AIRI 风格视线驱动 —— 头绕鼠标 + 身体微侧

**任务**：用户"头部绕鼠标转动，只在一定范围内生效且必须是头部转动加身体微侧，鼠标的围绕中心也是头部，幅度都不用太大。可以参考 AIRI"。纯代码可做（骨骼旋转运行时数据，零新素材）。

### 设计（最终形态）

- `SpineCanvas.tsx` 加 `pointerRef` prop（App 的全局光标轮询/click-through listener 已有，client 坐标，60Hz 后端采样）。
- 每帧：head 骨世界坐标 → 画布坐标 = 围绕中心；光标距头顶 `GAZE_RANGE=320px` 内生效，径向衰减 `f=1-dist/RANGE`，范围外平滑回正（AIRI ignored-return）。
- **只保留水平旋转通道**：头 ±`GAZE_HEAD_H=10°` + 身体(spine)±`GAZE_BODY=3°` 微侧；`GAZE_H_SIGN=-1`（用户报方向反后翻）。指数平滑 τ=`GAZE_TAU=0.12s`（挂钟 elapsedMS，昼夜变速不影响响应）；`BehaviorState.Sleeping` 时不跟随。
- **上下通道已移除**（用户：头飞起来了，下巴必须固定不能动）：2D 平面骨骼旋转只能表达左右，上下俯仰=位移=下巴脱离脖子。留给美术 `look_up/look_down` 动画，按"状态→动画叠加轨道"接（续¹⁹ 架构）。

### 五个坑（全部 CDP 实机坐实，踩坑级）

1. **`app.ticker.elapsedMS` 是每帧增量**（≈16.6ms 常数）——拿相邻帧相减≈0，平滑系数 k 冻结 → 视线纹丝不动（"没效果"第一报）。直接用 `elapsedMS/1000` 作帧时长。
2. **pixi-spine 烘焙在 `update()` 内部**：`apply(动画) → skeleton.updateWorldTransform() → 立即烘焙每个 slot 的 sprite transform/mesh 顶点`。在 update() **之后**改 bone.rotation 永远进不了渲染（顶点已旧）——数值对但视觉为零（"没效果"第二报）。修法：**包装 `skeleton.updateWorldTransform`**——动画写 locals 后、烘焙前把凝视角 ADDITIVE 加进 head/spine 的 rotation，再重算一次世界矩阵；apply() 每帧重置 locals → 无累积。
3. **head 骨局部 y 轴在世界空间里横指**（骨骼链带 ~92° 旋转）——沿局部轴位移 96% 横向走、竖直只剩 4%（CDP faceY 实测）。曾用 2×2 矩阵求逆把 canvas 空间 (0,dy) 解回局部轴——可行，但随后因坑 5 整通道废弃。
4. **头部位移无动画 key**：idle 动画只 key 头部旋转不 key 位移 → `apply()` 每帧不重置 → `+=` 每帧累加，头漂移 -1200 局部单位（hx 184→-164 实测）。位移必须绝对写入 `data.x/data.y` 基线（旋转则相反：必须 ADDITIVE，呼吸动画每帧 key rotation 天然重置）。
5. **平面旋转只能表达左右**：上下俯仰在 2D 里唯一手段是位移，位移必然让头部图层脱离脖子（"头飞起来了"）。用户钦定下巴固定 → 删点头通道。

### 验证（CDP 铁证）

- `headRot = 9.45°` = 呼吸 7.27° + 视线 2.10°，分毫不差 → 烘焙管线吃进凝视。
- 方向验证：光标在下 → faceY 屏幕下移 ✓（旧点头通道方向曾反，已翻）。
- 像素差分：视线激活 vs 回正，差异 bbox 恰为头部+上身（不含腿脚）。
- **CDP 诊断句柄已入代码**：`window.__gazeDiag`（凝视数值逐帧）/`window.__spine`（spine 实例，读骨/slot）/`window.__ctDiag`（origin+scale）。**GDI 截屏拍不到 WebGL 内容，验证前端渲染一律 CDP `Page.captureScreenshot`/`Runtime.evaluate`**（temp 脚本 `%TEMP%\opencode\eval_cdp.mjs`）。
- **多实例教训**：CDP 9222 端口可能连着残留实例（读数 -124 之谜），排查前先 `Get-Process desktop-pet` 确认单实例 + 各 PowerShell 会话 DPI 感知不一致致 GDI/物理坐标漂移，探针前用 `__ctDiag` 校准。

### 状态

`tsc/vitest 34 绿 / release rebuild / 用户目视确认没问题`。commit `beecbc0`(初版) → `8d273a1`(k 冻结) → `16797e0`(烘焙注入) → `786643e`(上下通道+方向) → `533b5ec`(下巴固定，最终)。调参：`SpineCanvas.tsx` 顶部 GAZE_* 常量。⚠️ 对方会话并行提交仍在进行（64d4e44 感知提示、4efbd2f 抽取器文风——与凝视零重叠，未干预）。

---

## §最近一轮 (2026-08-13 续²²b)：音效治理 —— 全局单音互斥 + 800ms 间隔 + 静默优先

**任务**（用户三轮反馈收敛）：①"每次点击都有音效，混乱，有时同时两个音效"→ ②"连着出声，先笑后ha"→ ③ 修完仍不符"我们制定的规范"（soundManager #10 宁少勿突兀 + 设计 6.5 行为-音效映射）。

### 三处改动（commit `c5c3a6b` + `fa2954b`）

1. **全局单音互斥**（`c5c3a6b`）：`playSample` 加 `currentSource`——新音效先 `stop()` 上一个，跨触发器永不重叠（右键菜单 send + 摸头/戳、拖拽 + 落地）。
2. **静默优先**（`c5c3a6b`）：menu 100% 必响 → 60 静默/40 出声；poke1 75% → 50 静默/30/20。
3. **全局最小间隔 10s**（`fa2954b` 800ms → `a279c05` 10000ms）：`play()` 加 `GLOBAL_MIN_GAP_MS`——任何**可听**音效后 10s 内的新请求直接拒绝（静默结果不占闸）。**用户规范最终版：播放 1 次，下次播放必须隔 10 秒**。治"连着出声"根因：**点身体无时间闸**（摸头有 3s 闸）+ **poke1/2/3 是三个独立触发器、冷却互不共享**（快速连点 3 下 = 3 音效 ~600ms 连响）+ **跨触发器零间隔**（摸头 laugh + 随即戳身体 surprise 相接）；上一轮互斥只是"截断+下一个立刻起"= 正是"先笑后ha"听感。

### 验证与合并

- soundManager 独立 tsc 验证（无 import 可单文件编译）；全量 tsc/vitest 34/cargo lib 301/check --tests 绿。
- ⚠️ 两次误提交教训：① 对方会话已 stage 的 haru 删除混入我的 commit（`git reset --soft` + `restore --staged` 撤销重来）；② 构建前必须确认无运行中实例（对方会话又启动了一个，踩坑#6 os error 5）——**提交前先 `git status` 查对方 stage，构建前先 taskkill**。
- 对方 Live2D 移除 `1e3cb0f` + 抽取器中文化 `f6c9c0a` 已入库 push；**release rebuild + 干净重启完成（14:1x）**，当前唯一实例。

---

## §最近一轮 (2026-08-13 续²¹)：记忆浮现多样性 —— novelty + 加权抽样 + 冷却

**任务**：用户"记忆浮现是根据置信度来排序的，导致每次浮现出来的都是星际穿越相关、宠物糯米相关的。太死板。出个更好的解决方案"。先 codegraph 全链路梳理 + 调研 xinchao-nian（借鉴其"驱力偏置召回/念头池/不自噬"思想，不搬平台层），定位三个根因后实施 Phase 1（用户钦定范围）。

### 三根因

1. **强化死循环**：`reinforce` 每次真实回忆 `strength += 0.03`（MIN 封顶 1.0），日衰减 `×0.998` 约等于无 → 主导记忆钉死 1.0，占评分 30% 权重永远赢 → 再被回忆 → 再强化。
2. **置信度 argmax 锚点**：`get_active_facts ORDER BY confidence DESC` → 三个浮现路径都用 `facts.iter().find(is_anchorable_fact)` 取**最高置信度第一个** → 星际穿越/糯米 facts 永远被选。
3. **零多样性机制**：无"最近浮现→冷却"、无"从未想起→探索加分"；MEMORY_QUERIES 轮换池 5 条语义同质，检索回来仍是同一批强记忆。

### 改动（8 文件，commit `ba87632`）

- `db/episodes.rs`：`reinforce` 改边际递减 `memory_strength += RECALL_BOOST*(1-strength)`（1.0 时增益 0，永不超过 1.0）；`test_decay_and_reinforce` 断言同步改。
- `mind/retrieval.rs`：权重 0.4语义/0.2strength/**0.15novelty**/0.15recency/0.1情绪；`compute_novelty = exp(-recall_count/5)`；`ScoreBreakdown` 加 `novelty` 字段（6 处构造点同步补，踩坑#4）；新增 `sample_surface_anchor`（12h 冷却过滤 `last_recalled_at` + softmax(score/0.6) 加权抽样，全冷却则放宽；空池 None）+ `SURFACE_COOLDOWN_HOURS`/`SURFACE_TEMPERATURE`/`NOVELTY_TAU` 常量（调参入口）。
- `pending/proactive.rs`：三处浮现路径（generate 记忆分支 / welcome_back / lonely_nudge）锚点改抽样——fact 按 `1/(1+mention_count)` 加权（新 `sample_anchorable_fact`）、episode 走 `sample_surface_anchor`；retrieve top_k 3→8（更大抽样池）；到期提醒仍绝对优先；ThreadRng 收敛内层块（Send 踩坑）。
- 对话路径不变：planner 仍取 top-1（相关性优先），仅浮现路径抽样——直接提问"最近忙啥"仍精确召回。

### 验证

`cargo test --lib` **301 绿**（+8 新测：novelty 单调/排名、冷却排除 20 seeds、全冷却放宽、fact 抽样 100 seeds 95+% 选中未提及者、强化递减）/ `golden_conversations` **29 绿** / `check --tests` 绿。**零新增 LLM/embedding 调用**（#8）。

### ⚠️ 并行会话冲突（重要）

实施中发现**另一会话正在并行改仓库**（记忆导出重构：`mind/export.rs` 新文件 + vectors.rs `get_all` + commands/facts/pending/lib.rs 等 9+ 文件未提交）。其 `vectors.rs` 在 13:18:59 被改成**半写状态**（`count()` 签名行被删、函数体残留）→ 语法错误 → **release 构建被阻塞**。处理：本会话只 `git add` 自己的 8 个文件单独提交（`ba87632`，已 push），**未碰**对方文件、未 rebuild、未重启桌宠。

### ⏳ 待办（下一会话）

1. 等另一会话的导出重构完成（其文件入库、语法恢复）。
2. `npx tauri build --no-bundle`（含本轮后端改动）+ 重启桌宠。
3. 实跑观察浮现多样性：Debug Panel 看 proactive/welcome/lonely 的 anchor 是否不再单一；若仍偏死板调 `SURFACE_TEMPERATURE`（大→更随机）或 `SURFACE_COOLDOWN_HOURS`（小→更活）。
4. 可选 Phase 2（用户未选）：驱力→query 映射（孤独→关系记忆、疲惫→轻松回忆），把 MEMORY_QUERIES 轮换池改成情绪驱动。

---

## §最近一轮 (2026-08-13 续²⁰)：气泡尾巴锚点 —— 固定璃头顶右侧，任何情况不再漂移

**任务**：用户"希望以底部的尾巴为锚点将气泡固定在头顶右侧位置，且之后任何情况都不会发生改变。当前气泡位置在左侧"。

### 根因（两个位置漂移源 + 一个错误锚点值）

1. `PetBubble.tsx` 内联样式硬编码 `left:150px / bottom:530px`（左侧），且 CSS `.pet-bubble-anchor` 还带着 `translate:-50% 0`（半宽位移，随文字宽度变化——长文气泡会进一步左移）。
2. `.pet-bubble-anchor.bubble-pet` 覆盖规则把摸头气泡挪到 `left:40%`（违反"任何情况不变"）。
3. 定位常量没有依据真实模型几何。

### 几何推导（与实机一致）

- 窗口 400×760；canvas 400×600，顶在窗口 y=150（`.pet-container` flex-end + padding-bottom 10px）。
- 璃模型运行时实测（CDP 读 `.bounds-overlay` 命令式定位）：**画布 x[124,276] y[90,510]**（模型永远居中于画布中心 (200,300)，fit=min(400/w,600/h)×0.7）。
- 头部（后发团+双耳）占模型顶部：头顶=窗口 y240；右耳窗口 x[219,280] y[275,324]（node 解析 liri.json 各 slot attachment 世界包围盒 + 比例映射）。**头顶右侧锚点 = 窗口 (210,255)**。

### 改动

- `PetBubble.tsx`：锚点盒 `left 150→188px / bottom 530→512px`（尾巴尖在盒左下角 22px、盒底下方 7px 处 → 尖端落 (210,255)）；注释写清换算公式，未来改锚点只动这两个值。
- `styles.css`：删 `.pet-bubble-anchor` 的 `translate:-50%` + TEMP DIAG `left:320px`；**删 `.bubble-pet` 位置覆盖规则**（保留 App 端 plumbing 但已失效=位置恒定）。
- 长文向上/右生长，尾巴尖不动（"尾巴即锚点"契约，PetBubble 原有设计）。

### 验证（CDP 实机，非仅静态）

- 重启 pet 带 `--remote-debugging-port=9222`（踩坑续：GDI 截屏拍不到 WebView2 GPU 合成内容——角色和气泡 DOM 都不可见，只能 CDP）。
- CDP 实测：canvas (0,150,400,600) ✓；模型 bounds 与推导一致 ✓；点击璃头触发摸头气泡"抹抹~"，实测尾巴尖渲染于**窗口 (211,256)**，与设计值 (210,255) 差 1px ✓。
- `tsc exit0 / vitest 34 绿 / release rebuild exit0` + 桌面快捷方式重启（干净实例，无调试端口）。
- 顺带收尾：上轮未提交的 PetBubble 表面 + liriAssetPatch + spineIntent 清理一并入库（commit `4687e3a`，11 files）。

### 踩坑新增（写入避免重复踩）

- **GDI CopyFromScreen/PrintWindow 拍不到 WebView2 的 GPU 合成内容**（WebGL canvas 和 DOM 都不可见，只能看到穿透窗口的桌面背景）——验证前端渲染一律走 CDP（`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` + `_cdp_run.mjs` eval）。视觉模型在这种截图上会幻觉（把编辑器 UI 当角色）。
- **release 构建可覆盖运行中的 exe**（本轮实测成功，未触发踩坑#6 的 os error 5——但别依赖，仍先 taskkill 稳妥）。
- `taskkill //IM` 在 PowerShell 里参数无效，用 `Stop-Process -Name desktop-pet -Force`。
- `.bounds-overlay` 命令式定位是读模型 bounds 的现成后门：光标靠近模型外框±12px 触发 `.bounds-visible`，CDP 读 `getBoundingClientRect()` 即得实机几何（免改代码）。

---

## §最近一轮 (2026-08-12 续¹⁹)：Spine 表情架构转向 —— 状态→调动画，代码不碰 attachment

**任务**：续¹⁸ 留的"重新实现表情映射（互斥隐藏默认眼 + 阈值 0.65 + forceSyncSlot name key）"待办。我按计划做完 + dev 三态截图验证，**用户实跑反馈推翻前提**，最终架构转向。

### 第一阶段：按续¹⁸ 待办实现（后废弃）

按 3 修复点做：`spineIntent.ts` 加 `eyeLSlot/RSlot` + 默认眼 attachment（互斥用）+ 阈值 0.5→0.65；`applyEmotionFace` 改返回 `changedSlots`；`SpineCanvas.tsx` 在 update 后 forceSyncSlot 循环（复制 pixi-spine update() 的 region 分支：sprite 缓存 key=attachment.name、createSprite/addChild/setSpriteRegion）。踩坑 `setSpriteRegion` 是 TS private，用 `(spine as any)` 绕过。`tsc/vitest 24/build` 全绿。

### 第二阶段：dev CDP 三态验证（推翻前提）

通过 `__TAURI_INTERNALS__.invoke('set_emotion', {edit:{...}})` 注入情绪（Tauri v2 dev 无 `__TAURI__` 全局，但 `__TAURI_INTERNALS__.invoke(cmd, payload)` 可用，9ms 返回；signature 是 `invoke(cmd, payload={})` 非 args）。三态截图（normal energy0.6 / tired energy0.1+rest_need0.8 fatigue1.02 / restored），用户目视 + GLM 视觉模型确认 normal 完全睁眼 ✓、tired 半闭眯眼 ✓。

**但用户反馈两个新问题，推翻 phase3 整套前提**：
1. **"现在的嘴绝对不是呼吸状态的嘴，一直张开"** —— 呼吸态嘴应是闭合线。
2. **"smile 动画张大嘴笑幅度比现在大2倍，通过 mesh 实现的"** —— 当前幅度不够。

### 第三阶段：CDP 深查根因（纯美术资产问题）

加临时 dev-only `window.__spineDiag = spine`（验证完删）暴露 spine 实例，CDP 读 slot attachment：
- **idle 状态**：`嘴`→`嘴`(显示)、`小笑嘴`→`小笑嘴`(显示)、`张大笑嘴`→`NULL`(body_breath 救了)、`左眼/右眼`→正常。
- **force smile**（`state.setAnimation(5,'smile')`）：`张大笑嘴`→`NULL`、`嘴`→`嘴`、`小笑嘴`→`小笑嘴`。

**结合 node 解析 liri.json 坐实**：
- **setup pose**：`嘴`/`张大笑嘴`/`小笑嘴` 三个 slot 默认 attachment 都设成显示（应 null）。`张大笑嘴` 被 body_breath t=0 null 救回，但 `嘴` 和 `小笑嘴` 没有任何动画碰 → 永远显示 → 叠在 `脸`（含闭合嘴）之上 = 看着张开。
- **smile 动画结构**：`slots` 部分**只切眼睛**（左笑眯眼/右笑眯眼/左半笑眼/右半笑眼/左眼/右眼），**完全不碰 `嘴`/`张大笑嘴`/`小笑嘴` 的 attachment**；deform 部分改 `嘴`+`张大笑嘴` mesh 顶点（42 顶点大幅形变 0-0.4s + 半张保持 + 3.33-3.93s 恢复）。但 deform 只在 attachment shown 时可见 → body_breath 钉死 `张大笑嘴` null → smile 期间张大嘴 deform 不可见 → 视觉只看到眼睛笑眯眼。
- 用户美术意图（实跑确认）：idle 只有 `脸`（含闭合嘴）显示；smile 才有嘴部变化；`小笑嘴` 暂不涉及。

### 第四阶段：架构原则转向（用户钦定）+ 代码清理

用户："我的动画里面做了相关的内容，不需要再去从骨的状态拆解" + "后续会补充更多动画，现在只是对已经做好的基础部分进行测试。后续只需要相应状态可叠加的调用相应的动画即可。现在的任务是特定动作或特定设定能精准调动合适的动画。还没做动画的部分一律先不用管"。

**新原则**：**状态/情绪 → 播放对应动画（叠加 track）；动画 timeline 自己管 slot attachment；代码绝不 setAttachment 改 slot**。

**代码清理（删掉所有错误前提产物）**：
- `spineIntent.ts`：删 `applyEmotionFace`/`fatigueLevel`/FaceState 的 eye/mouth 字段/阈值常量/EmotionVector import；`triggerSmile` 删手动 `嘴/张大笑嘴.setAttachment`；`endSmileMouth` 删；`endAction` 改空 hook；FaceState 只剩 `smileDuration`（计时用）；initFace 只 findAnimation 拿 duration。
- `SpineCanvas.tsx`：删 `applyEmotionFace` 调用 + forceSyncSlot 循环 + `emotionVector` prop + `emoRef` + 临时 `__spineDiag`；updateFn 回到 phase1 纯调动画形态。
- `App.tsx`：删给 `SpineCanvas` 传的 `emotionVector` prop（Live2DCanvas 的保留）。
- **保留**：串行动作通道 + 呼吸对齐 + 4 个 action 全部纯调动画（playAction 只 setAnimation 不碰 attachment）+ setupMix/setupIdleTracks/triggerBehavior(Embarrassed→wink)。
- `tsc exit0 / vitest 24 ✅ / build 3.28s`。

### 验证

- 静态全绿。
- **运行时验证（待美术补资产）**：dev 看到 idle 嘴闭合、smile 张大嘴 + 笑眯眼 —— 都依赖美术在 Spine 里改 setup pose + smile slot 关键帧。代码侧已干净，等资产到位。

### 待美术资产补（下一会话起点见 §当前任务）

1. **setup pose**：`嘴`、`张大笑嘴`、`小笑嘴` slot 默认 attachment 设 null。
2. **smile 动画 slot 关键帧**：`嘴`/`张大笑嘴` t=0 show → t=3.93 null。
3. 拷贝更新后的 `liri.json`/`liri.atlas`/`skeleton.png` 到 `public/spine/liri/` → dev 验证 → release rebuild。

### 架构契合

#1（表情映射纯前端规则，但前提错了——改为纯美术 timeline）/ #5（mind-body 解耦：intent→anim，不直接操控 slot）/ #6（graceful degrade：FaceState 缺 slot 只降级 smileDuration 计时）/ #10（情绪→表情外显：通过播放对应动画实现，未来补动画即可接）/ 踩坑新增（写入避免重复踩）：**setup pose 默认显示的 slot 会永显，必须有动画 null 它才会隐藏；deform 只在 attachment shown 时可见；Tauri v2 dev 无 `__TAURI__` 全局，用 `__TAURI_INTERNALS__.invoke(cmd, payload)`；pixi-spine setSpriteRegion 是 TS private，运行时可用 `(spine as any)`；改渲染层任何代码前先确认前提（运行时覆盖 slot vs 调动画）——phase3 整套建立在"美术没做 timeline 才需要运行时覆盖"的假设上，但实际美术做了 deform 只是漏了 attachment show 关键帧，应美术补而非代码补**。

---

## §最近一轮 (2026-08-12 续¹⁸)：Debug 窗口死锁修复 + Emotion 编辑器动画测试链路

**任务**：① 用户"继续修 debug 白屏的问题"（续¹⁵ 留的 follow-up）；② 用户问"如何用面板测试动画"，实测"调整 Emotion 编辑器看不到任何效果变化"。

**① Debug 窗口死锁（真根因，已修✅）**：
- 续¹⁵ 后代码已是 label 判据 + 无 query（1542e84），但用户仍白屏 → 我实测（CDP 合成 F12）发现 **F12 后窗口根本不弹**（比白屏更糟）。
- 诊断链：CDP 附加主窗 → 合成 F12 → onKey 执行（preventDefault 证明）→ **invoke 全 pending**（get_debug_snapshot/set_emotion 都 pending）→ **Rust 日志停在 "[debug-window] creating new window" 无 build ok/failed** → 定位 **sync command 在主线程执行 `WebviewWindowBuilder::build()`，build() 等 WebView2 回调但主线程消息循环被同步命令阻塞 → 死锁**（主线程卡死解释所有 invoke pending）。
- **修复**：改 `pub async fn open_debug_window`（async command 在 tokio 线程执行 build，主线程消息循环继续转）→ 用户按 F12 两次均 build ok ✅ → **用户确认"没问题了"**。
- 诊断方法论教训：**CDP 附加状态会干扰 Tauri IPC**（evaluate 里 invoke 全 pending 是主线程死锁的结果而非 CDP 所致；合成 F12 事件走页面 handler 是可靠触发路径）；`/json` 列出的 page 无法用 title 区分主窗/debug 窗（document.title 相同），**用 `getCurrentWindow().label` 区分**（前几轮截图对比全截在 debug 窗上导致 diff=0 误判"没效果"！）。

**② Emotion 编辑器"看不到效果"（真根因：pixi-spine 渲染机制）**：
- 数值链路正常（set_emotion→DB→emotion-update→emotionVector→fatigueLevel 计算全通），**渲染不动**。
- **根因**：pixi-spine 渲染走**缓存显示对象** `slot.currentSprite`（Region）/`slot.currentMesh`（Mesh），**只在 Spine.update() 内按 `slot.getAttachment()` 同步**（SpineBase.js update() 的 slot 遍历）。`slot.setAttachment()` 只改数据、**不更新缓存 sprite** → 下一帧 update() 又按动画值同步 → 视觉永不变。**这就是为什么 face-diag 显示"设置成功"但画面 diff=0**。
- **修复**：新 `forceSyncSlot(spine, slot)` 复制 update() 的 region/mesh 分支（切换 currentSprite/currentMesh + visible），在 setAttachment 后调用。
- **踩坑①（key 用错）**：region 分支 sprite 缓存 `slot.sprites` 的 key 是 **`attachment.name`**（SpineBase update() region 分支用 `currentSpriteName !== attachment.name`），mesh 分支才用 `attachment.id`——我初版 region 用 id → `slot.sprites[id]` undefined → `currentSprite.renderable` 崩（try/catch 吞 → 表情不动 + 无感知）。修：region 用 name。
- **踩坑②**：region attachment 无 `computeWorldVerticesOld` 方法（报错但不影响显示）→ 删。
- **验证**：dev 截图三态 + GLM 视觉模型确认——**normal 完全睁开 / tired 半睁（位置无异常）/ happy 眯成弯月+微笑嘴** ✓。release 构建后**用户反馈"双层眯眼图层+启动即眯眼"**：
  - 双层图层 = 显示半睁眼时**未隐藏默认眼**（左眼/右眼 mesh 还在下面）→ 叠加；
  - 启动即眯眼 = 默认 fatigue 0.55 > 阈值 0.5（默认能量基线低）→ 半睁常驻。
- **回退（用户要求）**：`git checkout -- src/SpineCanvas.tsx src/animation/spineIntent.ts`（回到 366ffc8，眼睛正常）→ release 重建 → **用户确认正常**。

**③ DebugPanel 增强（保留，未回退）**：
- **Face State 分区**：后端 snapshot 本地计算 fatigue/halfOpen/smiling/阈值显示（**放弃跨窗口 emit 方案**——updateFn 里每帧 IPC emit 是"桌宠透明"事故的元凶嫌疑，回退时移除；面板本地计算零新增 IPC、零渲染层耦合）。
- **滑块拖动即时生效**：250ms 节流自动 invoke set_emotion（不用点 Apply——"只拖不点"是用户看不到效果的可能原因之一）。
- **后端 `EmotionEdit` 加 `rest_need` 字段**（update_fields 第 7 参接通）+ 面板滑块加 rest_need。

**透明事故复盘**：某次 release 桌宠完全不可见（透明）。回退后恢复正常。嫌疑：SpineCanvas updateFn 里新加的 `emit("face-state")`（每帧状态变化时跨窗口 IPC）——updateFn 抛错 → PIXI ticker 崩 → 画布空白 → 窗口透明。**结论：渲染热路径（每帧）绝不碰 IPC/emit；任何渲染层改动全 try/catch**。

**架构契合**：#1（表情映射纯前端规则）/ #6（try/catch 静默降级 + slot 缺失优雅退化 + 回退保底）/ #10（情绪→表情外显）/ #11（[debug-window] 日志 + face-diag 诊断 + console.warn 暴露 forceSyncSlot 错误）/ 踩坑新增（写入避免重复踩）：**pixi-spine setAttachment 不更新渲染、region 缓存 key=name、渲染热路径禁 IPC、CDP 区分窗口用 label、PowerShell 不写 UTF8 中文文件（Set-Content 编码损坏 HANDOFF，git checkout 恢复）**。

**验证**：tsc exit0 / vitest 24 / lib 293（未动 Rust 逻辑，rest_need 字段加后全绿）/ dev 截图三态验证 ✓ / release 重建（回退版）。

**当前无进行中任务**。下一会话起点：按 §当前任务 待办重新实现表情映射（互斥隐藏默认眼 + 阈值 0.65 + forceSyncSlot name key，全 try/catch）。

---

## §最近一轮 (2026-08-11 续¹⁵)：Debug Panel 独立 OS 窗口[实跑白屏·留 follow-up]

**起因**：续¹⁴·补 把 Debug Panel 改成内嵌可拖动浮窗(300px 右下、toolbar `startDrag`)，用户反馈"还是会挡住身体下半部分"。根因：主窗 400×760 透明，`position:fixed` 元素被窗口边界裁剪，拖到哪都和 Liri 重叠。用户要"全局可拖动"。

**诊断**：窗口内 `position:fixed` 无法逃出 400×760 边界 → 屏幕级拖动只有独立 OS 窗口(WebviewWindowBuilder 第二窗口)一条路。

**落地 8 处（commit 7f5e912）**：
- `src-tauri/src/commands.rs` +`open_debug_window(app_handle)`：`get_webview_window("debug")` 已存在则 show+set_focus 返回；否则 `WebviewWindowBuilder::new(&app,"debug",WebviewUrl::App("index.html?window=debug"))`.title("DesktopPet·Debug").inner_size(360,720).min_inner_size(300,400).resizable(true).build()。仿 `open_devtools`(commands.rs:1243)。
- `src-tauri/src/lib.rs` invoke_handler 注册（open_devtools 与 quit_app 间）。
- `src-tauri/capabilities/default.json` `"windows":["main"]`→`["main","debug"]`：label=debug 窗口能 invoke 所有已注册命令。
- `src/main.tsx`：`URLSearchParams(location.search).get("window")==="debug"` → 渲染 `DebugStandalone`，否则 `App`。
- `src/DebugStandalone.tsx`(新)：包装 DebugPanel，onClose=`getCurrentWindow().close()`、onQuit=`invoke("quit_app")`、`anim={state:"（主窗口独占）",history:[]}`。
- `src/App.tsx`：F12 handler 改 `invoke("open_debug_window")`(删 setShowDebug toggle)；删 `import{DebugPanel}`、`showDebug` state(L95)、forceCapture 两处 showDebug(L602+L631 deps)、内嵌 `{showDebug&&<DebugPanel/>}` 块。grep 确认 `fsmRef`(109-993 多处)/`handleQuit`(L1182 ContextMenu) 仍有引用(非孤儿)。
- `src/DebugPanel.tsx`：删自绘拖拽全套(`ReactMouseEvent` import/pos/drag state/startDrag 闭包/toolbar onMouseDown/panel style pos 注入)，hint 改"独立窗口·标题栏可拖到任意位置"。
- `src/styles.css`：`.debug-panel` 从 `fixed bottom:0 right:0 width:300px max-height:60vh border-radius box-shadow` 还原为 `fixed inset:0 box-sizing:border-box`(钉满独立窗视口，绕开 body margin)；`.debug-toolbar` 删 `cursor:move`(OS 标题栏拖动，toolbar 不再是拖把)。

**验证**：`cargo check` 34.44s ✅(WebviewWindowBuilder API 正确，PowerShell NativeCommandError 是 5.1 对 stderr 包装非真错) / `npx tauri build --no-bundle` exit0(tsc+vite 0 类型错，release 52.98s，desktop-pet.exe) / commit 7f5e912 + push(6dcbe90..7f5e912)。

**⚠️ 实跑白屏（用户"debug界面打开是白色，没有内容。不用修改"）**：F12 能弹独立窗口(标题 DesktopPet·Debug)，但内容区全白。
- **疑似根因①（最可能）**：`WebviewUrl::App("index.html?window=debug")` 的 query string 在 release custom-protocol(`tauri://`/`asset://`)下未被保留/被当 path 字符 → `main.tsx` 的 `URLSearchParams(search).get("window")` 取 null → 三目走 `<App/>`(主窗逻辑在 debug 窗里无 canvas 挂载/无 400×760 适配→白)。
- **疑似根因②**：分支命中渲染了 DebugStandalone，但 `DebugPanel` L143 `if(!snapshot)return null`——debug 窗口 invoke `get_debug_snapshot` 若失败(capability 对动态 WebviewWindowBuilder 窗口实际未覆盖？)→ snapshot 永空→return null→白。
- **修复方向(follow-up 未做)**：① 最佳——`main.tsx` 改判据 `getCurrentWindow().label==="debug"`(label 由 Tauri 注入，不依赖 URL)，`WebviewUrl::App("index.html")` 不带 query；② 或 hash `index.html#window=debug`(`location.hash` 解析，App 路径不读 hash 无副作用)；③ 实测 capability 对 WebviewWindowBuilder 动态窗是否真生效。
- **用户决策**：明示"不用修改"。当前 F12=弹白屏独立窗(不挡 Liri，主诉求"不挡桌宠"已达成，仅 panel 内容不可见)。

**附**：commit 7f5e912 的 `git add -A` 顺带带入之前 untracked 的 `docs/review/prompt-quality-report-2026-08-09.md`(文档，已告知用户)。

## §最近一轮 (2026-08-11 续¹⁴)：Spine driver phase3-A 情绪→半睁眼持续映射

**背景**：续¹³ driver phase1（串行通道+呼吸对齐）代码+release 完，⏳ 待用户确认体感。本轮接 Phase 3（HANDOFF 既定下一步：emotion→表情 slot）。北极星 #10「情绪连续外显」对 Live2D 早标 ✅（emotionDriver 全维度），但 **Spine 路径此前零接 emotion**——App 算好的 emotionVector 只传 Live2DCanvas，SpineCanvas props 没有该字段。这是 Spine 路径最大的功能缺口。

**Phase 3 不阻塞 phase1 验证**：phase1 治耳尾呼吸对齐（脊柱跳变），Phase 3 治表情 slot（眼/嘴），独立维度。半睁眼叠加在串行通道之上但不碰脊柱链，phase1 跳变问题（若有）不影响 Phase 3 验证。

**MVP 范围收敛（只做半睁眼一维）**：animation_spec 既定 emotion→slot 三条：① rest_need 高/energy 低 → 半睁眼左/右（疲惫）——**本轮做**；② mood 高(>0.55) → 笑眯眼+小笑嘴（常驻）——skip；③ stress 高/mood 低 → 眉毛下垂（需补动画）——skip。只做①的理由（Ponytail + #克制）：①唯一**无争议**（疲惫必显）、**无耦合**（不与 smile 嘴 override 交织）、**最显著**；②笑眯眼常驻与 Liri 安静 15% 人格有张力（spec >0.55 阈值对安静角色偏低），小笑嘴与 triggerSmile 嘴 override 耦合（smile 结束 endSmileMouth 还原 vs emotion 接管，边界易微闪）；③spec 自标"需补动画"。先验证①，②③阈值/耦合方案等用户看过①体感再定。

**关键约束（读 animation_spec.md:18 坐实）**：几乎所有 idle 都 key `半睁眼左/右`=隐藏，只有 blink/smile/yawn 让它显示。pixi-spine attachment timeline 每帧 update 覆盖手写 attachment → 持续 emotion 映射必须在 `spine.update()` **后** setAttachment 且每帧重设（每帧渲染前都是我写的值 → 稳定不闪）。blink/smile busy 时让位，ear/tail 不让位（只 key 脊柱不碰眼）。

**落地**：
- `src/animation/spineIntent.ts`：FaceState 加 `halfEyeL/RSlot`+`halfEyeL/RAtt`（nullable）；`initFace` 独立 try 捕获半睁眼（findSlot + getAttachmentByName 同名约定，验证自现有 小笑嘴 模式 + atlas Grep 确认 slot 存在）——缺失只降级 emotion 眼，smile 嘴不受影响（#6）；新 `fatigueLevel(e)` 镜像 emotionDriver 眼公式（`max(0,0.6-energy)*1.4 + rest_need*0.4`）；新 `applyEmotionFace(face,fatigue,suppressed)`——`!suppressed && fatigue>0.5` 显示半睁眼，否则隐藏。
- `src/SpineCanvas.tsx`：props 加 `emotionVector: EmotionVector`；`emoRef`（镜像 speedRef/behaviorRef 范式）；`updateFn` 里 `spine.update(dt)` 后算 `suppressed = busy && (blink|smile)` 调 `applyEmotionFace(face, fatigueLevel(emoRef.current), suppressed)`。
- `src/App.tsx`：SpineCanvas 加 `emotionVector={emotionVector}`（state 早存在，DEFAULT_EMOTION 初始 fatigue=0 → 默认睁眼）。

**验证**：`tsc --noEmit` exit0；`npm run build` exit0（2.84s）；`npx tauri build --no-bundle` exit0（1m01s，纯前端改动 Rust 缓存命中只重新嵌入 dist）。commit `366ffc8`。

**续¹⁴·补（Debug Panel 可拖动浮窗，commit 184d7e0）**：用户反馈 panel 全屏覆盖挡脸、无法验证表情。改 `.debug-panel` 全屏 absolute → fixed 右下 300px 浮窗（max-height 60vh、圆角阴影，让出左上脸区）；toolbar 加拖拽（mousedown→全局 mousemove/up，clamp 窗口内，点按钮不触发）+ cursor:move；Emotion 编辑器移到面板顶部（验证表情免滚动）。tsc/vite build 绿，release rebuild 1m35s。

**⏳ 待实跑**：桌面快捷方式 → F12 Debug Panel → Emotion 编辑器拉低 `physical_energy`（如 0.2）或拉高 `rest_need`（如 0.8）→ Apply → 肉眼确认璃**半眯眼**；等 blink/smile 触发瞬间半眯让位、过后恢复；拉回正常值半眯消失。若半眯不出现 = 半睁眼 slot 名不符（halfEye 字段 null）→ F12 console 诊断。

**已知局限 / 下一阶段**：仅 fatigue→半睁眼一维，笑眯眼+小笑嘴(mood)/眉毛(stress)待 follow-up；半睁眼阈值 0.5/增益照搬 emotionDriver，可能需实跑按 Liri 体感微调；phase1 体感（续¹³ 跳变根治）仍待用户确认（本轮不依赖不阻塞）；仍待 Phase 4 凝视追指针、Phase 5 测试面板、FSM-behavior→动画映射补全、`Live2DCanvas` 占位待删。

---

## §最近一轮 (2026-08-10 续¹³)：Liri Spine driver 层 phase1 —— 串行通道 + 呼吸节拍对齐治跳变

**背景**：续¹² 全身显示 OK 后接 driver 层。先做 idle 生命感（耳/尾/眨眼/笑间歇动），用户三轮迭代反馈暴露**身体跳变**，最终诊断+方案如下。

### 三轮反馈 → 根因收敛
1. 初版（4 idle 持续 loop）：用户"太频繁"。
2. 改间歇（4 选 1 共享池 + loop 2.5s）：用户"耳不动、尾间隔 1 分钟且连续动 2 次、偶发跳变（身体摆右→跳左重新开始）"。
3. **node 解析 liri.json 坐实根因**：
   - `ear_idle`/`tail_idle`/`arm_idle`/`hair_idle` **全部 key 整条 spine 链 + head**（不只命名部位）→ 任何 idle 触发都驱动身体。
   - "1 分钟间隔" = 4 选 1 池分摊（单部位 ~48s）+ 间隔用 circadian-scaled dt（深夜 ×2-2.5 放大）。
   - "连续动 2 次" = idle duration ~1s 但 loop 2.5s → 循环 2-3 次。
   - "跳变" = ① loop 接缝（末帧身体偏右→首帧左）② **更主要**：ear/tail 在 `body_breath` 呼吸中途插入，spine 从呼吸中间态跳到 idle 首帧。
   - ear/tail 幅度足够（ear_l2 ±17° 明显）——"耳不动"纯概率（4 选 1 没轮到）。

### 最终方案（用户点破"呼吸做完一轮恢复初始状态才允许动作加入"）
**① 单一串行动作通道**：`busy` 标志，blink/ear/tail/smile 一次一个，做完（含 fade）才下一个 → 绝不重叠。
**② 呼吸节拍对齐**：ear/tail（key spine）只在 `body_breath` 每轮 `complete` 事件触发——此刻身体回 setup，idle 首帧亦 setup，两者重合零跳变。`spine.state.addListener({complete})` 监听 track0；`spinePending` 在 spineT 到点时置位，等 complete 兑现。blink/smile 只 key 眼 slot 不碰脊柱→不跳，独立计时（眨眼~5s/笑~12-18s）但受 busy 互斥。
**③ idle `loop=false`**（一次性，消 loop 接缝跳）+ 播完 `setEmptyAnimation(track, 0.3)`（带 mix 回 setup，消 clearTrack 硬切跳）。
**④ 双时钟**：`dt=deltaMS/1000`(circadian 缩放) 喂 `spine.update`（动作播放随昼夜变速，#10）；`wall=elapsedMS/1000`(挂钟) 驱动 blink/smile/spine 间隔（"多久动一次"昼夜稳定）。
**⑤ 笑容嘴部覆盖**：smile 动画只 key 眼 slot（笑眯眼），嘴不变——手动 `嘴.setAttachment(null)` + `小笑嘴.setAttachment(附件)` 持续 smileDuration，结束还原（initFace 在 setup pose 捕获 slot ref）。

### 落地
- 新 `src/animation/spineIntent.ts`：翻译层。`TRACK{breath0/ear1/tail2/expr5}`；`playAction(kind)`/`actionDuration`(ear/tail=duration+fade)/`beginFadeOut`(ear/tail→setEmptyAnimation)/`endAction`(smile→关嘴)；`setupMix`(defaultMix 0.15)；`initFace`；间隔 `nextBlinkDelay(~5s)`/`nextSmileDelay(12-18s)`/`nextSpineDelay(5-8s→耳尾各~10-16s)`。内部封装 playEar/playTail/triggerBlink/triggerSmile/endSmileMouth（不再各自 export）。
- `src/SpineCanvas.tsx`：autoUpdate=false（续¹² 修，circadian 经 deltaMS 到达）；串行状态机（busy/busyRem/busyKind/faded）+ `onBreathComplete` listener + 双时钟；点击上下分屏 hit（待真 polygon）。
- `src/App.tsx`：SpineCanvas 分支传 `behavior` prop（FSM BehaviorState，目前仅 Embarrassed→单眼 wink）。

### 验证
- `tsc --noEmit` exit0；`npx tauri build --no-bundle` exit0（48.95s）。
- **⏳ 待用户最终确认体感**（跳变根治否/串行节奏自然否/频率 OK 否——用户"今天就到这"未给本轮反馈）。

### 已知局限 / 下一阶段
- ear/tail 播放期间 body_breath 被覆盖（都 key spine）→ 该 ~1s 呼吸暂停（**美术限制**：idle 不该 key spine，需美术修才能让呼吸与耳尾并存；当前用呼吸对齐把暂停点放在轮回边界，视觉上像"呼吸一下→动一下耳→再呼吸"）。
- 未做：Phase 3 emotion→表情 slot 持续映射（mood 高→笑眯眼/小笑嘴，rest_need 高→半睁眼，需 spine.update 后 slot override）；Phase 4 凝视追指针（neck/head 骨骼旋转）；Phase 5 测试面板（Debug Panel Spine 按钮）；`Live2DCanvas` 占位待删。
- FSM 14 behavior vs Spine 10 动画非 1:1，目前仅 Embarrassed 接 wink，余待映射。

---

## §最近一轮 (2026-08-10 续¹²)：Liri Spine 全身显示 —— 两个 release-only bug 修复

**背景**：里程碑1（2957cf6）声明接通 Spine 渲染链路（加载+显示+body_breath），但 release 实跑暴露两个 **dev 模式隐身**的 bug（dev tauri 自动放宽 CSP，故 dev 永远正常，踩坑#7 同类）。用户反馈：① 重启后空白 ② 只显示上半身。

### Bug 1：release 空白 = CSP 缺 worker-src
PIXI/pixi-spine 启动建 `blob:` Web Worker。CSP 未设 `worker-src` 时回退 `script-src`，而 `script-src` 仅 `'unsafe-eval' 'wasm-unsafe-eval'`（无 `blob:`）→ worker 创建被阻 → PIXI Application 崩 → 画布空白（后端/React 正常，极难排查，同踩坑#7 PIXI 崩的隐蔽模式）。**修复**：`tauri.conf.json` CSP 加 `worker-src 'self' blob:;`。

### Bug 2：只显上半身 = pixi-spine getBounds scale 缓存
`SpineBase.update(dt)` 在 update 时刻把 mesh vertices 烘焙进缓存（此刻 scale=1）；之后的 `scale.set(fit)` **不重算**这些 vertices，故 `getBounds()` 永远返回 **scale=1 未缩放尺寸**。原 centering `(H - b.height)/2 - b.y` 信任了这个 post-scale 谎言 bounds → 璃被推到 world y∈[400,940]，可见区 [0,600] 只露头肩。CDP `Runtime.evaluate` 量出真实数值坐实（spine.y=940.5 时确为上半身）。**修复**：scale=1 时量 `b1 = getBounds(true)`，手动做缩放 centering——`spine.y = H/2 - (b1.y + b1.height/2)*fit`，worldBounds 手算 y∈[30,570] 全入画布。click hit-testing 的 on-screen bounds 同从 `b1×fit` 手推（post-scale getBounds 不可信）。

### 验证
- CDP 控制台 CSP worker 错消失；截图 base64 185768→325644 chars（≈翻倍，全身像素量）。
- 数学验 worldBounds y∈[30,570] ⊂ [0,600]；用户目视确认**全身显示**。
- `npx tauri build --no-bundle` exit0（先 `taskkill //IM desktop-pet.exe //F` 避坑#6）。

### 诊断方法笔记（防重复造轮子）
release 无 DevTools → WebView2 `--remote-debugging-port=9222` + CDP（`Runtime.enable`/`Page.captureScreenshot`/`Runtime.evaluate` 轮询 `window.__spineDiag`）。`analyze_image` 对此例**不可靠**（上半身截图两次误判"完整"）→ **数值诊断优先于视觉模型**。诊断脚本用完即删，不入库。

---

## §最近一轮 (2026-08-09 续¹⁰)：选择性遗忘 —— 多轮消歧义 + fact/pending 语义匹配

**任务**：08-05 续做的选择性遗忘（episode/fact/pending MVP）是**单轮、零状态、最高分赢家通吃**——gate→`Forget`→`forget_best_match` 扫三路各过 0.7 门、取置信度最高**直接删一个**，无候选则 converse 注入"不记得"。用户确认两个体验缺口都要解：① 多候选不澄清（猜删可能删错，违背 #1「Rust 绝不删错东西」）② 措辞不匹配太硬（char_overlap 字面不重叠，「忘掉早睡的事」匹配不到 fact「想早睡总是熬夜」）。实现深度定为**完整跨轮反问**：多候选→反问→slot 存候选→接第二轮→删指定。

### 关键约束（codegraph + 源码坐实，非假设）
1. **第二轮 gate 不进 Forget**：Forget 是动词驱动（"忘掉/删/取消"），"第一个/前者"会被分到 Silence → **接第二轮必须在 gate 之前拦截**，不能依赖路由。
2. **converse 对 AppState 是"瞎的"，但有跨轮注入范式**：`ConverseCtx.pacing: &Mutex<QuestionPacing>`（`converse.rs:69`）是现成的 turn-spanning slot → `pending_forget` 照抄，零架构新概念。
3. **踩坑#4 雷区**：改 `converse`/`ingest` 签名会断所有 harness。本轮**只加 enum 变体 / struct 字段，不改函数签名**；`ConverseCtx`/`AppState` 加字段则同步所有构造点（3 harness + lib.rs init + commands.rs send_message）。prompt_quality case 1009 种子下**双候选**（fact「想早睡总是熬夜」+ episode「熬夜写代码…早睡」非地标）→ 新逻辑从 ForgetAck 翻成反问 → 启发式判 FAIL，**必须同步**。

### 模块 A：多候选反问 + 跨轮消歧义（`forget.rs` 主体重写）
**新类型**（替 `ForgetResult`）：`ForgetOutcome::{Deleted{summary}, Declined, Ambiguous{candidates}}` + `PendingForget{query,candidates,created_at}` + `ForgetCandidate{target,id,summary,confidence}`（均 `#[derive(Debug,Clone)]`）。三态比 bool+Option 清晰：删了一个 / 诚实拒绝 / 需反问。

**`forget_best_match` 改纯决策**（`forget.rs:307`）：三路候选（含模块 B 语义匹配）→ `≥2` 返回 `Ambiguous`（**不删**，landmark 已被 episode 腿过滤，候选皆可删）→ `==1` 删它 `Deleted` → `0` `Declined`。纯决策**不碰 slot**（#1：Rust 决定删什么；slot 读写归 converse）。

**第二轮解析纯函数**（可单测，无 DB/模型）：`resolve_candidate` 先 `ordinal_index`（第N个/前者/后者/最后/1/A/甲乙，含 `cjk_to_digit`）→ 命中返回索引；否则各候选 char_overlap 取最高≥0.4；都不中 None。`is_off_topic`：无序数且全候选 char_overlap<0.2 → 判换话题（保守：疑似仍在话题内就留循环重问，只对明确新话题清 slot）。

### 模块 A 续：converse 控制流汇合（`converse.rs`，最复杂）
**入口（ingest 之前）拦第二轮**（`converse.rs:209`）—— `resolve_pending_forget`：take-and-clear **一次锁**（>90s stale drop，clone 候选出作用域后不持锁跨 DB 擦除）→ `resolve_candidate` 命中 → `execute_candidate` 删第 i 个 + 清 slot → `Resolved`；off-topic → 清 slot + `Proceed`（正常 ingest）；仍不明 → 清 slot + `Reask(candidates)`（**只重问一次**，slot 已清防循环）。

**跳过 ingest**（`converse.rs:210`）：Resolved/Reask 合成 `IngestionOutcome{route:Silence,…全 None}`——二轮"第一个"**绝不存为新记忆**（erase 已在 resolve 发生，ingest 只会污染）；但 emotion/retrieve/plan/chat 仍跑以产出回复。

**注入块**（`converse.rs:468`）：`Resolved` → "好我忘了"确认提示；`Reask` → `disambig_prompt`（列候选 summary 让 LLM 自然问"你说的是 A 还是 B？"，引真实摘要减少编造）；`Proceed` → 看 `outcome.forget`：Deleted/Declined 照旧，Ambiguous → **写 slot**（PendingForget）+ `disambig_prompt` 反问。三路径汇合既有 chat 生成。

### 模块 B：fact/pending 语义匹配兜底（`forget.rs`）
`find_fact_candidate`/`find_pending_candidate` 加 `embedding: Option<&EmbeddingService>`：char_overlap 粗筛 → 若 `emb.is_ready()` 调 `semantic_rerank`——**char_overlap top-5 现场 embed_batch + cosine**，`cosine_similarity` 未归一故 `((cos+1)/2).clamp(0,1)` 映射匹配 `retrieval::compute_semantic`，0.7 门读法不变；embedding 任意 hiccup 退回 char_overlap（#6 优雅退化）。效果：「忘掉早睡的事」语义命中「想早睡总是熬夜」。成本：每次 forget 最多 1 query + 5 value embed（forget 低频，可接受）。

### 模块 C：harness 同步（踩坑#4 全程未踩）
- `ForgetCandidate` 漏 `#[derive(Debug,Clone)]` → `ForgetOutcome`/`PendingForget` 的 Vec 成员要求它 → 5 处编译错全此一因，加 derive 即解。
- `IngestionOutcome.forget` 字段名不变、类型 `ForgetResult`→`ForgetOutcome`，ingest Forget 分支类型自动推断无需改。
- 3 harness（conversation_harness / memory_recall / prompt_quality）每个 ConverseCtx 构造点加 `pending_forget: &Mutex::new(None)`（memory_recall 有 3 处用 replace_all）。
- prompt_quality：`Expect::ForgetAsk` 启发式（回复含 哪/还是/具体/哪个/哪件/哪条/哪段 即 pass）已加但**当前无 case 触发**——case 1009 经验证为单候选（见下「修正」），保持 ForgetAck；case 1002/1005/1007（单候选/0候选/单候选）亦 ForgetAck。

### 验证
- **lib 293 passed**（forget 18 测含 6 新：`forget_best_match_ambiguous_keeps_both` 钉 ≥2 候选不删、`resolve_candidate_ordinals/keyword_overlap/unresolvable/out_of_range`、`is_off_topic_detects_new_subject`；语义路径 embedding=None 退回 char_overlap 由 `find_fact_candidate_below_gate_is_none` 等覆盖 #6）。
- **cargo check --tests ✅**（17 测试二进制全编译，含 3 harness ConverseCtx 同步）。
- **release `npx tauri build --no-bundle` exit0**（17:20，`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 24.4MB；先 `taskkill //IM desktop-pet.exe //F` 避坑#6 文件锁）。
- ✅ **prompt_quality G10 全 9 例 hard-check 0/9**（真模型，含修正见下）。

### 修正（commit 9bc3dac）—— 语义精排假阳性 + 1009 种子假象
1. **BGE-M3 假阳性根因**：首轮 harness 跑出 1002/1005/1007 误判 Ambiguous 引用无关 fact（早睡/实习）。根因——`semantic_rerank` 原本对**所有** fact（含 char_overlap=0）做语义精排，而 BGE-M3 无关文本基线 ~0.5 raw cosine → `((cos+1)/2)` 映射后 **0.75 > 0.7 遗忘门** → 无关 fact 被伪造成候选（"忘掉火锅"误命中"想早睡总是熬夜"）。**修复**：语义精排改为**只提升 char_overlap>0 的条目**（字面锚点过滤基线噪声），同时仍能捕捉近义（"忘掉早睡的事"→"想早睡总是熬夜"共享「早睡」）。lib 测用 embedding=None 不触发该路径故未暴露，harness 真模型才暴露——**遗忘这类「无 fallback 的语义门」改动必须真模型验**。
2. **1009 是单候选非双候选**：Fix B 后 1009「忘掉早睡」只命中早睡 **fact**（Deleted→"好，我忘了"）。规划时假设的 episode「熬夜写代码早睡」**未被 episode leg 命中**——`find_episode_candidate` 用 `retrieve(top_k=1)`，种子 offer 地标（importance 0.9）blend 排序第一 → should_forget 拒（地标）→ 早睡 episode 排第二根本没被看到 → episode leg 返回 None。**这是 episode leg 既有 top_k=1 局限（08-05 至今）+ 种子假象**（生产中"忘掉咖啡"无地标干扰 → episode leg top-1 直接相关 episode → 双候选正常）。期望回退 ForgetAck；多候选 Ambiguous 路径由 lib `forget_best_match_ambiguous_keeps_both` + D15 手动覆盖。`Expect::ForgetAsk` 启发式保留（识别合法反问行为，留待 D15 自动化或新增干净多候选 case）。

### 待实跑（D15）
dev 聊出两条同主题记忆（如偏好「猫」+ 一次「和糯米看猫」episode）→ 发"忘掉猫" → 见**反问**（"你说的是哪个？"）→ 答"那次经历"/"第二个" → 见"好，我把那段忘了"+ Debug Panel 确认 episode 删、fact 保留；再测序数"第一个"、换话题清 slot、90s 超时。

---

## §最近一轮 (2026-08-09 续⁹)：记忆卫生层 —— 结构性治三类易复发缺陷（写闸门 + 检索纯化 + 去重视野）

**任务**（用户原话）："1先观察，2治理。另外，不能只是完成这一次治理。你需要设计更好的结构来承担记忆任务，避免之后出现同样的或者类似的问题。设计完成之后需要自己复盘3次（合理否/会否引新问题/有无更优解）。先不要急着自己造，去其他地方看看有没有可以直接复用的框架。设计并复盘后自主执行并进行测试。"

### 调研（firecrawl，决定"不造什么"）
- **mem0**：`REJECT` 闸门 + ADD-only（无原地改）+ `supersede_by` 软废弃链；**V3 已砍 LLM-as-judge 二次校验**（V1/V2 的 extract→verdict 引发回归 + 成本，业界收敛到确定性规则闸门）。复用其负向规则 + 软废弃形态（我们 `expire` 机制已是）。
- **MemGPT / Letta**：blocks + 容量上限 + CAS（archive）+ **sleep-time 后台 worker**（把状态维护关进后台）。我们 consolidation + loop_runner slow_tick 已是这个形态。
- **Zep / Graphiti**：bi-temporal 知识图谱（节点+边带 valid_from/valid_to）。判 **overkill**（39 facts/单用户/成本#8 规模），且我们 `facts(valid_from/valid_to/source_episode)` 已是 bi-temporal 形状。

### 三类结构性缺陷（读码定位，非一次性脏数据）
| 缺陷 | 根因（代码） | 表现 |
|---|---|---|
| **A 抽取无校验** | `store_fact` 全盘信任 extractor 输出 + LLM 自打 confidence；extractor prompt 写对但 LLM 违规 10-20% | "太阳东升西落"conf0.98、"user is asking about my dreams"、知识问答入库 |
| **B 读路径强化** | `retrieve()` 每次**读**都副作用**写** `reinforce()`（+strength、+recall_count）；forget / proactive / **测试** 都触发 | recall_count 刷爆(382/445/446)、strength 饱和钉 1.0、富者愈富 |
| **C 去重视区** | `converse.rs:94` known_facts 只拉 `preference` 类 | 糯米跨 relationship/preference/profile 碎片化、extractor 看不到 → 重抽 |

> ⚠️ **复盘纠正**：原判 strength"只升不降"。**错**——`db::episodes::decay_strength`（×0.998/天）已在 `loop_runner.rs:309` 每日运行。B 的真正根因是"读路径也强化"，不是"无衰减"。

### 设计：两层确定性卫生（LLM 只提议，Rust 校验，原则 #1）
- **Part 1 写入闸门**（治 A，新 `mind/memory_gate.rs`）：`admits(fact)->bool` / `filter_facts`，无 LLM、可单测，`store()` 写库前调用。三条独立 deny：① category 白名单（preference/relationship/goal/profile/school/work/health，对齐 extractor.txt）；② 噪声 key（结尾 `_question`/`_gap`/`_knowledge` 或 `belief_in_*` 前缀——中文 trivia "太阳东升西落" 的 key 是 `knowledge_question`，靠此抓）；③ 噪声 value（英文 + 对齐 proactive `is_anchorable_fact`：asked about / asking about / user asked / user is asking / does not know / curious about / busy with work…）。
- **Part 2 检索纯化**（治 B，**零签名变更**）：`retrieve()` 删 reinforce 副作用 → 纯读；新增 `reinforce_top(db, episodes)` 辅助，仅 genuine-recall 调用方用（converse 非 QA / proactive 3 处）。**不新增衰减**（decay 已存在）。**为何不用 `reinforce:bool` flag**：retrieve 回归纯函数语义更清 + 签名零变更 → forget/tests/embedding_ab 调用点无需改（避坑#4）。
- **Part 3 去重视野**（治 C）：`converse.rs` known_facts `get_by_category("preference") take(20)` → `get_all_active(30)`（按 mention_count/confidence 排序）。

### 三次多视角复盘（设计定稿前，全文见 ADR）
1. **架构/正确性**：纠正 B"无衰减"为假 → 砍新衰减子系统；value 黑名单全英文漏中文反例 → key 黑名单兜住；旧 `test_strength_reinforcement` 会挂 → 改纯读契约 + 新 `reinforce_top` 单测。
2. **回归/副作用**：签名零变更确认；两个固定断言会失败（retrieval + gc_008）→ 已改；迁移误杀 `current_reading` → 改显式 expire（非 blanket 重放）；stale 注释（forget/embedding_ab）→ 已更新。
3. **小马尾/更优解**：砍 ~40% 代码（filter_facts 内联、`reinforce_top` 替 flag、衰减子系统全砍）；known_facts 全类保留。

### 不做（复盘收敛）
知识图谱（overkill）/ LLM judge 二次校验（翻车+成本）/ 新衰减子系统 & importance 地板（decay 已有效，无过衰减证据，地板治未病且可能保噪音）/ `enable_memory_gate` kill-switch（gate 与 `dedup_insert`/`expire_old` 同属零成本确定性 ingest 闸门，后者也无 toggle；#6 kill-switch 专给昂贵/LLM 能力省成本，gate 无成本可省；threading config 进 store() 是坑#4 级签名动荡）。

### 数据治理（一次性，用户 #2）
`scripts/migrate_memory_hygiene.py`（python sqlite3，镜像 memory_gate 模式 + 重置测试期饱和 strength，dry-run 默认 / `--apply` 提交，先备份 `.bak-hygiene`）。**执行结果**：expire **10 噪声 facts**（知识问答/自我语境/越界类，保留 current_reading + 糯米副本）+ **19** 非地标 episode strength snap 回 importance → facts 36→26 active、episodes 0 饱和（原 7）、排序现按 importance（小猪去世 0.8 居顶 / 素数 trivia 0.1 落底）。recall_count 不动（不参与评分，仅诊断）。

### 闭环2 测试途中修了续⁸ 既存 bug（非续⁹ 回归）
`cargo test --test closed_loop2_harness`（真实 LLM）首跑 **FAILED**：`proactive_bubble_brings_up_due_pending` 断言 pending 被触发，但 generate 走了 lively 分支（"伸了个懒腰…"）跳过到期 pending。**根因**：续⁸ 的 lively 70% 概率早返回（`proactive.rs:210` `gen_range(0..100)>=30`）在 `pending_due` 检查**之前** → 到期提醒被 70% 随机跳过。续⁸ 当时只跑 `check --tests`（编译）没跑 harness，漏掉。**不是续⁹ 回归**（测试用全新内存 DB，lively 早返回我未触碰，我的 reinforce_top 只在 non-lively 分支）。但它**破坏核心承诺**（北极星：到期提醒该被带出）。**一行守卫根因修复**：`is_lively = pending_due.is_empty() && rng.gen_range(0..100)>=30` —— 到期提醒在则强制走 memory 分支（确定性触发 mark_triggered），无到期提醒时 70/30 多样性原样保留（尊重续⁸"先观察"）。`generate_welcome_back` / `generate_lonely_bubble` 无 lively 概率分支，不受影响。**修复后闭环2 ✅ 1 passed**（anchor="明天有个大公司的实习面试" goal=care，pending anchored: true）。

### 验证（全绿）
`cargo test --lib` **287 passed** / `--test golden_conversations` **29 passed** / memory_gate 6 单测 / `--test closed_loop2_harness` **1 passed**（真实 LLM）/ 17 测试二进制全编译零签名破坏。commit `7f4af17`（卫生层）+ proactive 一行守卫（待提交）。

### 改动清单
新 `mind/memory_gate.rs`（admits/filter_facts + 6 单测）；`mind/mod.rs`（注册）；`mind/store.rs`（写库前过闸门）；`mind/retrieval.rs`（删 reinforce 块→纯读 + reinforce_top + 测试改纯读契约）；`mind/converse.rs`（known_facts 全类30 + 非 QA reinforce_top）；`pending/proactive.rs`（3 处 reinforce_top + **续⁸ lively 守卫**）；`mind/forget.rs`+`tests/embedding_ab_harness.rs`（stale 注释）；`tests/golden_conversations.rs`（gc_008→纯读契约）。ADR + 治理脚本。**release 待 `npx tauri build --no-bundle`**。

---

## §最近一轮 (2026-08-09 续⁸)：自主冒泡频率修复 + 灵性重构（记忆30/灵性70）

**起因**：用户体感——自主冒泡①频率太高（几分钟一次）②内容单一（全和糯米有关，要像真人突然找你聊天，话题任意，可自言自语/撒娇）。firecrawl 调研（companion app 主动对话：频控靠 cooldown、内容靠多类型+情绪+时段驱动，避免单记忆锚定重复）+ AskUserQuestion 定：频率=30min（修 bug + config 可调）/ 比例=记忆30:灵性70。

**频率根因（bug 非 design）**：`commands.rs:470` `let last_bubble = chrono::Utc::now() - chrono::Duration::minutes(31);` 硬编码——每次都满足 trigger_proactive Rule2（elapsed<1800 → 31min>30min 恒过）→ 前端 5min 轮询（App.tsx:407）每次拿 action → 高频。30min 设计（proactive.rs MIN_BUBBLE_INTERVAL_SECS=1800）本身对，是上游传参造假。

**内容根因（design 倾斜）**：`proactive.rs::generate`(168-259) ① 固定 query 每次召回同一批（糯米=强记忆）② 强制三选一 anchor（pending>fact>episode），无锚点 `Ok(None)` 沉默 ③ prompt "只能围绕它原意...绝不能换别的" → 永远糯米。

**改动（4 文件 surgical）**：
- `config.rs`：新 `ProactiveConfig{min_interval_secs:i64}` 默认 1800，`#[serde(default)]` 进 AppConfig（旧 config.toml 无 [proactive] 段用默认，无需改 AppData）。
- `commands.rs`：AppState 加 `last_proactive_bubble: Mutex<Option<DateTime<Utc>>>`；check_proactive 读真实值（None→now-36500days 哨兵，elapsed 巨大放行首次）传 trigger_proactive，**过门控即占位** `*t=Some(now)`（在 proactive_bubble 生成前；生成失败/None 也不让 5min 轮询区间内重复触发，conservative 宁少勿突兀）。
- `pending/proactive.rs`：① trigger_proactive 加 `min_interval_secs` 参数（删常量，Rule2 用参数；6 单测调用点同步+1800 踩坑#4）。② generate 入口 rand 加权：rng 收敛块内（ThreadRng 非 Send 不能跨 await）算 `(is_lively,query)` 后 drop；`>=30` 走新 generate_lively；memory 分支 query 从 `MEMORY_QUERIES`(5 条) 随机选 + 无锚点降级 lively（不再沉默）。③ 新 `generate_lively`(70%)：**不调 retrieve**（省 embedding）用 `RetrievalResult::default()`——空检索让 grounding_guard 自然禁任何用户过往编造（只能说自己的感受/环境/时间）；Intent goal=converse/tone=`lively_tone`(mood≥.7→playful/lonely>.6→gentle/else curious)；prompt=`lively_prompt(emotion,hour)` 纯函数——注入本地时段（format("%H")→早上/快中午/下午/傍晚/晚上/深夜）+情绪（想ta/不错/平静/闷闷），"此刻心里冒句话"（自言自语/撒娇/碎碎念，禁总结/套话/逼问），过 grounding_guard + record_interaction。④ 3 新纯函数测（lively_tone 三分支 / lively_prompt 六时段+防幻觉 / min_interval 可配证参数生效）。
- `lib.rs`：AppState 构造点初始化 `last_proactive_bubble: Mutex::new(None)`。

**两编译坑（已修）**：① ThreadRng(Rc-based) 非 Send 跨 await → tauri Future 不 Send → 收敛 rng 到独立块 drop。② chrono Timetrait::hour() 解析报错 → format("%H").parse() 不依赖 trait。

**验证**：cargo test --lib **280 passed**(277+3) / cargo check --tests ✅（generate 签名未变故 harness 无波及）/ release `npx tauri build --no-bundle` exit0（1m10s Rust+2.64s 前端，前端未改 CSS hash 不变）。

**待实跑**：观察 ① 冒泡间隔≈30min ② 内容多样性（**续⁸b 已用 harness 验证**，见下；Debug Panel Last Turn action=lively_bubble vs proactive_check 区分）。可调：AppData config 加 `[proactive] min_interval_secs=900` 改频率。

**续⁸b（2026-08-09，lively prompt 反同质化，commit 4a7516c）**：续⁸的 `bubble_content_check` harness 第一轮暴露 lively 气泡雷同——hour=11/loneliness=0.85 固定情境下 7 条全"快中午了+阳光/太阳+想你"变体。**根因**：`lively_prompt` 把 `time_desc`/`mood_desc` 当**成品词**直接拼进 prompt 句首（"现在是快中午了，你有点想 ta"），LLM 惰性照搬这两个词作输出骨架。**修复**（`proactive.rs` surgical +18/-15）：① `time_desc`/`mood_desc` → 描述性 `time_hint`/`mood_hint`（"中午时分"/"心里莫名有点空"，非可直接照搬的成品短语）；② 配 `time_avoid` 显式禁各时段套路报时词（快中午了/早上好/夕阳…）；③ 通用禁套路「忽然/突然+想你」「阳光正好/太阳正暖」；④ 强调具体小切入点菜单（动作/细节/身体感/荒唐念头/自言自语）+"不是打招呼、不是表达关心"破套话退路。新增 `tests/bubble_content_check.rs`（N=15 真实 LLM 内容回归资产，校验 70/30 比例 + 0 编造/套话/多重提问）。**第二轮验证**：11 lively/4 memory=**73:27**，0 编造/套话/多重提问全过，反套路词 0 命中，lively 11 条各异（数灰尘/影子/团成一团/后背咔哒/哈欠叹回/屏幕发烫/饭菜香/肚子咕噜），同质化根除。**残留**：伸懒腰动作重复 4 次（后续各异，可接受）；memory 仍 100% 糯米（**记忆数据集中度，非 prompt 问题**——库里糯米是唯一强记忆，等用户积累更多记忆自然分散）。

**续⁸c（2026-08-09，lively 允许轻好奇提问，commit 5075db2）**：用户反馈"不必全陈述句，也可以对我在做什么或其他事好奇并提问，像真人"。续⁸b 的 prompt 完全排除提问（"不是要 ta 回答" + 切入点全陈述），过严。改 `lively_prompt`：① "也不是要 ta 回答" → "也不一定要 ta 回答"；② 切入点菜单加"此刻有点好奇的小问题（ta 在忙什么/累不累/小疑问）"；③ 反套路补「在吗/在干嘛/有事吗」+ "想问只问一个、不必答、别追问别查岗"。守住单问号（multi_question 检测）+ grounding_guard 仍拦编造（lively 无 anchor）。**验证**：13 lively/2 memory，0 编造/套话/多重提问；提问自然出现（#15"你那边现在是晴是阴？"）。**本轮 lively 多样性较续⁸b 下降**（"打哈欠×4/晒光×6/犯困×3"聚集）= hour=11 固定情境放大收敛，真实多时段流变会分散；提问率 1/13 偏低（小样本方差）。**判断**：同质化是概率性非确定性 bug，继续加禁词是打地鼠（续⁷ Ali:Chat 教训：规则压不过模型倾向），不再追加禁词；真实使用观察后再定。

**memory 诊断（续⁸c 旁支，非改码）**：查 `%APPDATA%/DesktopPet/desktop_pet.db`——facts 39/episodes 21/pending 3。糯米 dominate 因：① 宠物是库里**唯一成簇**记忆（8 facts+5 episodes，confidence 0.80–0.98 最高档），向量检索 top-3 必被簇包揽；② 其他记忆（奶茶/星际穿越/实习/考试）语义稀疏单点斗不过多点簇；③ 测试期反复聊糯米，真实分布。**附带发现两问题**（待用户定夺是否治）：extractor 误抽（"太阳东升西落"等知识问答当 fact 存，conf 0.98~1.00；"dream_interest=user is asking about my dreams" 把桌宠被问语境误存用户 fact）；recall_count 被测试刷爆（桌宠 rc382/429、火锅445、素数446、work476，memory_strength 钉死1.00）污染检索加权。

## §历史详细（2026-08-08 续⁷ → 2026-07-26，已收尾·压缩归档）

> 08-07 及更早各轮**压缩摘要**（逐轮全文已查 git 历史 + 上方 §当前任务 对应日期条目）。仅保留：交付了什么 / 关键决策与 ADR / 新踩坑。"实现细节/架构契合/验证日志"不再保留（`git log` / 对应 commit 可查）。

### 2026-08-08 续⁷ 速度+性格+记忆幻觉根因
- 6 轮 A/B。main 回复关思考（`converse.rs:415 ThinkingConfig::disabled()`）→ FULL max4s/mean2.7s/0 超 5s。grounding 显式标记空记忆（`grounding.rs:290/293`）→ fresh 组幻觉全 0。system.txt round-2 soul block+8 样例 → 性格回归（human 4.07）。**披露 G6 越界 6/10 = 性格同源 trade**（"上次说"framing 是用户要的连结，不可全除）。速度达标 → **option A（gate/extractor 并行）不做，留 backlog**。commit `13e7dc8`。

### 2026-08-08 续⁶ 真人感 prompt 调教
- 150 条 A/B + 真人感指标 + `CASE_FILTER`。system.txt 反 AI 味 4 条 + engage"可不问"；样例 4→6 仅 1 问。提问结尾率 35%→**14%**，G3/G12 大降。human 4.24→4.11（变短非变冷）。报告 `docs/review/realism-report-2026-08-08.md`。commit `b5afac6`。

### 2026-08-08 续⁵ BrainState 扩 prompt/budget —— **经评估关闭（ADR）**
- BrainState 边界终态 = planner（旗舰纯决策）。字面扩进 5 函数不相容（intent 循环 + 捆绑 3 无用字段 + 踩坑#4 级签名动荡 + 零价值）→ 否决。**ADR `docs/decisions/2026-08-08-brainstate-prompt-budget.md`**。纯决策无代码行为变更，无需 rebuild。

### 2026-08-08 续⁴ idle_weights JSON 化
- 微行为权重表数据驱动（JSON 配置，非硬编码）。

### 2026-08-08 续³ 害羞慢现气泡
- 后端 closeness-aware mood 标签（关系早期害羞）。verify-checklist D14 待人验。

### 2026-08-08 续² Alt+Space 全局唤醒（P11.4）
- 全局热键唤醒桌宠窗口。

### 2026-08-08 续 B5 三层人格评估
- LLM-as-judge 第三线（judge_persona temp0.1/max2048 + 3 次指数退避重试防 rate-limit 静默零分）。

### 2026-08-08 自主批次（深度专注接线 / Scheduler 观测 / Grounding B 档 / 全局 BrainState）
- Scheduler 观测层（`lifecycle/scheduler.rs` 进程级注册表 11 任务 + `[scheduler]` enable flag + DebugPanel 分区），**取代**旧 deferral ADR。全局 BrainState `mind/brain_state.rs::BrainState<'a>`（5 借用字段零 clone）→ `planner::plan` 5 散参合并（采纳边界=planner，踩坑#4 命中 10 调用点已修）。

### 2026-08-07 续² 自主批次（鲁棒性 / BrainState / 记忆编辑 / loneliness 收尾 / 死代码）
- 见 §当前任务对应条目 + 逐项审计清单。

### 2026-08-07 关系进展摘要（Hermes 后台 review）
- 每 15 新 episode，后台 reflection 产出 1-2 句关系总结，注入为 always-on `[Relationship]` 区块。新表 `relationship_reviews`（migration v3 + `db/relationship_reviews.rs`）+ `soul/review.rs` + RetrievalResult 加字段走现成注入管道 + slow_tick + budget=80。踩坑#4：RetrievalResult 加字段同步所有构造点（lib + harness）。

### 2026-08-07 续 激活 loneliness —— 璃会"想你"
- loneliness 曾是死字段（`apply_homeostasis_time_aware` 只更新 5 字段漏 loneliness）→ 镜像 rest_need 修法（`needs.rs::tick_loneliness` + SQL UPDATE）。主动气泡 `generate_lonely_bubble` + `check_lonely_nudge`（门控 loneliness>0.6 + closeness≥20 + Active + 非对话中 + 30min cooldown）。closeness≥20 保证早期不主动找你（安全阀）。

### 2026-08-05 选择性遗忘扩展 fact/pending + **FTS5 可行性证伪**
- **FTS5 对中文不可行**（trigram 需≥3 字 / unicode61 不分 CJK / ascii 只认 ASCII；"sqlite-vec 自带 fts5_cjk"为误）→ **从 backlog 移除，勿再尝试**（除非引入 jieba 扩展）。转向 fact+pending 遗忘（镜像 episode）：`forget_best_match` 三路调度 + 0.7 语义门 + 新 `char_overlap`（bigram 重叠系数 `|A∩B|/min`）。

### 2026-08-04 续④ 修复 opencode 续③ QA 直答 4 问题
- ① QA 模式丢身份层（补 persona/relationship/user_profile DB 读）② QA budget→`qa_system_prompt_budget()=505` ③ qa_mode 强制 action=normal ④ qa_mode 跳 grounding check。

### 2026-08-04 续③ QA 直答路由 + system.txt 正向重写 + Hermes 记忆优化
- 知识问答走 QA 直答路由（跳 episodes/facts 防跑偏，保留璃身份）。

### 2026-08-04 续 选择性遗忘 episode MVP
- 用户说"忘掉X"→ gate Forget → retrieve 语义匹配最佳 episode → **置信度门在 `score_breakdown.semantic`（0.7，非 total）** + landmark 保护 → Rust 硬删 + vectors::delete → converse 确认（禁复述）。新 `mind/forget.rs` + `db/episodes::delete`。

### 2026-08-04 续 dev 构建重定向 D 盘
- `.cargo/config.toml` 移到项目根，CARGO_TARGET_DIR→`D:\cargo-target\desktop-pet`。

### 2026-08-04 #10 生命感收尾
- rest_need 暴露+激活 + circadian speedModifier 接动画。

### 2026-08-03 续⑧ B4-余余 Debug Panel 补全 + B5 Golden 评估框架
- 见 [§审计 (2026-08-03 续³)](#审计-2026-08-03-续③深度审计--代码级核验)。

### 2026-08-03 续⑦ sleep 内容首次有测试
- 加 vitest（devDep + vitest.config.ts）+ 抽纯逻辑 `sleepLogic.ts::shouldAutoSleep` / `microBehavior.ts::applySleepyWeight` + 24 前端单测。CDP 自动化验证 A4/A5/B3（`window.__pet` dev-only 钩子，重写 `Date.prototype.getHours` 模拟时段）。**唯一待人验：B3② sleep 音效**。

### 2026-08-03 续⑥ 清测试
- golden_conversations 2 stale 测试（gc_003 焦虑→care 有意改 / gc_012 中文维度 `温柔`）改测试不改生产。确定性测试全绿 251。

### 2026-08-03 续⑤ Settings 下载 + 暂时离开→托盘
- 下载 Qdrant 401→Xenova(hf-mirror) + external data 子目录处理。暂时离开最小化到系统托盘（`hide_to_tray` + TrayIcon 左键恢复；**原只设 awayMode 标志窗口根本没隐藏**）。

### 2026-08-03 续④ BGE-M3 embedding 接入 + 检索质量翻倍
- BGE-M3 装 `D:\models\bge-m3`（Xenova/bge-m3）+ **修 ort rc.12 加载 bug**（Level3→All；记忆 `ort-rc12-embedding-load-bug`）+ backfill。语义 Hit@3 33%→67%、avg sem 0.035→0.741。**follow-up**：download.rs HF_BASE_URL 仍指失效 Qdrant（Settings 下载按钮坏，手动下载已绕过）。记忆 `bge-m3-model-location`。

### 2026-08-03 续③ 深度审计 + #11 可观测簇
- 对照 implementation-plan P0-P17 逐项核验代码实际状态（见 [§审计](#审计-2026-08-03-续③深度审计--代码级核验)）。B4b conversations 死表修复（生产从未调 `conversations::insert`）→ B4-MVP（Retrieved+Intent+Reflect 三分区）→ B4-余 Cost（LlmClient 今日调用+token）。实跑首次暴露单轮 3 次 LLM 调用。

### 2026-08-03 合并 opencode 副本
- B1 Consolidation 反向更新 Facts + B2 完整物理（自由落体/任务栏弹跳/飘落悬停）+ A4/A5 实跑方法论。两副本 base 同 HEAD，纯增量零冲突。**踩坑**：`startDragging` 吞 webview 鼠标事件（拖拽结束不能用 mouseup，必须 `win.onMoved()` + 静止期）。

### 2026-08-03 续² Liri 角色方向 + 人格落进 system prompt
- 最终角色=璃 Liri（小狐灵），动画走 Spine+PixiJS（不用 Live2D），Live2DCanvas 占位待迁。设计三文档入 `docs/specs/liri/`。system.txt + `firstrun.rs::seed_persona`（中文 key，仅新装生效）。记忆 `liri-character-spine-direction` / `liri-design-bible`。

### 2026-08-03 续 B3 Sleeping 配套收尾（纯前端）
- 睡着抑制 nudge + sleep 音效（`soundManager.ts` sleep()）+ LateNight 不入睡只 yawn（已满足零改动）。dev-only `window.__pet` 验收钩子 + `docs/verify-checklist.md`。

### 2026-07-31 19:10 主动开口幻觉 grounding A 档收紧
- proactive 收紧：retrieve 锚 + intent goal 驱动 + 空检索不说话。残余：prompt 软约束无运行时阻断，B 档待命（B1b 条件触发）。

### 2026-07-31 续 气泡 release rebuild + consolidation 修复 + Reflection 触发器 + Sleeping 入睡
- **踩坑**：dev HMR ≠ release exe——前端/CSS 改动 dev 热更"看着修好"但桌面快捷方式不自动更新，必须 `npx tauri build --no-bundle`（涉及前端/CSS 的"实跑通过"必须在 release exe 上确认）。

### 2026-07-31 早些 Foley 接线补全 + 频率调整 + 气泡位移
- 实跑通过 ✅。

### 2026-07-31 converse 注入 surfaced thought（Tier2 #4）
- 不改签名（converse 内部加逻辑）+ 不增 LLM 调用（#8 复用同一次 turn）。

### 2026-07-29 Foley 音效真实素材接入
- `import.meta.env.BASE_URL` 无类型→硬编码 `/audio/`。实跑通过 ✅。

### 2026-07-29 circadian 接入微行为（Tier3 #7）
- 时段权重调制微行为。

### 2026-07-29 气泡生命力 — 打字节奏随情绪 + 无文字气泡（Tier1 #2）

### 2026-07-28 流式回复 emit/listen → ipc::Channel
- **踩坑（release 重建，已进 CLAUDE.md 踩坑#6）**：`npx tauri build --no-bundle` 覆盖 exe 时桌宠**正在运行**→ Windows 锁文件→`failed to remove file 拒绝访问 (os error 5)`。构建前必须 `taskkill //IM desktop-pet.exe //F` + sleep ~3s。

### 2026-07-28 情绪外显·连续表情插值（P10 emotionBridge）

### 2026-07-27 Soul 慢循环闭环
- Reflection 自动调度 + thought 融入回来招呼 + Consolidation 调度。`trigger_reflection_if_due` IPC 签名不变（规避踩坑#4）。

### 2026-07-27 早些 welcome-back 回来主动招呼
- `generate_welcome_back(away_secs)` 不改 generate 签名。

### 2026-07-26 docs 治理 / proactive_harness 简化 / 提醒功能修复
- 闭环2 真实运行 ✅（"3分钟后提醒喝水"全链路通过）。

## §部署：桌面启动方式（2026-07-28）
release exe 构建一次，桌面快捷方式双击启动（无需终端 / `npm run tauri dev`）。
- 构建：`npx tauri build --no-bundle`（**勿用** `cargo build --release`——后者产物 embed 不全、webview 加载异常）。`--no-bundle` 跳过 msi 打包（wix 可能失败，非必需），只出 exe。
- 产物：`D:\cargo-target\desktop-pet\release\desktop-pet.exe`（CARGO_TARGET_DIR 重定向到 D 盘，**非** `src-tauri/target/`；bin 名 `desktop-pet`，非 productName `DesktopPet`）。
- 桌面快捷方式：`C:\Users\SunJialei\Desktop\DesktopPet.lnk`（Target=exe, Icon=src-tauri/icons/icon.ico）。
- 踩坑1（已修）：`open_devtools` 是 Tauri **debug-only** API（release 下该方法不存在 → E0599）。commands.rs 已加 `cfg(debug_assertions)` 守卫，release no-op。
- 踩坑2（已修，release-only 隐蔽）：PIXI ShaderSystem 需 CSP `unsafe-eval`，但 tauri.conf.json CSP 原本只有 `wasm-unsafe-eval`（给 Live2D Core）→ PIXI Application 创建即崩 → 桌宠空白不显示。dev 模式 tauri 自动放宽 CSP（dev 正常），release 用配置 CSP 才暴露。诊断法：WebView2 设 `--remote-debugging-port=9222` + CDP `Runtime.evaluate` 抓异常。已加 `'unsafe-eval'` 到 `script-src`。
- 重建：改 Rust/tauri.conf.json → `npx tauri build --no-bundle`；改前端 → 先 `npm run build`。快捷方式自动指向新 exe（同路径覆盖）。

## §审计：P0-P17 + 架构原则完成度（2026-07-28，对照 implementation-plan v1.1 + design v2 + Architecture-Principles）

Kill List 三闭环全部端到端跑通（Body→Memory→Soul）。逐项审计（✅ 完整 / ⚠️ 有缺口 / ❌ 未做）：

| 阶段 | 状态 | 说明 / 缺口 |
|---|---|---|
| P0 脚手架/配置 | ✅ | AppData config（踩坑#1）|
| P1 数据库 | ✅ | schema v2，8 层记忆全 |
| P2 Embedding | ✅ | BGE-M3，AppData 引导下载 |
| P3 LLM 客户端 | ⚠️ | 非流式；**流式 chat_stream 未做**（client `stream:false`）|
| P4 Emotion | ✅ | state/homeostasis/needs/pace 全 |
| P5 摄入管道 | ✅ | gate/extractor/store/correction/working |
| P6 检索管道 | ✅ | trigger/retrieval/budget/grounding，score breakdown |
| P7 Planner | ⚠️ | director+actor 闭环；**流式逐字渲染未做** |
| P8 Pending | ✅ | 闭环2 实跑 |
| P9 Body 窗口 | ✅ | Live2D/透明/点击穿透 |
| P10 FSM | ⚠️→✅ circadian | fsm+emotionDriver(连续表情)+microBehavior+circadian sleepiness 接入 ✅；idle_weights 硬编码(非 JSON，可调) |
| P11 交互 | ⚠️ | 摸头/戳/注意力三态 ✅；气泡生命力(节奏+glyph) ✅；Foley 音效 5 音 ✅；**走路脚步声 loop、Alt+Space 全局键未做** |
| P12 物理 | ⚠️ | 空间(窝/回巢)/昼夜 ✅；**自由落体/任务栏弹跳简化(松手停原地)** |
| P13 Soul | ⚠️ | reflection/monologue/consolidation+慢循环闭环 ✅；**TurnThreshold/MajorEvent 触发器、Consolidation 反向更新 Facts 未做** |
| P14 感知 | ✅ | time/presence/window 模块全 |
| P15 Life Loop | ✅ | 三循环+recovery(前端catch)+firstrun 访谈 |
| P16 Debug Panel | ⚠️ | Brain/Counts/Facts/Episodes/Pending/Timeline ✅；**Prompt token/Retrieved score/Reflect/AnimFSM/Cost 分区缺** |
| P17 Golden | ⚠️ | golden_conversations 测试数据有；**evaluation 框架+人格漂移 score+CI 未完整** |
| A1 BrainState 快照 | ❌ | converse 多参数，未统一 BrainState（架构债）|
| A2 统一 Scheduler | ❌ | loop_runner 线程+sleep，非 Scheduler trait（架构债）|
| A3-A6 | ✅ | 直接调用+事件 / Change Log / Suspend-Resume / schema_version |

## §未解决问题
- **P16 Debug Panel 部分缺**：Prompt token 预算 / Retrieved score breakdown / Reflect 分区未实现（核心状态面板已在）。现在 `BubbleOutcome.anchor` 已暴露，Debug Panel 可顺手显示"当前冒泡锚定的记忆"。
- **物理简化**：拖拽松手停原地 + 30s 回巢；完整桌面物理（碰撞、空间 Episode）未做，MVP 够用。

## §下一步候选（按优先级重排，基于 §审计 + 北极星 #10 + Kill List 已完成）

> ⚠️ **本节为 07-28 快照，已过时**（Tier1 三项全完成、Tier2 #4/#5 已做）。最新统一优先级 backlog 见文末 [§下一步总清单](#下一步总清单2026-07-31-统一优先级--取代上方-下一步候选)。保留下方作历史对照。

Kill List 三闭环已完成，现按"提升体验/生命感"→"闭环深度"→"Body 完善"→"开发者基建"→"架构债"→"二期"排序。

**Tier 1 — 生命感/体验（#10 北极星，对话是核心交互）**
1. ✅ **流式回复**（已完成并实跑确认）：ipc::Channel 逐字渲染（短回复看不出逐字是 DeepSeek-v4 reasoning content 占比小，非 bug）。详见 §最近一轮。
2. ✅ **气泡生命力**（P11.3，已完成 `abb9d49`，待实跑）：打字节奏随情绪（`bubblePacing` 6 档）+ 无文字气泡（glyph 省略号/叹气，#12）。形态动画本就有 5 种 keyframes。「害羞慢现」缺后端 mood 标签未做（follow-up）。
3. ✅ **Foley 音效**（P11.5，已完成实跑通过）：真实素材 10 接入（ow/啊/啊1/生气/笑/布料/落地/跳/UI/hi）+ 权重静默优先 + cooldown + 亲密度分档 + 启动招呼(autoplay 补播)；sleep 预留（Sleeping 未做）。详见 §最近一轮。

**Tier 2 — Soul/对话深度（闭环增强）**
4. ✅ **converse 注入 surfaced thought**（已完成，build 过 / 待实跑）：正常对话也带出昨晚念头。converse Step 8 后注入克制措辞的 thought_clause（#8 零额外 LLM、消费性与 welcome-back 自洽）。详见 §最近一轮。
5. **Reflection TurnThreshold/MajorEvent 触发器**：每 30 轮 / importance>0.85 自动反思（现只 Daily）。
6. **Consolidation 反向更新 Facts**（#9 V2）：压缩总结中的事实回写 Facts。

**Tier 3 — Body 完善**
7. ✅ **circadian 接入微行为**（已完成，build 过 / 待实跑）：sleepiness 调制 idle 权重（深夜 yawn↑/look_around↓）。详见 §最近一轮。follow-up：speedModifier 未接动画速度；Sleeping 自动入睡/唤醒机制（现只调权重，未真正入睡）。
8. **完整物理**（P12.1）：自由落体 + 任务栏弹跳（现简化松手停原地）。

**Tier 4 — 开发者基建（#11 Explainability）**
9. **P16 Debug Panel 补全**：Prompt token / Retrieved score breakdown / Reflect(has_thought/unsurfaced) / Cost 分区。
10. **P17 Golden 评估框架**：人格漂移 score + CI 自动跑（现 golden 数据有，框架不完整）。

**Tier 5 — 架构债务（重构，功能已在跑）**
11. **A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表。
12. **A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14 二期清单）**
13. Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

---

## §下一步总清单（2026-07-31，统一优先级 · 取代上方 §下一步候选）

> **权威 backlog。** 上方 §下一步候选 是 07-28 快照（Tier1 已全完成、Tier2 #4/#5 已做），仅作历史对照。
> Kill List 三闭环全部端到端跑通（活着 Body → 记住你 Memory → 懂你 Soul）。
> 排序驱动：北极星 #10（优先生命感不优先功能）+ 优先级阶梯（活着→记住→懂你→工具砍）+ 实施计划 P0-P17 / A1-A2。
> 两类工作：**① 待验收**（已编码、收尾即闭环，最高 ROI）→ **② 待开发**（按 Tier 优先级）。

### ① 待验收（代码层已全部验收 ✅ 2026-07-31 18:01；GUI 实跑待用户）

> **代码层闭环**：`cargo test --lib` **207 passed** / `cargo check --tests` 全 harness 编译 ✅ / `tsc --noEmit` ✅ / `npm run build` ✅（2.12s）。**全部已 rebuild 进 release exe**（`D:\cargo-target\desktop-pet\release\desktop-pet.exe` 07-31 18:01，含工作树未提交的 A1/A2/A4 Rust 改动；桌面快捷方式自动指向）。A1-A6 代码层验收通过，余下仅 GUI 运行时实跑（见"运行时实跑"列）。

| # | 项 | 代码层验收 | 运行时实跑（用户） |
|---|---|---|---|
| A1 | consolidation max_tokens 修复 | ✅ `consolidation.rs:89` `Some(4096)` + `:97-103` 空 content 防御 | 需攒 ≥100 低 importance episodes 自然触发，难快速复现（不必强测） |
| A2 | Reflection TurnThreshold/MajorEvent 触发器 | ✅ 优先级 Daily→MajorEvent→TurnThreshold + 12 单测全过 | 需攒 30 条对话记忆 或 importance>0.85 事件 |
| A3 | converse 注入 surfaced thought | ✅ `converse.rs:202-221` 注入 + 消费性 | 需 reflection 先产 thought（一日以上），下次对话观察带出 |
| A4 | Sleeping 入睡/唤醒 | ✅ `App.tsx:216-222` 入睡 + `:604-607` 唤醒 | **可立即验证**：改系统时间 2-6 点 + 不交互 10min→入睡；戳/摸/对话→唤醒 |
| A5 | circadian sleepiness 调权重 | ✅ `microBehavior.ts` sleepy 公式 + `App.tsx:226` 喂入 fsm.tick | **可立即验证**：深夜 yawn↑ / look_around↓（对比白天） |
| A6 | emotionBridge 连续表情 | ✅ `App.tsx:56` toEmotionVector + `:934` 传 Live2DCanvas | **可立即验证**：戳→嘴角下垂；开心→微笑笑眼；久运行→半眯 |
| A7 | ~~多气泡堆叠~~ | ❌ **未实现** | 降级为 ③ follow-up（见下） |

> **A7 勘误**：原 backlog 把"多气泡堆叠"列为待验收，核验发现 `App.tsx:75-77` 气泡是单气泡状态（`bubbleText/Visible/Style/Pos` 均单一 useState）、`showBubble`(:159) 是覆盖语义（新气泡直接覆盖旧的 + 重置 timer），从未实现堆叠。降级为 follow-up；若用户确认需要"堆叠/排队"再开。

### ② 待开发（按优先级）

**Tier 2 — Soul/对话深度（懂你 · 闭环增强）**
- ~~**B1. Consolidation 反向更新 Facts**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：`consolidate` 成功后调 `backfill_facts`（LLM 从摘要提取 JSON 事实 → category 白名单+confidence clamp → `expire_old` 冲突过期 + `dedup_insert`）。失败隔离（只 warn）。+8 单测 + 新 `consolidation_harness`（真实 LLM 端到端）。详见 §最近一轮 (2026-08-03)。
- **B1b. Grounding 运行时阻断（B 档 · ⏳ 条件触发）**：A 档（prompt 收紧）实跑若仍偶发主动开口幻觉则升级——`check_groundedness` 补中文 claim 模式（现全英文、中文漏检）+ 在 proactive/welcome_back 输出端挂检测、发现编造就丢弃/降级。根因+修复详见 §最近一轮 (07-31 19:10)。

**Tier 3 — Body 完善（活着 · 生命感）**
- ~~**B2. 完整物理**~~ ✅ **已完成（2026-08-03，合并自 opencode 副本）**：自由落体 + 任务栏弹跳（P12.1）。新 `gravity.ts`（GRAVITY/BOUNCE 常量 + `stepGravity` 纯函数）。**关键**：发现 `startDragging` 吞 webview 鼠标事件（旧 `onUp` 死代码）→ 改 `onMoved`+静止检测；petPos useState→ref 重构修卡顿。用户偏好"1/3 飘落悬停"（不真触底，bounce 当前是死代码，待确认）。详见 §最近一轮 (2026-08-03)。
- ~~**B3. Sleeping 配套收尾**~~ ✅ **已完成（2026-08-03 续，纯前端）**：① 睡着抑制 DeepNight/LateNight nudge（`App.tsx` nudge effect 加 `fsmRef.state===Sleeping` 守卫，不再梦话）② 接 sleep 音效（`soundManager.ts` 加 `"sleep"` AssetKey + `sleep()` 方法 mirroring `greet()`；入睡时 `sound.sleep()`，mute 尊重 #6）③ LateNight 不入睡只 yawn（**已满足、零改动**：auto-sleep 本就 DeepNight-only）。详见 §最近一轮 (2026-08-03 续)。**待实跑**。

**Tier 4 — 开发者基建（#11 Explainability · ⭐ 当前最高 ROI 且未受阻）**
- ~~**B4b. conversations 死表修复**~~ ✅ **本轮完成（2026-08-03 续③）**：审计确认真 Bug——生产路径从未调 `conversations::insert`（grep 0 / callers 仅测试）。`commands.rs::send_message` 镜像 working_memory push 写 user+assistant turn。详见 §最近一轮 (2026-08-03 续③)。
- ~~**B4-MVP. Debug Panel 决策链分区（Retrieved+Intent+Reflect）**~~ ✅ **本轮完成（2026-08-03 续③）**：服务"她为什么这么说"诊断链。详见 §最近一轮 (2026-08-03 续③)。
- **B4-余. Debug Panel 补全（follow-up）**：~~Cost~~ ✅ 续③；~~AnimFSM（当前态+history）~~ ✅ **续⑧**（fsm.getHistory + DebugPanel AnimFSM 分区）；~~Prompt（动态 token）~~ ✅ **续⑧**（PromptTokenDebug → DecisionTrace → Last Turn "sys N/budget M"）。**Debug Panel 9 分区全补齐**（Brain/Counts/Cost/Facts/Episodes/Pending/Timeline/Last Turn/Retrieved/Reflect/AnimFSM）。待 dev 实跑确认 AnimFSM/Prompt 渲染。
- **B5. P17 Golden 评估框架**：✅ **三层完成**——① 规则启发式层（2026-03 续⑧，`personality_drift_score` 抓 GROSS 话痨/卖萌/依赖）+ ② 语义 cosine 层（2026-08-08 Item6，`semantic_drift_score` 抓语气漂移）+ ③ **LLM-as-judge 层（2026-08-08 续，`tests/personality_judge_harness.rs`：30 条 golden 集 + persona_fit 0-10 + 漂移维度命名 + 3 次退避重试）**。规则/cosine 是廉价 CI 线（合成向量 + 规则单测），judge 是重手动线（同 prompt_quality/embedding_ab 模式）。三层交叉验证各覆盖边界：规则层对 Subtle(cold/客服腔/鸡汤/动作描写) **0/10 盲**、judge 是唯一抓这些的线。详见 §最近一轮 (2026-08-08 续)。

**Tier 5 — 架构债务（重构 · 功能已在跑）**
- **B6. A1 BrainState 统一快照**：converse 等改 `fn(brain: &BrainState)`，消除多参数列表（架构债）。
- **B7. A2 统一 Scheduler**：loop_runner 线程+sleep → Scheduler trait（ticks_1s/30s/daily）。

**Tier 6 — 二期愿景（design §14）**
- **B8.** Shared World（桌面元素认知）/ Rituals / Landmarks / Adaptive Traits V2 / 混合检索 V2。

### ③ 散落 follow-up（低优先 · 可并入相关 Tier）
Alt+Space 全局键（P11.4）/ 走路脚步声 loop（P11.5）/ 害羞慢现气泡形态（缺后端 mood 标签）/ ~~rest_need 后端暴露（P10）~~ ✅ **2026-08-04**（含激活生产 homeostasis + 恢复项；详见 §最近一轮 2026-08-04）/ ~~speedModifier 接动画速度（circadian）~~ ✅ **2026-08-04**（PIXI ticker.speed；energyModifier 仍未消费——能量已是情绪维度，speed 够用）/ idle_weights JSON 化（数据驱动）/ ~~选择性遗忘（用户请求"忘掉..."，P13 lifecycle_cleanup）~~ ✅ **2026-08-04 续 episode MVP + 2026-08-05 fact/pending 扩展**（gate Forget + `forget_best_match` 三路调度 episode/fact/pending + converse 确认；详见 §最近一轮 2026-08-05。**仍留 follow-up**：多轮消歧义、fact/pending 语义级匹配需加向量）/ **loneliness 生产未激活**（apply_homeostasis_time_aware 不更新；tick_needs 死代码；影响检索/planner，激活属行为变更需评估）/ ~~**FTS5 全历史检索**~~ ❌ **2026-08-05 证伪**（bundled SQLite 三分词器对中文 MATCH 全 0——无 CJK 分词；"fts5_cjk"旧记错误；除非引入 jieba 扩展/Rust 分词否则不可行，已从 backlog 移除）。

### ③ Hermes 记忆优化 follow-up（续③ 立项，按 ROI）
~~FTS5 全历史检索~~ ❌ **2026-08-05 证伪移除**（见上，CJK 不兼容）/ ~~"关系进展摘要"（后台每 N 次对话异步总结，对应 Hermes 后台 review）~~ ✅ **2026-08-07**（relationship_reviews 表 + soul/review.rs + [Relationship] 注入 + slow_tick 调度；详见 §最近一轮）/ 记忆可视化编辑（Debug Panel 只读→可改）。

> **建议下一会话起点**：先清 ① 待验收（A1-A7 逐项 rebuild+实跑，零新代码、闭环既有成果），再按 B1→B8 推进。实跑前提：`%APPDATA%\DesktopPet\config.toml` 配好 DeepSeek key + 桌面快捷方式（或 `npm run tauri dev`）。
