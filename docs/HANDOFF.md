# HANDOFF — 跨会话交接（压缩版）

> **新会话进入顺序**：① `CLAUDE.md`（自动加载）→ ② 本文件 → ③ 按需 `Architecture-Principles.md` / design / plan。
> **本文件是压缩版**（2026-08-27 改写，1150 行 → ~265 行）：每条已完成项**一句话**，只保留"交付了什么 / 关键决策 / 新踩坑"。
> **完整原始日志**（逐轮诊断链、验证证据、并行会话记录）在 **git 历史**：
> `git log --oneline -- docs/HANDOFF.md` 找到改写前的 commit → `git show <commit>:docs/HANDOFF.md`（1150 行原文）。
> **进度以 `cargo test` + harness 为准**，本文件是带上下文的快照，可能滞后于代码。
> **维护规则**：每次会话结束前更新 `§最近一轮` 与 `§待办`。超过一屏的诊断/验证细节写进对应 `docs/plans/` / `docs/decisions/` / `docs/review/`，此处只留一句摘要 + 链接。
> 最后更新：**2026-09-15（续⁶⁵）**·工作区收尾 + CodeGraph 重建 + ⭐**发现 `念头` 分支**（25 提交 / 已完成的"念头流 v3 三层决策" + 应用内更新推送）——**待决策合并路线，见 §待办 13**。
> ⚠️ **日期纪律**：本仓库文档长期以 08-27 为"今天"，实际系统日期已是 **09-15**（08-27 后停了 19 天）。新建文档/条目一律**以系统时钟为准**，别再跟着旧文档的日期写。

---

## 0. 项目一句话 & 当前进度

见 [`CLAUDE.md`](../CLAUDE.md)。三闭环驱动开发：活着 Body → 记住你 Memory → 懂你 Soul。

| 闭环 / 层 | 状态 | 锚定测试 |
|---|---|---|
| 闭环1 说→记住→跨会话召回 | ✅ | `cargo test --test memory_recall` |
| 闭环2 到期主动提起 | ✅ | `cargo test --test closed_loop2_harness` |
| 闭环3「她记得我」体感 | ✅ | 实跑：重启后问"我最近忙啥"→recall 出"找实习" |
| Soul 反思→念头外显 | ✅ | `cargo test --test soul_harness` |
| 库单测 | ✅ 567 passed | `cargo test --lib` |
| 工具层（4 工具 + 授权链 + fs 读写） | ✅ | `tests/tool_conversations.rs` / `p6_*` |
| 多供应商兼容 + 安装包发布 | ✅ v0.1.2 | NSIS + 便携 zip + GitHub Release |
| 生命感（视线/节律/微行为/Foley/情绪外显） | ✅ 代码层 | 部分待实跑（见 §待办） |
| **陪伴感 / 用户粘性** | ⚠️ **缺口已定位，方案待实施** | `docs/plans/2026-08-27-companionship-gap-and-belief-layer.md` |

**阶段判断**：三闭环全通、工程近乎工业级。**当前真正的短板不是功能，而是"她活着"的表达层**——详见下方诊断文档。

---

## 1. ⭐ 踩坑总表（非显然，勿重复踩）

> CLAUDE.md §踩坑约束 是精编 7 条（最常踩），下面是**完整版**，按领域分组。

### A. 构建 / 发布
1. **运行时 config 在 `%APPDATA%\DesktopPet\config.toml`**，不是项目根的 `config.toml`（后者运行时不读）。
2. **必须 `npm run tauri dev`**；浏览器开 localhost:1420 会让所有 `invoke`/`listen` 失效（无后端）。
3. **release 用 `npx tauri build --no-bundle`**，**勿用** `cargo build --release`（embed 不全、webview 加载异常）。
4. 产物在 `D:\cargo-target\desktop-pet\release\desktop-pet.exe`（CARGO_TARGET_DIR 重定向 D 盘；bin 名 `desktop-pet` 非 productName）。
5. **构建前必须 `taskkill //IM desktop-pet.exe //F`** 并等 ~3s：运行中的 exe 锁文件 → `failed to remove file ... os error 5`。
6. **dev HMR ≠ release exe**：前端/CSS 改动在 dev 热更"看着修好"，但桌面快捷方式不会自动更新 → 涉及前端的"实跑通过"必须在 release exe 上确认。
7. `open_devtools` 是 debug-only API，`commands.rs` 已加 `cfg(debug_assertions)` 守卫。

### B. 渲染（Spine / PIXI / WebView2）—— 高隐蔽区
8. **release CSP（PIXI 崩）**：PIXI ShaderSystem 需 `unsafe-eval`（已加）；PIXI/pixi-spine 建 `blob:` Worker 需 `worker-src 'self' blob:`（已加）。**dev 模式 tauri 自动放宽 CSP → dev 永远正常，release 才暴露**（表现为画布空白，后端/React 正常，极难排查）。
9. **pixi-spine `getBounds()` 返回 scale=1 的烘焙缓存**：`update()` 时烘焙，之后 `scale.set()` 不重算 → post-scale bounds 是谎言。必须在 scale=1 时量 `b1`，缩放手算。
10. **pixi-spine 在 `update()` 内部烘焙 slot transform/mesh 顶点**：`update()` 之后改 bone.rotation 永远进不了渲染（数值对、视觉零）。要注入必须**包装 `skeleton.updateWorldTransform`**（动画写 locals 后、烘焙前 ADDITIVE 加偏移）。
11. **`setAttachment()` 不更新渲染**：pixi-spine 渲染走缓存显示对象，只在 `Spine.update()` 内按 `slot.getAttachment()` 同步。→ 架构上已转向"状态→播动画，代码绝不碰 attachment"（续¹⁹）。
12. region sprite 缓存 key = **`attachment.name`**（mesh 分支才用 `attachment.id`）。
13. **setup pose 里默认显示的 slot 会永显**，必须有动画 null 它才会隐藏；**deform 只在 attachment shown 时可见**。
14. **`app.ticker.elapsedMS` 是每帧增量**（≈16.6ms 常数），不是累计值；拿相邻帧相减 ≈0 会让平滑系数冻结。
15. **渲染热路径（每帧 updateFn）绝不碰 IPC/emit**：曾每帧 `emit("face-state")` 导致 ticker 抛错 → 画布空白 → 窗口透明。任何渲染层改动全 try/catch。
16. 骨骼旋转必须 **ADDITIVE**（idle 动画每帧 key rotation，apply 会重置）；骨骼位移必须**绝对写入**（idle 不 key 位移，`+=` 会每帧累加导致头漂移）。
17. 2D 平面骨骼旋转只能表达左右；上下俯仰只能靠位移 → 必然让头脱离脖子（"头飞起来"）。垂直视线留给美术做 look_up/look_down 动画。

### C. 调试方法论
18. **GDI `CopyFromScreen`/`PrintWindow` 拍不到 WebView2 的 GPU 合成内容**（WebGL canvas 与 DOM 都不可见）→ 验证前端渲染一律走 **CDP**：`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` + `Runtime.evaluate`/`Page.captureScreenshot`（release exe 也能开）。
19. **视觉模型（GLM vision）在这种截图上会幻觉**（把编辑器 UI 当角色、把上半身判成完整）→ **数值诊断优先于视觉模型**。
20. CDP 区分主窗/debug 窗要用 `getCurrentWindow().label`（`document.title` 相同）——前几轮截图对比全截在 debug 窗导致 diff=0 误判"没效果"。
21. dev-only 诊断句柄是有效的后门：`window.__spine` / `__gazeDiag` / `__ctDiag` / `__dragDiag` / `__pet`（`import.meta.env.DEV` 守卫，prod grep 零命中）。`.bounds-overlay` 命令式定位可读模型实机几何（免改代码）。
22. Tauri v2 dev 无 `__TAURI__` 全局，但 `__TAURI_INTERNALS__.invoke(cmd, payload)` 可用（签名 `(cmd, payload={})`，非 `args`）。

### D. Rust / LLM / DB
23. **DeepSeek v4 是 reasoning 模型**：新增 LLM 调用 `max_tokens` 至少 2048（分类）/ 4096（生成），否则 reasoning 独占预算、`content` 空、JSON 解析崩。
24. **改 `converse`/`run_reflection` 等签名，必须同步所有 harness 调用方**（`src-tauri/tests/*`）；`RetrievalResult`/`ConverseCtx` 加字段同理（所有显式构造点）。历史多次因此编译挂。
25. **流式 `chat_stream` 不支持 tool_calls**（Delta 无该字段，静默丢弃）→ 工具轮非流式 `chat()`，最终答案轮流式。
26. `ThreadRng`（Rc-based）非 Send，不能跨 `await` → 必须收敛到独立块内 drop。
27. SQLite `ADD COLUMN` 只写新 migration、**不回写 001**（无 `IF NOT EXISTS`）；新库按序跑覆盖。
28. `PRAGMA foreign_keys = ON` → 插父表（reflection）必须在子表（thoughts）之前。
29. **FTS5 对中文不可行**（trigram 需 ≥3 字 / unicode61 不分 CJK / ascii 只认 ASCII）——已证伪，**除非引入 jieba 扩展否则勿再尝试**（见 §已证伪）。
30. 并行 `rustc` 编译测试目标可能报 `os error 1455`（页面文件太小）= 内存压力非代码错，加 `-j 2` 即过。

### E. 多会话协作（本仓库曾多会话并行）
31. **并行会话期间验证一律用 `git worktree`**：`git worktree add D:\桌宠-wt <commit>` → 只放/只测自己的改动 → 完事 `git worktree remove --force`。
32. **`git add -A` 会捎带他人未提交改动** → `git add` 前逐文件核对 diff 归属；`git status` 查对方 stage。
33. worktree **不继承** `.cargo/config.toml` 的 `CARGO_TARGET_DIR`（该文件本地未提交）→ 构建产物落 worktree 自己的 target，release 需手动拷 exe。
34. 多实例会抢 Alt+Space（先启动者注册成功，后者 WARN）；测试前先确认单实例。

### F. PowerShell / Windows 自动化
35. **PowerShell 默认 DPI-unaware**：150% 屏上 `GetWindowRect`/`SetCursorPos` 坐标被虚拟化 → UI 自动化必须先 `SetProcessDPIAware()`。
36. **含中文的 `.ps1` 必须 UTF-8 BOM**（PS 5.1 按 ANSI/GBK 解析会炸；here-string 结束符 `"@` 必须顶格）；PS 5.1 也会把中文注释误读致 `Add-Type` 失效。
37. `taskkill //IM` 在 PowerShell 里参数无效 → 用 `Stop-Process -Name desktop-pet -Force`。
38. 发中文消息给桌宠：SendKeys 过不了 IME → **剪贴板（base64 传输防编码损坏）+ Ctrl+V** 是唯一可靠路径（`scripts/send_pet_msg.ps1`）。
39. 桌面快捷方式迁移时 **TargetPath 和 IconLocation 都要查**（续²⁹ 只改 TargetPath，C 盘删除后图标变白纸）；Icon 建议指 `desktop-pet.exe,0`（内嵌）；改完 `ie4uinit` 刷缓存。
40. **`.tauri-signing/liri-updater.key` 是更新器签名私钥，永不入库**（已 gitignore）。`.qa-download/`（QA 拉下的安装包）同属临时产物。

### G. 工具链 / CodeGraph
41. ⭐ **同一仓库有第二条第开发线：master 上查不到的东西，先去分支找（2026-09-15 踩，代价很大）**：CodeGraph 旧索引里长期存在 `src-tauri/src/soul/stream.rs` 等文件，而 master 工作树没有、`git log --diff-filter=D` 也查不到 → 我据此误判为"索引删除不清理产生的幽灵"，还把结论写进了踩坑与方案（已推送，本条为更正）。**真相：它们是 `念头` 分支上的真实文件**（本地 + `origin/念头`，`soul/stream.rs` **1156 行**、`db/thoughts.rs`、`migrations/008_thought_stream.sql`、三个 harness；分支共 51 文件 +7350 行，另含应用内更新推送）。索引是在 `念头` 被检出时建立的，切回 master 后未重建（209 文件 vs master 实际 163）。**后果**：差点在 master 上重造一套分支里早已实现且更完整的东西。
   - **纪律**：查到"磁盘上没有 / git 没有"的符号时，先 `git branch -a` + `git log --all --oneline`，**不要**先假设索引坏了或代码丢了。
   - **分支间 schema 会留在运行时库**：`%APPDATA%\DesktopPet\desktop_pet.db` 的 `schema_migrations` 已有第 8 行、`thought_stream` 表存在，而 master **没有 008 迁移文件** → master 构建跑在这个库上属 schema 未定义状态（未验证是否报错）。跨分支切换后先核对活库迁移号。
42. **CodeGraph 重建命令**：`codegraph init -i` 对已初始化目录会**拒绝覆盖**，必须 `codegraph index -f`（全量）或 `codegraph sync`（增量）。`codegraph_status` 的文件数/节点数可作体检指标；重建后抽查一个符号确认结果符合预期。

---

## 2. §最近一轮（压缩，仅近 6 轮）

> 逐轮完整诊断链见 git 历史；每条一行。

- **续⁶⁵（09-15）工作区收尾 + 工具链体检 + 交互方案（无代码改动）**：①把 08-27 遗留的未提交文档入库（HANDOFF 压缩版 1150→274 行 / 陪伴感方案）；②`.gitignore` 补 `.tauri-signing/`（**更新器签名私钥，此前既未跟踪也未被忽略，差点随 `git add -A` 入库**）与 `.qa-download/`；③`scripts/friend-diagnosis/` 去 zip 存明文源文件并实测跑通（HTTP 200，顺带确认 config 已切回 DeepSeek → 解锁待办 1）；④**CodeGraph 全量重建**，清掉 46 个幽灵文件（JS 38→10、Python 17→5），见 §1 踩坑 41；⑤交互「不死板」增量提案（§待办 14）；⑥⭐**发现 `念头` 分支**（08-27 11:41 分叉，`念头` +25 提交 / 51 文件 / +7350 行）：内含**已完成的"念头流 v3 三层决策冒泡架构"**（`soul/stream.rs` 1156 行 + `008_thought_stream.sql` + 三 harness）、应用内更新推送、一批动画 UI。我先前把它的文件误判为"索引幽灵"（§1 踩坑 41 已更正），并据此写了一份重复方案——现降级为增量提案。
- **续⁶⁴（08-27）首次访谈被拖拽杀死修复**：拖拽时 mousedown/mouseup 的 client 坐标重合 → 浏览器合成 click 被当"摸头" → 反应气泡顶掉访谈问题且无重显 → 访谈静默卡死。修法三件：捕获阶段截停合成点击（窗口期由 `wasDraggedRef` 覆盖）+ 补齐气泡守卫（摸头/proactive-prompt/proactiveTimer 三条路径）+ **兜底网**（访谈 active 而气泡消失 → 400ms 后自动重显当前问题，120s 超时自愈）。纯前端（`App.tsx`），**待 release rebuild 真机复验**。
- **续⁶³（08-27）快捷方式子系统重构**：抽 `src-tauri/src/lnk.rs`（三处散落 .lnk 解析归一）+ Recent 反查 30s TTL 缓存（省 ~2.9 万次/天全量列目录）+ `dedup_first_seen` 跨根去重。lib 567 绿。
- **续⁶²（08-27）设置面板 UX + 视觉整版**：拆「固定 header + 可滚动 body」（修"滚不动 + × 关闭钮被裁出屏幕"）、Esc 关闭、紫色系改陶土橘棕；顺修 `.settings-tools-toggle` flex 被通用 label 规则压制的真 bug。
- **续⁶¹（08-27）`open_application` 假成功修复**：spawn 成功 ≠ 程序起来（抖音类启动器架构冷启动几十秒）→ spawn 前拍进程快照 + 差分轮询 ≤2.5s（噪声名单 18 个防 explorer 回声），检测不到就**如实说"没检测到新进程"**。
- **续⁶⁰（08-26）搜索"搜不到"修复**：`EXTERNAL_INFO_KEYWORDS` 补"帮我找/找一下/找一找/找找/找一篇"（**刻意不加裸"找"**——"找工作/找实习"人生话题与"找不到"情绪必须保持 None）。这是该机制第三次同型修补，结构性观察见下。
- **续⁵⁹（08-26）角色沉浸思考灰度**：`[prompt] enable_immersion_thinking`（默认关，用户已开）+ `inner_os_probability=0.1`；A/B 实测首字仅 +1s、每轮 +62-92 reasoning token、前缀缓存零损伤；`reasoning_effort:low` 为阴性结果不接线。顺修用户 config 的 `platform.deepseek.com`→`api.deepseek.com`（网页域名对 API 恒 405）。

---

## 3. §待办（接手即看）

**阻塞中 / 等外部条件**
1. **DeepSeek 真实命中率验收**——✅ **key 已就绪**（2026-08-27 实测：`api.deepseek.com/v1` + `deepseek-v4-flash`，诊断脚本 HTTP 200，已不是 Agnes）。用 `[llm-cache]` 日志 / DebugPanel 看命中率（预期 80%+）。
2. **Agnes 500 条 provider matrix 结案**——外部阻塞：中转全天在「宕机 ↔ 复活 ↔ 令牌无效」间震荡，有效数据 235/1533 轮。harness 与增量报告就绪，择健康期补做整跑即可。

**真机验收（代码已就绪）**
3. 续⁵⁹ 沉浸思考灰度观察：release rebuild + `[converse] immersion thinking ON (os_allowed=..)` 日志、TTFT 体感、OS 出现频率（期望 ~10% 且永不连发）、长对话延迟复验（A/B 只跑过 4 轮短历史）。
4. 设置面板：key 回显 + 👁 切换、保存后「已保存的模型」出现该方案、两案并存、点「使用」切换后即时生效；重建 release 后确认早安话术不再出现周中「新的一周」类说法。
5. **续⁶⁴ 首次访谈拖拽修复（`acaad1c`）需 release rebuild 后真机复验**（`App.tsx` 纯前端改动，dev HMR 会掩盖问题；当前快捷方式 exe 停在 v0.1.2，不含此修复）——复验动作：首次访谈进行中拖一下桌宠 → 问题不消失、不出现"呜…啊…"反应气泡、拖完仍能正常答题。

**已知小债（低优先，随手可做）**
6. `system.txt` 危机守则里的「稳稳接住」需换词（口癖榜词，续⁵⁹ 调研发现）。
7. profile 列表暂无手动重命名/自定义名字（自动以 main_model 命名）。
8. `scan_apps` 可见性维持私有、`src/shortcuts.ts` 前端死文件（无人 import，仅记录不删）。
9. `.rtf` 理论 OLE 面仍在 open_file allowlist（续⁵⁵ 遗留观察项）。
10. turn_root 跨轮不清但每环境轮重钉（泄漏面极小，观察项）；`read_authorized=true` 的 apply 不复查 grant（已注释论证）；单槽 undo 仅一步。
11. **结构性观察（续⁶⁰）**：rules-based prefilter 每漏一种自然说法就"我做不到"（已三次同型修补）。若再发生，考虑 (a) ExternalInfo 候选放宽为常态广告 + 纯靠 LLM 弃权（黑名单测试已证模型会弃权），或 (b) 门控小模型化（`gate.rs` 同款 flash 路由，每轮 +1 次 flash 调用）。

**产品方向（本轮新立，见下）**
12. ⭐ **陪伴感缺口 + 信念层方案**：`docs/plans/2026-08-27-companionship-gap-and-belief-layer.md`——诊断"没有粘性 / 陪伴感不足"的根因，核心方案是新增 **Belief（信念）层**（可改口的看法）+ 身体/声音表达 + 冒泡加"由头"。**建议下一会话从这里开始。**
13. ⭐⭐ **`念头` 分支去留（阻塞项，最优先）**：`念头`（含 `origin/念头`）自 08-27 11:41 分叉后已 **25 提交 / 51 文件 / +7350 行**，内含**已完成的"念头流 v3 三层决策冒泡架构"**（`soul/stream.rs` 1156 行 + `008_thought_stream.sql` + 三 harness + Debug 分区，方案见该分支 `docs/plans/2026-08-27-thought-stream-plan.md`）、**应用内更新推送**、一批动画/UI 改动。master 这边 8 提交。**需决定：合并 / 继续在分支上开发 / 弃用。** 该决定同时阻塞：①`pet_events`（见待办 14）的迁移号与实现基线；②更新器与动画改动是否入主线。**切分支前后务必核对活库迁移号（踩坑 41）。**
14. **交互「不死板」增量提案（待审，仅剩两件事）**：`docs/plans/2026-09-15-interaction-aliveness.md`——原方案机制 ①由头+想要 ②情绪有对象 ③连续/节奏 已被 `念头` 分支实现覆盖（作废）；**仍有效的只有**：①`pet_events`（她自己的生活——分支 8 条喂流全以用户/环境/时间为对象，**没有一条是"她自己做了什么"**，是唯一真空白）；②客观验收（burst 计数替代 CV、7 天去重/多样性、盲评）。同文件 §A 含对 GLM 评审的逐条核实（GLM 指出我的 perception 事实错误——**它是对的**；它怀疑"幽灵文件"——**这条它错**），§C 是我的自我纠错记录。

---

## 4. 已完成历史（一句一条）

### 4.1 当前阶段：环境/文件/工具/成本（2026-08-17 ~ 08-27）
- 续⁵⁸ 按角色分模型（成本路由）：`[llm.gate]`/`[llm.extractor]` 可选端点 + `chat_core` 端点参数化；30 轮全角色验收 **≈0.006 元/轮 → 200 轮/天 ≈1.2 元/天**；选型结论 gate 推荐 glm-4.7 或 v4-flash、extractor 推荐 glm-4.7/v4-flash。
- 续⁵⁷ 多供应商兼容层：build_url 四形态 / Usage 归一 / `thinking` 字段家族白名单 / `LlmError::Balance` 分型 + provider matrix harness（`MATRIX_*` env 覆盖式）+ v0.1.1 安装包。
- 续⁵⁶ 模型配置界面：API key 回显 + 👁 切换 + `[[llm_profiles]]` 方案列表一键切换（`apply_llm_profile` 立即重建 LlmClient，免重启）。
- 续⁵⁵ DeepSeek 接手 24 commit 全量复审：修 `find_patch_block` 大小写映射错切 / `undo_last_edit` 乐观锁 / open_file 白名单矛盾（移除旧版 OLE 格式）。
- 续⁵⁴ 发布链路：NSIS 安装包（`Liri_0.1.0_x64-setup.exe`）+ 便携 zip + `SetupWizard` 首启向导（key 真实连接验证 + BGE-M3 下载）+ GitHub Release v0.1.0。
- 续⁵³ DeepSeek 前缀缓存大修：`messages[0]` 只留静态（persona/模板/grounding），易变内容（[Memories]/关系数字/里程碑/review）移入尾部；检索排序加 id tiebreaker；working memory 改整批裁剪。命中率 50%→80%+，账单降至 ~1.5 元/日。
- 续⁵²·6 / 续⁵²·3~5 env-fs 真机终修：planner「我打开的…」环境优先 + pet/debug 自聚焦回退保留 pid + `hydrate_relative_path`（turn root + 有界目录搜索）+ F12/环境 root 实时走 Recent-lnk 反推。
- 续⁵¹ env-fs 深度审查：4 Critical（假授权根除 / 环境字段控制字符剥离 + 分字段截断 / git output 改 `spawn_blocking`+watchdog / fs_grants 最长前缀仲裁）+ 4 High（once 只在成功使用的 canonical root 烧票 / registry 仅 NotFound 写空 / search 目录级剪枝）+ M2 顺修，全带回归测试。
- 续⁵¹·2 Medium 小轮：进程缓存限容 256 / deny 冷却等价根归一 / 项目 hint 大小写不敏感 / 多被拒 root 统一 resolve / `fs_grant_access` 入库前预检。
- 续⁴⁸ 内存治理收尾：int8 vs fp32 质量验证通过（基准集 top-1 逐条一致）→ 删 2.16GB 旧文件；小模型 P3 评估**否决**（真实库 top-5 重合度仅 0.52）。
- 续⁴⁷ P2 内存治理：embedding 懒加载（`with_lazy`）+ 空闲卸载（60s 看护线程）→ 闲置 870MB → **49.5MB**；`lazy_load` 默认 true / `idle_unload_minutes` 默认 30。
- 续⁴⁶ P1 后台内存治理：fp32→int8 量化（2161MB→570MB）+ ORT 调优（`with_device_allocated_initializers` 为最大变量）→ 主进程 1500MB → **870MB**。
- 续⁴¹·7 陈旧记忆不浮现：候选年龄信号（"N天前记下"）+ 选择器惯性衰减规则（小愿望两周翻篇 / 大事可问一个月）+ 发声层禁报日期出处。
- 续⁴¹·6 火锅气泡三连修：forget 扫除同话题 episode（`execute_candidate_with_sweep`）+ `[Memories]` 日期显著化（今天/昨天/N天前）+ 无锚 lonely/welcome 改 identity-only 检索。
- 续⁴¹·5 重启问候活化石清除：删 `lib.rs` 无条件 2s 硬编码气泡 + medium loop 首 tick 从 30s 提前到 5s（早安即时）。
- 续⁴¹·4 周日总结默认关闭（用户决策"两个朋友聊天不会每周复盘"）。
- 续⁴¹·3「流星雨」气泡溯源：仪式路径补写 `bubble_log`（选择器从此看得见仪式内容）+ 周总结 prompt 加名词级原意约束 + 反硬凑。
- 续⁴¹·2 选择器真 LLM 冒烟三轮迭代：修 schema 早退守卫（v5 库永不被迁移）/ 池子机械预选饿死重要事实 / 氛围否决 / id 抄写降级（改位置短编号），48 窗口带记忆落率 ≈4%。
- 续⁴⁵ 拖拽跟手丝滑化：`cursor.rs` 轮询 16ms→8ms + `timeBeginPeriod(1)`（~125Hz）。
- 续⁴⁴ 顶部停靠气泡移到头顶右侧 + 拖拽后光标错位根因修复（新增 `moveWindowTo` 统一入口，setPosition 前同步 origin）。
- 续⁴² 重启问候多元化：新 `src/greetings.ts` 本地池（离开时长分桶 × 时段风味，同桶不连续重复，**零 LLM**）。
- 续⁴¹ 记忆浮现"值不值得说"交还 LLM：新 `pending/selector.rs`（候选池 + LLM 选择 + **可弃权**）+ `db/bubble_log.rs` + 跨气泡连续性 `last_bubbles_clause`。
- 续⁴⁰ 重启问候单声化：三问候源协调（早安 > 睡醒罐头 > 念头）；念头等待安静窗口（45s 无气泡）才出。
- 续³⁹·3 手动拖拽 + 屏幕墙钳制：放弃 OS 原生拖拽（会穿模），改 global-cursor 管线驱动 + `clampModelToScreen` 四墙。
- 续³⁹ 拖拽落体彻底关闭（`ENABLE_POST_DRAG_FALL=false`）：三轮"回位"报告实为同一诉求「**放哪停哪**」。
- 续³⁸ 拖拽松手回原位根治加固：武装时取真实 `outerPosition()` + 要求 cursor 事件新鲜（<1.5s）+ >2px 失配遥测。
- 续³⁷ 落体手感调参 → 回退（用户偏好原手感；慢速长下落被读作"朝放下点回滑"）。
- 续³⁶ 拖拽剧烈晃动根治：`GetAsyncKeyState(VK_LBUTTON)` 拿 **OS 按键真值**门控物理循环，取代"窗口静止 300ms"猜测。
- 续³⁵ Soul v2 灵魂工程全链路：L2a 静态/近端消息拆分（时间+情绪+Intent 移到历史之后的末位）+ `system.txt` v2（认知透镜 + 14 示例 + 温和推回）+ tone_hint 表达许可措辞 + distress 让位；评测 M1 盲认 3.87→4.43、缓存命中 80-90%。
- 续³⁴ 二期第一梯队三连：晚安仪式（接管"该睡了"nudge）+ 周日总结 + 关系里程碑（7/30/100/365 天，降序覆盖语义）+ Memory Serendipity（弱相关带 [0.15,0.45]，1/3 概率）。
- 续³³ 搜索源国外优先级联：DDG 先试（5s 预算）+ 失败/超时记 10min 冷却走头条兜底（"尝试本身即探测"，无需单独 ping）。
- 续³² 主动冒泡治理五修：全局预算持久化（`last_proactive_bubble_at` + 原子 check-and-occupy）/ 记忆比例 30→**15%** / 7 天硬排除 + 确定性轮转 / 加"可不问" / deictic 时间词剥离（`mind/deictic.rs`）。
- 续³¹ 借鉴 memory-trigger 三功能：承诺追踪（`pending_events.origin` user/pet + `pet_promise` 抽取）+ `recall_reason`（为什么此刻想起）+ 情感锚点（`episodes.emotion_anchor`）。
- 续²⁷ 工具层 7 阶段全落地：三层门控（Planner Capability Gate → LLM `tool_choice=auto` → Tool Policy）+ 4 工具（search_web/get_time/open_application/open_url）+ **三条铁律写进 `Architecture-Principles.md`（#13/#14/#15）**。
- 续²⁶ Rituals 早安仪式：日期驱动 + presence Active + 每日一次 + 与 welcome-back 协调（早安优先）。
- 续²⁵ 计划文档对齐 + 完成度审计：P0-P17 主干 100% 完成，真缺口仅 P10.2 Spine 表情映射；用户砍 3 项（窗口边缘坐姿 / 注意力 Focused+Ignored 两态 / 走路）。
- 续²⁴ 全面测试验收 + Live2D 全移除（-10688 行）+ forget 消歧义修复（confidence gap 0.15）+ extractor 文风/规则（便签风 2-8 字 / 瞬时 desire 不进 fact）+ 记忆卫生数据治理。
- 续²³ AIRI 风格视线驱动：头绕鼠标 ±10° + 身体 ±3° 微侧，径向衰减 `GAZE_RANGE=320` + 平滑回正（**仅水平通道**，下巴必须固定）。
- 续²² Live2D 全移除（代码 + `public/live2d` 3.4MB + npm 依赖），Spine 为唯一渲染。
- 续²²b 音效治理：全局单音互斥 + 静默优先（menu 60% 静默）+ 全局最小间隔 **10s**。
- 续²¹ 记忆浮现多样性：`novelty=exp(-recall_count/5)` 进权重 + `sample_surface_anchor`（softmax 加权抽样 + 12h 冷却）+ `reinforce` 改边际递减 `+0.03*(1-strength)`。
- 续²⁰ 气泡尾巴锚点固定璃头顶右侧：几何推导锚点盒 + 删 `translate:-50%` 漂移源 + 删 `.bubble-pet` 覆盖规则（CDP 实测差 1px）。

### 4.2 Spine / Liri 渲染落地（2026-08-09 ~ 08-12）
- 续¹⁹ Spine 表情架构转向（用户钦定）：**状态/情绪 → 播放对应动画（叠加 track），动画 timeline 自己管 slot，代码绝不 `setAttachment`**；删掉 phase3 运行时覆盖整套产物。
- 续¹⁸ Debug 窗口死锁修复（sync command 在主线程 `build()` 阻塞消息循环 → 改 `async`）+ pixi-spine `forceSyncSlot` 诊断链路（后随续¹⁹ 回退）。
- 续¹⁵ Debug Panel 独立 OS 窗口（`WebviewWindowBuilder` label=debug；主诉求"不挡璃"达成，实跑白屏留 follow-up）。
- 续¹⁴ Spine driver phase3-A：emotion→半睁眼持续映射（后随续¹⁹ 架构转向废弃）。
- 续¹³ Spine driver phase1：**单一串行动作通道**（blink/ear/tail/smile 互斥）+ **呼吸节拍对齐**（ear/tail 只在 `body_breath` 每轮 complete 触发，零跳变）+ 双时钟（`deltaMS` 驱动播放随昼夜变速 / `elapsedMS` 驱动间隔保持稳定）。
- 续¹² Liri Spine 全身显示：修两个 **release-only** bug（CSP 缺 `worker-src` / `getBounds` scale 缓存谎言导致只显上半身）。
- 续¹¹·补² Liri 设为默认渲染 + 加载失败自动回退 Haru（永不空白）。
- 续¹¹ Spine 链路里程碑1：资产加载 + 显示 + `body_breath` 呼吸 + 生成两份 spec（`skeleton_structure.md` / `animation_spec.md`）。
- 续¹⁰ 选择性遗忘：多轮消歧义（跨轮 `pending_forget` slot + 序数词解析）+ fact/pending 语义匹配（`semantic_rerank` **只提升 char_overlap>0 的条目**——BGE-M3 无关基线 ~0.5 映射后 0.75 会伪造候选）。
- 续⁹ 记忆卫生层：写入闸门（`mind/memory_gate.rs`：category 白名单 + 噪声 key/value deny）+ 检索纯化（`retrieve()` 删 reinforce 副作用，新增 `reinforce_top` 仅供 genuine-recall）+ 去重视野（`known_facts` 全类 30）；firecrawl 调研 mem0/MemGPT/Zep 定"不造什么"。
- 续⁸ 自主冒泡频率修复 + 灵性重构：修 `commands.rs` 硬编码 `now-31min` 绕过门控的 bug；新 `generate_lively`（70%，不调 retrieve，注入时段+情绪驱动 prompt）。
- 续⁸b lively prompt 反同质化：成品词 → `time_hint`/`mood_hint` + 显式禁套路报时词 + 具体小切入点菜单。
- 续⁸c lively 允许轻好奇提问（续⁸b 完全排除提问是过严）；新增 `tests/bubble_content_check.rs`（N=15 真实 LLM 内容回归资产）。

### 4.3 对话质量 / 记忆深化（2026-07-31 ~ 08-09）
- 续⁷ 完成 + 速度/性格/幻觉根因 6 轮 A/B：主回复关思考（`ThinkingConfig::disabled()`）→ max 4s / mean 2.7s；grounding 空记忆**显式标记**（此前省略导致编造"你上次说…"）；披露 G6 越界 6/10 是"上次说"framing 的性格同源 trade。速度达标 → gate/extractor 并行优化不做。
- 续⁶ 真人感 prompt 调教：150 条 A/B，`system.txt` 反 AI 味 4 条 + engage"可不问"；提问结尾率 35%→**14%**。
- 续⁵ BrainState 扩 prompt/budget **经评估关闭**（ADR：`intent` 是 planner 输出会循环依赖 + 捆绑 3 个无用字段）。
- 续⁴ `idle_weights` JSON 化（数据↔逻辑解耦，`idle-behaviors.json`）。
- 续³ 害羞慢现气泡：`derive_mood_label_with_closeness`（closeness < 20 时中性/正向标签覆盖为「害羞」，但不掩盖真实 distress）+ `bubble-shy` 1.2s 慢揭幕。
- 续² Alt+Space 全局唤醒（`tauri-plugin-global-shortcut`）。
- 续 B5 三层人格评估：规则启发式 + 语义 cosine + **LLM-as-judge**（30 条 golden 集 + 3 次指数退避重试防 rate-limit 静默零分）。
- 自主批次（08-08）：`perception/focus.rs` 深度专注接线（25min 阈值，此前硬编码 false 空转）/ `lifecycle/scheduler.rs` 观测层（11 任务注册表 + DebugPanel 分区）/ Grounding B 档运行时阻断（中文 claim 模式 + 二次重试 + 仍编造则抑制冒泡）/ 全局 `BrainState<'a>`（`planner::plan` 5 散参合并）。
- 自主批次（08-07）：鲁棒性加固（main 空回复重试）/ `ConverseCtx` 9 参合并 / 记忆可视化编辑（`forget_fact`/`delete_episode`/`set_emotion`）/ loneliness 收尾（ Sleeping 守卫 + 摸头降孤独 -0.1）/ 死代码清理（删 `homeostasis.rs` 双实现）/ `verify-checklist.md` 扩写。
- 08-07 关系进展摘要（Hermes 后台 review）：每 15 新 episode 产出 1-2 句关系总结，注入为 always-on `[Relationship]` 区块（新表 `relationship_reviews` + `soul/review.rs`）。
- 08-07 激活 loneliness：`needs.rs::tick_loneliness` + `generate_lonely_bubble` + `check_lonely_nudge`（loneliness>0.6 + closeness≥20 + Active + 30min cooldown）。
- 08-05 100 条提示词质量评测 4 轮迭代（98/100 硬检查通过，0 真乱扯，知识问答 20/20）。
- 08-05 选择性遗忘扩展 fact/pending + **FTS5 中文证伪**（从 backlog 永久移除）。
- 08-04 修复 opencode QA 直答 4 问题：补身份层 DB 读 / `qa_system_prompt_budget()=505` / 强制 action=normal / 跳 grounding check。
- 08-04 QA 直答路由 + `system.txt` 正向重写 + Hermes 记忆优化：新增 `GateRoute::Question`（跳 extractor/检索，防知识问题被硬套宠物话题）；**用户消息永不压缩**（Hermes 规则）+ landmark episode 独立 `[Milestones]` 区块。
- 08-04 选择性遗忘 episode MVP：gate `Forget` → 语义匹配 → **置信度门在 `score_breakdown.semantic`（0.7，非 total）** + landmark 保护 → Rust 硬删 + 向量清理 → 确认时**禁复述**。
- 08-04 构建重定向 D 盘（`.cargo/config.toml` 的 `CARGO_TARGET_DIR`）。
- 08-04 #10 生命感收尾：`rest_need` 暴露 + 激活（此前生产 homeostasis 从不更新它）+ `circadian.speedModifier` 接 PIXI `ticker.speed`。
- 08-03 续⑧ B4-余余（AnimFSM + Prompt token 分区 → **Debug Panel 9 分区全补齐**）+ B5 Golden 评估框架。
- 08-03 续⑦ sleep 首次有测试：加 vitest + 抽纯逻辑（`shouldAutoSleep` / `applySleepyWeight`）+ 24 前端单测 + dev-only `window.__pet` 验收钩子（重写 `Date.prototype.getHours` 模拟时段）。
- 08-03 续⑥ 清测试：2 个 stale golden（焦虑→care 有意改 / 中文维度 `温柔`）改测试不改生产。
- 08-03 续⑤ Settings 下载（Qdrant 401 → Xenova/hf-mirror）+ 暂时离开 = 最小化到系统托盘（此前只设标志窗口根本没隐藏）。
- 08-03 续④ BGE-M3 embedding 接入 + 检索质量翻倍（语义 Hit@3 33%→67%、avg sem 0.035→0.741）；顺修 ort rc.12 加载 bug（Level3→All）。
- 08-03 续③ 深度审计（对照 P0-P17 的代码级核验表）+ #11 可观测簇：修 `conversations` 死表（生产从未调用，导致幻觉无法回溯）+ 决策链三分区（Retrieved/Intent/Reflect）+ Cost 计数（首次暴露单轮 3 次 LLM 调用）。
- 08-03 合并 opencode 副本：Consolidation 反向更新 Facts（`backfill_facts`）+ 完整物理（自由落体/任务栏弹跳）+ 实跑方法论；**踩坑**：`startDragging` 吞 webview 鼠标事件（无 mouseup）。
- 08-03 续² Liri 角色方向确认：最终角色 = 璃 Liri（小狐灵），动画走 Spine+PixiJS（不用 Live2D）；人格配比落进 `system.txt` + `firstrun.rs::seed_persona`。
- 08-03 续 B3 Sleeping 配套：睡着抑制 nudge + sleep 音效 + LateNight 只 yawn（本就满足，零改动）。
- 07-31 主动开口幻觉 grounding A 档收紧（retrieve 锚 + intent goal 驱动 + 空检索不说话）。
- 07-31 气泡 release rebuild 闭环 + consolidation `max_tokens` 2048→4096 + Reflection 事件驱动触发器（TurnThreshold 30 条 / MajorEvent importance>0.85）+ Sleeping 入睡机制（DeepNight 无交互 ≥10min）。
- 07-31 早些 Foley 接线补全 + 频率调整 + 气泡位移。
- 07-31 converse 注入 surfaced thought（Tier2 #4，零额外 LLM 调用）。
- 07-29 Foley 音效真实素材接入（10 个）+ circadian 接入微行为权重 + 气泡生命力（打字节奏随情绪 `bubblePacing` + 无文字 glyph 气泡）。
- 07-28 流式回复从 `emit/listen` 改 `ipc::Channel`（`emit` 命令体内投递延迟 + listener 立即 unlisten 会全丢）+ 情绪外显连续表情插值（P10 `emotionBridge`）。
- 07-27 Soul 慢循环闭环：Reflection 自动调度 + thought 融入回来招呼 + Consolidation 调度；`welcome-back` 回来主动招呼。
- 07-26 docs 治理 + `proactive_harness` 简化 + 提醒功能修复（闭环2 真实运行 ✅）。

### 4.4 MVP 主干（2026-07-14 ~ 07-26）
- P0-P17 主干全部实现并跑通：脚手架/配置 → DB（8 层记忆 + sqlite-vec，schema v2）→ BGE-M3 embedding（进程内 ONNX）→ LLM 客户端 → Emotion（state/homeostasis/needs/pace）→ 摄入管道（gate/extractor/store/correction/working）→ 检索管道（trigger/retrieval/budget/grounding，score breakdown）→ Planner（director+actor）→ Pending Events（闭环2）→ Body（透明窗口 + 点击穿透 + FSM + 微行为 + 坐姿/物理）→ 交互（摸头/戳/注意力）→ Soul（reflection/monologue/consolidation）→ 感知（time/presence/window）→ Life Loop（三循环 + recovery 角色化）→ Debug Panel → Golden Conversations。
- **三闭环端到端跑通**（含真实运行），Kill List 解锁。

---

## 5. Backlog（待开发，按优先级）

**Tier A — 陪伴感（本轮新立，最高优先）**
- ⭐ Belief（信念）层：可改口的看法 + `[你眼中的他]` 透镜注入 + 冒泡"由头"机制。方案见 `docs/plans/2026-08-27-companionship-gap-and-belief-layer.md`。
- 身体表达层：事件→姿态反应（welcome-back 抬头 / celebrate 弹跳 / care 下沉）；情绪→可见状态（不只是微行为权重）。
- 声音：TTS 接入（先覆盖 早安/晚安/欢迎回来/里程碑 几句仪式性的话）。
- 初次登场时刻（设计 §7.6 标"极其重要"但未做）。
- 可累积的视觉痕迹（窝里堆东西 / 状态可见变化）——**不需要 K 帧，绕开 Spine 产能瓶颈**。

**Tier B — 感知与生活**
- 感知型 Episode（"连续工作 8 小时"入记忆）。
- Curiosity / Habits（她注意到你的习惯并主动问）——设计 §14 二期。
- 喂食 / 拖文件当礼物。

**Tier C — 工程债 / 低优先**
- `[Environment]` 自适应（turn_root 跨轮清理）。
- 混合检索 V2 / 重排序 V3。
- Adaptive Traits V2 / Persona 审批流。
- 跨显示器 / 全屏性能降级 / 告别动画（等美术）。

---

## 6. 关键命令 / 部署

```
npm run tauri dev                                          # 开发（桌面窗口）
npx tauri build --no-bundle                                # release（产物见踩坑#4）
cargo test --manifest-path src-tauri/Cargo.toml --lib      # 库单测（快，无 LLM）
cargo test --test memory_recall       -- --nocapture --test-threads=1   # 闭环1
cargo test --test closed_loop2_harness -- --nocapture --test-threads=1  # 闭环2
cargo test --test soul_harness        -- --nocapture --test-threads=1   # Soul
F12 / Ctrl+Shift+D                                          # Debug Panel（独立窗口，仅 debug）
```

- release exe：`D:\cargo-target\desktop-pet\release\desktop-pet.exe`；桌面快捷方式 `DesktopPet.lnk` 指向它。
- 除 `--lib` 外的 harness 调真实 LLM，需 AppData config 配好 key，慢（reasoning 模型）。
- 完整原始 HANDOFF 日志：`git log --oneline -- docs/HANDOFF.md` → `git show <改写前 commit>:docs/HANDOFF.md`。
