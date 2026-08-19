<div align="center">

<img src="src-tauri/icons/icon.png" width="170" alt="Liri · 璃" />

# Liri · 璃

### 一个有记忆、有情绪、会主动找你的 Windows 桌面陪伴宠物

她不是一个"带记忆的聊天机器人"，而是一个"有生活的生命"。

[特性](#-特性) · [快速开始](#-快速开始) · [技术栈](#-技术栈) · [架构](#-架构-mind--body--soul) · [文档](#-文档) · [English](#english-abstract)

</div>

---

> 🌟 **成功标准**：你对她说"我最近准备找实习"，一周后她会主动问你"实习找得怎么样啦？"
> —— 不靠定时器提醒，靠**记忆 + 关系 + 主动性**。

**Liri（璃）** 是一只常驻你桌面的小狐灵。她会记住你们聊过的每一件事，在情绪低落时主动来找你，在你忙时安静等待，在深夜催你早点睡。她记得自己答应过你的事，会带着"当时的氛围"想起你，也能在你需要时查资料、帮你打开应用。所有数据都在本地，所有模型由你自己配置——她只属于你。

> ⚠️ **当前状态：早期开发中（v0.1），仅支持 Windows。** 角色璃已由 **Spine + PixiJS** 骨骼动画驱动（呼吸、视线 360° 跟随、情绪表情渐变、微行为），美术资产仍在持续迭代。

---

## ✨ 特性

### 🧠 记住你（Memory）
- **情景记忆**：每一段对话沉淀为 episode（带重要度、情绪、时间衰减）
- **语义检索**：本地 BGE-M3 向量化 + sqlite-vec 向量库，按「语义 / 强度 / 新颖度 / 时近 / 情绪」打分召回
- **事实抽取与巩固**：从对话中提炼长期事实（偏好、计划、人际关系），冲突时自动过期，consolidation 反向回填；写入前过确定性**卫生闸门**（拒知识问答/越界内容入库）
- **可遗忘**：自然语言"忘掉我说过的 X"即可软删除，多候选时她会先反问确认，绝不误删
- **承诺追踪**：她亲口答应你的事（"明早叫你起床"）自动建档必追，到期主动兑现——"我说过要叫你起床的"，遗忘自己说过的话是最伤信任的事
- **有温度的浮现**：主动想起的记忆带 **recall_reason**（"从没主动提起过的旧事" / "你们常聊的话题"）和**情感锚点**（"在猫咖，眼睛亮亮的"），开口像真的惦记着，不是翻档案
- **浮现治理**：全局冒泡预算 + 记忆轮转（7 天硬排除 + 最少浮现优先），不会每分钟烦你，也不会翻来覆去只有那一件事

### 💗 懂你（Soul）
- **关系成长**：closeness / trust 随真实相处累积——早期不黏人、熟络后更亲
- **情绪稳态**：mood / energy / social_battery / stress / loneliness 持续漂移；loneliness 高 + 关系够熟时，她会**主动**冒一句"想你"
- **自我反思**：离线 reflection 生成内心 thought，在恰当时机自然带进对话
- **关系复盘**：阶段性 relationship review，她会"回想"这段关系的进展
- **仪式感**：早安仪式、久别欢迎、孤独轻戳、昼夜节律的"该睡了"

### 🐾 活着（Body）
- **物理交互**：拖拽自由落体、摸头（降 loneliness）、戳（逗弄）、双击
- **作息系统**：昼夜节律（circadian）驱动深夜打哈欠、自动入睡、轻声唤醒
- **微行为**：发呆、四处张望、歪头、伸懒腰、摇摆、偷瞄、害羞……多种自发动作
- **情绪表情**：Spine 骨骼动画驱动，表情随情绪**连续渐变**（非离散跳变），视线 360° 跟随鼠标
- **声音**：真实 Foley 音效（摸头 / 拖拽 / 入睡），全局单音互斥、静默优先

### 🛠️ 会搭手（Agent · 工具层）
- **联网搜索**："查查最近的 AI 新闻" → 头条搜索源，中文总结给你
- **打开应用**："打开网易云" → 动态扫描桌面/开始菜单快捷方式，自己判断，零白名单配置
- **知道时间**："现在几点" 直接答，不浪费一次工具调用
- **三层门控 + 安全铁律**：Planner 决定要不要给她工具 → LLM 决定怎么用 → Tool Policy 硬校验（白名单 / https / 超时 / 限流）；工具结果视为不可信输入、绝不进记忆；闲聊语境 **0 工具调用**（黑名单测试优先）

### 🔒 隐私与成本
- **全本地**：记忆存 SQLite，向量存 sqlite-vec，嵌入跑本地 BGE-M3，不传任何第三方
- **模型自配**：默认 DeepSeek，也支持 OpenAI、或 **Ollama 完全本地**运行（一个 API key 都不用）
- **成本可控**：budget 管控 + flash/pro 双模型分流（反思用便宜的）+ 流式回复逐字呈现
- **能力可关**：每个感知层（时间 / 在场 / 窗口）与调度能力（反思 / 固化 / 工具）可独立关闭（Architecture Principle #6）

---

## ⬇️ 下载安装（普通用户，无需任何开发环境）

> ⚠️ 当前状态：早期开发中（v0.1）。点击下方按钮，浏览器会直接跳转到 GitHub 发布页（下载页面）。

<div align="center">

[![下载 Windows 版](https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD%20Windows%20%E7%89%88-v0.1.0-9d7ee0?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/sunjialei1237-beep/desktop-pet/releases/latest)

也可以点击下面的直链**直接开始下载**：

[⬇️ Liri_0.1.0_x64-setup.exe — 一键安装（7.7 MB）](https://github.com/sunjialei1237-beep/desktop-pet/releases/download/v0.1.0/Liri_0.1.0_x64-setup.exe) ｜
[⬇️ Liri-0.1.0-x64-portable.zip — 免安装（9.7 MB）](https://github.com/sunjialei1237-beep/desktop-pet/releases/download/v0.1.0/Liri-0.1.0-x64-portable.zip)

</div>

**三步开始使用：**

1. **下载并安装**：双击 `setup.exe`，一路「下一步」即可（无需管理员权限）。
2. **配置 API Key**：首次启动会自动弹出配置向导 —— 粘贴你的 DeepSeek API Key（[免费获取](https://platform.deepseek.com/)），点「验证并保存」即可开口聊天。也支持任意 OpenAI 兼容服务（OpenAI / Ollama 等，改 Base URL 即可）。
3. **（可选）下载记忆模型**：在向导中选择「立即下载」（约 570MB，本地 BGE-M3，一次下载永久使用）。下载后她才能真正跨会话"记住你"；跳过也能正常聊天，只是记性差一些。之后随时可在 **右键 → 模型与设置** 里补下载。

> 💡 所有数据（对话记忆、向量、模型文件）都保存在你自己的电脑上（`%APPDATA%\DesktopPet\`），不上传任何第三方。

---

## 🚀 从源码运行（开发者）

### 前置要求
- **Windows 10/11**（一期；macOS / Linux 待后续）
- [Node.js](https://nodejs.org/) ≥ 20
- [Rust](https://www.rust-lang.org/) 工具链（`cargo`）
- 一个 OpenAI 兼容的 LLM —— 推荐 [DeepSeek](https://platform.deepseek.com/)，或本地 [Ollama](https://ollama.com/)

### 安装与运行

```bash
git clone https://github.com/sunjialei1237-beep/desktop-pet.git
cd desktop-pet
npm install
npm run tauri dev
```

首次启动会自动在 `%APPDATA%\DesktopPet\config.toml` 生成配置模板。**编辑它填入你的 API key**，然后重启：

```toml
[llm]
base_url = "https://api.deepseek.com/v1"   # 或 OpenAI / http://localhost:11434/v1 (Ollama)
api_key = "sk-..."                          # ← 填这里（用 Ollama 则留空）
main_model = "deepseek-v4-pro"
reflection_model = "deepseek-v4-flash"

[embedding]
# 留空 = 首次运行引导下载 BGE-M3（~2.2GB）到 AppData
model_dir = ""
model_name = "bge-m3"
```

> 💡 嵌入模型首次运行会下载 BGE-M3（约 2.2GB），之后离线可用。完整配置项见 [`src-tauri/resources/config.example.toml`](src-tauri/resources/config.example.toml)。

> ⚠️ **必须用 `npm run tauri dev` 运行**。直接用浏览器开 `localhost:1420` 会让所有 Tauri 命令（对话、记忆）失效（没有后端）。

### 构建发布版

```bash
npx tauri build --no-bundle    # 产物：desktop-pet.exe（前端已嵌入）
```

### 调试

- **F12** — 应用内 Debug Panel：BrainState 实时快照、记忆 / 情绪编辑器、成本统计
- **右键桌宠 → DevTools**（仅 dev）— `window.__pet` 钩子可模拟时段 / 强制入睡，详见 [`docs/verify-checklist.md`](docs/verify-checklist.md)

---

## 🧱 技术栈

| 层 | 选型 | 说明 |
|---|---|---|
| 应用框架 | **Tauri v2** | Rust 后端，常驻内存 ~30–50MB（Electron 通常 150–300MB） |
| 前端 | **React 19 + TypeScript** | 对话 UI、渲染编排 |
| 渲染 | **Spine + PixiJS** | 角色"璃"骨骼动画：呼吸、视线跟随、情绪表情、微行为 |
| 存储 | **SQLite + sqlite-vec** | 情景记忆 + 原生向量检索 |
| 嵌入 | **BGE-M3**（本地 ONNX Runtime） | 中文语义检索，离线 |
| LLM | **DeepSeek v4**（默认）/ OpenAI / Ollama | OpenAI 兼容，用户自配；工具调用 + 流式 |
| 构建 / 测试 | Vite 6 · Vitest · cargo test | 388 库单测 + 真 LLM 闭环 harness |

---

## 🏗️ 架构 (Mind / Body / Soul)

Liri 的内核遵循 **Mind / Body / Soul** 三层架构，并有一组不可违背的[架构原则](docs/Architecture-Principles.md)：

- **Mind（思维）** —— Rust 维护所有状态（记忆、情绪、计划）；LLM 只负责"表达"，绝不维护状态。
- **Body（身体）** —— React 前端，纯渲染 + 物理交互；不持有业务真相。
- **Soul（灵魂）** —— 关系、性格、反思；让记忆升华为"懂你"。

> 北极星原则摘要：① LLM 只表达不维护状态 ② BrainState 统一快照 ⑤ Mind-Body 解耦 ⑥ 每个能力可关闭 ⑧ 成本是设计约束 ⑫ 沉默也是表达。完整 12 条见 [`docs/Architecture-Principles.md`](docs/Architecture-Principles.md)。

---

## 📁 项目结构

```
desktop-pet/
├── src/                    # 前端 (React + TS)
│   ├── animation/          # FSM、昼夜节律、微行为、物理
│   ├── SpineCanvas.tsx     # 角色"璃"渲染（Spine + PixiJS）
│   └── App.tsx             # 主交互编排
├── src-tauri/              # 后端 (Rust)
│   ├── src/mind/           # 记忆抽取 / 检索 / 对话 / 遗忘 / 工具 Agent
│   ├── src/emotion/        # 情绪状态机 + 稳态
│   ├── src/soul/           # 关系 / 反思 / 复盘 / 仪式
│   ├── src/pending/        # 提醒 / 承诺追踪 / 主动冒泡治理
│   ├── src/perception/     # 时间 / 在场 / 焦点 / 光标感知
│   ├── src/db/             # SQLite + sqlite-vec
│   └── resources/prompts/  # 角色提示词
└── docs/                   # 设计文档、架构原则、ADR、交接日志
```

---

## 🧪 测试

```bash
npm test                                            # 前端单测 (Vitest)
cargo test --manifest-path src-tauri/Cargo.toml --lib   # 后端库单测（快，无 LLM）
```

端到端 harness（调真实 LLM，需配好 key）见 [`src-tauri/tests/`](src-tauri/tests/)。

---

## 🗺️ 路线图

- [x] Mind 记忆闭环（情景记忆 + 向量检索 + 巩固 + 遗忘 + 承诺追踪）
- [x] Soul 关系成长 + loneliness 主动陪伴 + 反思 + 仪式
- [x] Body 物理交互 + 昼夜作息 + 情绪表情（Spine 角色已上线）
- [x] Agent 工具层（搜索 / 打开应用 / 时间，三层门控）
- [ ] 角色美术资产持续迭代（表情 / 动作时间线补全）
- [ ] macOS / Linux 支持
- [ ] 用户自定义形象

完整计划见 [`docs/plans/`](docs/plans/)。

---

## 📄 文档

- [架构原则（北极星）](docs/Architecture-Principles.md)
- [产品设计 v2](docs/specs/2026-07-14-desktop-pet-design.md)
- [璃 · 角色圣经](docs/specs/liri/)（设计规范 / 动画设计 / 制作规范）
- [开发交接日志](docs/HANDOFF.md)
- [ADR 决策记录](docs/decisions/)

---

## 🤝 贡献

欢迎 Issue / PR！请先读[贡献指南](CONTRIBUTING.md)与[架构原则](docs/Architecture-Principles.md)——所有技术决策须与之相符。

---

## 📜 许可证

[MIT License](LICENSE) · Copyright © 2026 SunJialei

---

## English abstract

**Liri (璃)** is a Windows desktop companion pet with long-term memory, emotion, and genuine proactivity. Stack: **Tauri v2 (Rust) + React + Spine/PixiJS + SQLite/sqlite-vec + a local BGE-M3 embedder + any OpenAI-compatible LLM** (DeepSeek by default; Ollama for a fully-offline setup).

She is not "a chatbot with memory" but "a creature with a life": she remembers what you tell her, keeps her own promises ("我说过要叫你起床的…"), recalls moments with their original atmosphere, grows closer as the relationship deepens, proactively reaches out when she misses you, and can search the web or launch apps when you ask. All data stays local; all models are self-configured — she belongs only to you.

> Status: **early v0.1, Windows-only.** The character *Liri* is already animated with Spine skeletal animation (breathing, gaze tracking, emotion-driven expressions); art assets are still being iterated on.
