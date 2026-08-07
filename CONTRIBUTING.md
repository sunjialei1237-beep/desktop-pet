# 贡献指南

欢迎参与 Liri（璃）的开发！在提交 Issue 或 PR 前，请先阅读本指南。

## 1. 先读架构原则

[`docs/Architecture-Principles.md`](docs/Architecture-Principles.md) 是本项目的 12 条**北极星原则**，所有技术决策必须与之相符。这是最重要的约束，没有之一。摘要：

- ① LLM 只表达，不维护状态（Rust 维护）
- ② BrainState 统一快照
- ⑤ Mind（Rust）与 Body（前端）解耦
- ⑥ 每个能力必须可关闭
- ⑧ 成本是设计约束
- ⑫ 沉默也是表达

## 2. 本地能跑

```bash
npm install
# 编辑 %APPDATA%\DesktopPet\config.toml 填入 LLM api_key（首次启动自动生成模板）
npm run tauri dev          # 必须 tauri dev，浏览器开 1420 会让后端命令失效
```

详见 [README · 快速开始](README.md#-快速开始)。

## 3. 测试

- 前端改动：`npm test`
- 后端改动：`cargo test --manifest-path src-tauri/Cargo.toml --lib`
- 新功能请补单测；涉及对话/记忆闭环的改动可在 [`src-tauri/tests/`](src-tauri/tests/) 跑 harness（需配好 LLM key）。

## 4. 提交约定

- Conventional commits：`feat: ` / `fix: ` / `docs: ` / `refactor: ` / `test: ` …
- 中文 body 可；本仓库单人单线，直接提交到 `master`。

## 开发心法

- **核心是陪伴，不是工具。** 新增功能前先问：这让她更像一个生命，还是更像一个工具？
- Mind（Rust）维护状态，LLM 只表达；Body（前端）只渲染——勿越界（原则 #1 / #5）。
- 每个新能力必须可关闭（原则 #6）。
- 详细的会话流程、踩坑约束见 [`CLAUDE.md`](CLAUDE.md)。

期待你的贡献 ✨
