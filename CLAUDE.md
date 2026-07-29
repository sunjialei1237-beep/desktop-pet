# CLAUDE.md — 项目入口

> 本文件每次会话自动加载。是**路标**，不是百科——详细内容读 `docs/`。
> 跨会话进度看 [`docs/HANDOFF.md`](docs/HANDOFF.md)（每次会话读 + 结束前更新）。

## 项目一句话
带记忆的 Windows 桌宠（Tauri v2/Rust + React/TS + Live2D + SQLite/sqlite-vec + BGE-M3 + DeepSeek）。核心是**陪伴**不是工具。成功标准：用户说"准备找实习"，一周后她主动问"实习怎么样"。
- 设计：`docs/specs/2026-07-14-desktop-pet-design.md`
- 实施：`docs/plans/2026-07-14-implementation-plan.md`（P0–P17）

## ⭐ 先读（真正的北极星）
`docs/Architecture-Principles.md` —— 12 条不可违背原则。**做任何技术决策前回查。**
精髓：LLM 只表达不维护状态(#1) / BrainState 统一快照(#2) / Mind-Body 解耦(#5) / 每个能力可关闭(#6) / 成本是设计约束(#8) / 沉默也是表达(#12)。优先级阶梯：活着 Body → 记住你 Memory → 懂你 Soul → 工具(砍)。

## ⭐ 踩坑约束（非显然，勿重复踩）
1. **运行时 config 在 `%APPDATA%\DesktopPet\config.toml`**，**不是**项目根的 `config.toml`（后者运行时不读）。改配置改 AppData 那份。
2. **必须 `npm run tauri dev`**。浏览器开 localhost:1420 会让所有 `invoke`/`listen` 失效（无后端）。
3. **DeepSeek v4 是 reasoning 模型**：新增 LLM 调用 `max_tokens` 至少 2048（分类）/ 4096（生成），否则 reasoning 独占预算、`content` 空、JSON 解析崩。诊断细节见 `docs/HANDOFF.md` §最近一轮。
4. **harness 随签名更新**：改 `converse`/`run_reflection` 等签名，同步更新 `src-tauri/tests/*` 所有调用方（memory_recall 曾因漏传 pacing 编译挂）。
5. **`config.toml` 已 gitignore**，API key 不入库。
6. **release 构建（桌面快捷方式）**：用 `npx tauri build --no-bundle`（**勿用** `cargo build --release`——后者 embed 不全、webview 加载异常）。产物在 `D:\cargo-target\desktop-pet\release\desktop-pet.exe`（CARGO_TARGET_DIR 重定向 D 盘，**非** `src-tauri/target/`；bin 名 `desktop-pet` 非 productName）。桌面快捷方式 `DesktopPet.lnk` 指向它。`open_devtools` 是 debug-only API，`commands.rs` 已加 `cfg(debug_assertions)` 守卫。**构建前必须关闭桌宠**：运行中的 exe 锁文件，cargo 覆盖报 `failed to remove file ... 拒绝访问 (os error 5)`；先 `taskkill //IM desktop-pet.exe //F`，等 ~3s 释放句柄，且 build 完成前勿重开快捷方式。
7. **release CSP（PIXI 崩，隐蔽）**：PIXI ShaderSystem 需 `unsafe-eval`，但 `tauri.conf.json` CSP 原本只有 `wasm-unsafe-eval`（给 Live2D Core）。**dev 模式 tauri 自动放宽 CSP（dev 永远正常），release 用配置 CSP 才暴露** → PIXI Application 创建即崩 → 桌宠空白不显示（后端/React 都正常，极难排查）。已加 `'unsafe-eval'` 到 `script-src`。`@pixi/unsafe-eval` 是更严格的 follow-up。

## 关键命令
```
npm run tauri dev                                          # 开发（桌面窗口）
cargo build --release --manifest-path src-tauri/Cargo.toml # 构建 release exe（desktop-pet.exe，前端已嵌入）；桌面快捷方式见 HANDOFF §部署
cargo test --manifest-path src-tauri/Cargo.toml --lib      # 库单测（快，无 LLM）
cargo test --test memory_recall       -- --nocapture --test-threads=1   # 闭环1
cargo test --test closed_loop2_harness -- --nocapture --test-threads=1  # 闭环2
cargo test --test soul_harness        -- --nocapture --test-threads=1   # Soul
F12                                                         # Debug Panel（仅 debug 模式）
```
注：除 `--lib` 外的 harness 调真实 LLM，需 AppData config 配好 key，慢（reasoning 模型）。

## 文档导航
- `docs/HANDOFF.md` — ⭐ 跨会话进度/交接（**每次会话先读**）
- `docs/Architecture-Principles.md` — 12 条北极星原则
- `docs/specs/...design.md` — 产品设计 v2（16 节）
- `docs/plans/...plan.md` — 实施计划 v1.1（P0–P17 + Kill List）
- `docs/decisions/` — ADR 决策记录（窗口策略等）
- `docs/archive/known-issues-2026-07-18.md` — 历史问题诊断（P1 视线已修复，留作上下文）
- `docs/followup-frequency-2026-07-20.md` / `proactive-recall-standard-2026-07-18.md` — 行为标准
