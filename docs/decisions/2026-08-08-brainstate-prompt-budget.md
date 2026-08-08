# 决策记录:不把 BrainState 扩进 prompt builder / budget（ADR）

> 日期: 2026-08-08
> 状态: **Accepted**
> 决策者: Claude（自主推进批次，3d 架构债收尾）
> 触发: 实施计划 §A1「全局 BrainState」的留尾——Item 5（2026-08-08）把 `BrainState` 采纳边界定为 **planner**（旗舰纯决策），并在 `brain_state.rs` 顶部注明「system-prompt builder 和 budget allocator 取重叠子集，是干净的 follow-up」。本 ADR 复核该 follow-up 是否真的该做。

## 背景

`BrainState<'a>`（`mind/brain_state.rs`）持有**规划输入**快照：

| 字段 | 来源 | planner 用 | prompt builder 用 | budget 用 |
|---|---|:---:|:---:|:---:|
| `text` | 本轮用户消息 | ✅ | ✗ | ✗ |
| `emotion` | 当前情绪向量 | ✅ | ✅ | ✅ |
| `relationship` | 当前关系 | ✅ | ✗（用 retrieval 内的同名字段） | ✗ |
| `pending_due` | 到期 pending | ✅ | ✗ | ✗ |
| `retrieval` | 检索结果 | ✅ | ✅ | ✅ |

复核 prompt builder / budget 的实际签名与字段消费（`grounding.rs` / `budget.rs`）：

- `build_system_prompt(retrieval, emotion, intent)` —— 用 `retrieval.persona_traits` / `retrieval.relationship` / `retrieval.user_profile` + `emotion` + `intent`。
- `build_qa_system_prompt(retrieval, emotion, intent)` —— 同上（QA 版）。
- `allocate_and_compress(retrieval, working_memory, emotion, intent)` —— 上述 + `working_memory`。
- `allocate_qa(...)` —— 同上（QA 版）。
- `compress_system_prompt(prompt, retrieval, emotion, intent)` —— 私有，超预算时用截断后的 retrieval 重建。

**关键事实**：这五个函数都吃 `(retrieval, emotion, intent)` 三元组，而：

1. **`intent` 是 planner 的 *输出***（`planner::plan(&brain) → Intent`）。它是规划结果，不是规划输入，**不能放进 BrainState**——否则形成循环依赖（brain 喂给 planner，planner 产出 intent，output 不能塞回 input struct）。
2. BrainState 的 `text` / `relationship` / `pending_due` 这三个字段，prompt builder 和 budget **一个都不用**。

## 考虑过的方案

### 方案 A：把 `&BrainState` 扩进这五个函数（原 follow-up 字面意图）
`build_system_prompt(brain: &BrainState, intent: &Intent)` 等。
- 收益：与 planner 的传参风格一致（都吃 `&BrainState`）。
- 代价：
  1. **省不掉 `intent` 参数**——它不在 BrainState 里（见上），签名仍需 `(brain, intent)` 两参，没比现在的 `(retrieval, emotion, intent)` 三参更短。
  2. **捆绑 3 个无用字段**（text / relationship / pending_due）——这正是 `brain_state.rs` 顶部和 §A2 ADR 已明确否决的「投机 mega-state」：强迫一个结构跨三个消费者，却塞进各自不需要的字段。
  3. **零行为/正确性价值**：纯化妆式重写，每个函数仍只用 `(retrieval, emotion, intent)` 子集，只是多一层 `brain.retrieval` / `brain.emotion` 解引用。
  4. 命中**踩坑#4 风险**：改 5 个函数签名 → 同步 `budget.rs` 内部互调（build_system_prompt ↔ compress_system_prompt，后者还用截断 retrieval 重建，bundle 反而碍事）+ converse 调用点 + golden/questioning/evaluation 等 harness 调用点。高风险、零收益。

### 方案 B：新建窄类型 `PromptCtx { retrieval, emotion, intent }`（"对的尺寸" bundle）
把重复出现的 `(retrieval, emotion, intent)` 三元组收进一个**仅含这三字段**的新类型。
- 收益：消掉 budget 模块内 build_system_prompt / build_qa_system_prompt / compress_system_prompt 三处重复的 3 参传递。
- 代价：① 新增一个类型定义；② `compress_system_prompt` 内部用**截断后的 retrieval** 重建，bundle 在那里反而要先拆再组；③ 三个字段都各被每个函数全用，本就是「恰好够用」的紧签名——为消除重复而引入间接层，是 Karpathy 简单性原则里典型的「为单一用途加抽象」。
- 判定：**边际**。比方案 A 干净得多（不捆绑无用字段、不碰 intent 循环），但收益（少写几个参数）不抵成本（新类型 + 截断路径碍事 + 读代码多跳一层）。当前 3 参签名已经自解释。

### 方案 C：保持现状，把 follow-up 关掉（本决策采用）
`BrainState` 采纳边界停在 planner。prompt builder / budget 继续吃紧签名 `(retrieval, emotion, intent)`（+ working_memory），每个参数都被每个函数实际使用——这正是「刚够用」。

## 决策

**不把 BrainState 扩进 prompt builder / budget。** follow-up 关闭，§A1 的采纳边界定为终态 = planner。理由链：

1. **架构不相容**（原则 #1 / #2）：intent 是规划输出非输入，不能入 BrainState；强行扩会留一个 `(brain, intent)` 的半 bundle，比现状更别扭。
2. **投机抽象**（原则 #9 just-enough / Karpathy 简单性 / §A2 ADR 先例）：把 text/relationship/pending_due 捆给不消费它们的函数，是已否决的 mega-state 模式。
3. **风险/收益倒挂**：5 函数签名 + 多 harness 调用点的踩坑#4 级改动，换零用户价值、零正确性改善。
4. **现状已自洽**：`(retrieval, emotion, intent)` 是紧签名，每参必用；`brain_state.rs` 注释已诚实标注此边界，无需再动。

## 后果 / 何时复议

- `BrainState` 终态边界 = planner（旗舰纯决策）。prompt builder / budget 保持独立紧签名。
- 若未来 intent 的**下游消费者**（不止 prompt builder/budget，还有 grounding guard 等）增多到 `(retrieval, emotion, intent)` 三元组在 ≥5 处重复且各自还加字段，届时再考虑方案 B 的窄类型 `PromptCtx`——但触发条件是真实重复痛点，不是现在的 3 处。
- 若 planner 的输入维度显著膨胀（BrainState 字段翻倍），届时 prompt builder / budget 也许会自然开始消费更多 brain 字段，bundle 才有正当性——目前不会。
- 本决策不阻塞任何功能；converse 一次构造 `brain` 喂 planner、再用紧签名喂 budget 的现状继续工作。
