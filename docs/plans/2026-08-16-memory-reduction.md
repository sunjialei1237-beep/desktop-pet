# 后台内存治理方案（2026-08-16）

> 状态：**P1 已实施并实测 ✅（commit `ed5c69d` + `bd61d77`）**。目标：在不影响正常使用（记忆召回质量、响应速度、动画流畅度）的前提下，把后台内存占用降得越低越好。
> 基线实测总占用 ≈ **1.9 GB**（desktop-pet.exe 1500 MB + 其 WebView2 子进程 429 MB）。P1 后实测总占用 **1318 MB**（desktop-pet.exe 870 MB + WebView2 树 449 MB）。设计文档 §6.11 的性能目标是 **内存 < 80 MB**，仍有差距；剩余大头是 WebView2 平台开销与量化模型本身，对应 P2/P3。

## 1. 实测数据（2026-08-16，运行中的 release 桌宠）

| 对象 | 工作集 WS | Private | 说明 |
|---|---|---|---|
| `desktop-pet.exe`（Rust 后端） | **1500 MB** | **1477 MB** | ONNX Runtime 常驻 BGE-M3 fp32 |
| 其 WebView2 子进程 ×7 | **429 MB** | — | Chromium 内核开销（browser/GPU/renderer/utility） |
| **合计** | **≈ 1929 MB** | — | 与用户感知的"1600 左右"吻合（只看主进程即 1500） |
| `desktop_pet.db` + WAL | 2.7 MB | — | 可忽略 |
| 前端 Spine/Pixi 资源 | 1.3 MB（skeleton.png） | — | 可忽略 |

## 2. 根因分析

内存占用分布（按可治理度排序）：

1. **本地 BGE-M3 ONNX 模型（fp32）—— 占总内存约 75-80%，第一大头**
   - 模型文件：`D:\models\bge-m3\model.onnx_data` = **2161.8 MB**（fp32 权重）。
   - `src-tauri/src/embedding/model.rs` 在启动时 `Session::commit_from_file()` 全量加载，且 `EmbeddingService` 一经加载**永不卸载**（`lib.rs` 启动即 `embedding_service.load()`）。
   - ORT 把外部权重读入私有内存 → desktop-pet.exe private 1477 MB。
2. **WebView2 进程组 —— 约 429 MB，第二大头，平台开销**
   - 前端是 React + PixiJS(WebGL) + Spine，资源本身很小；429 MB 是 Chromium 多进程基线。可压缩空间有限，且禁 GPU 会伤 PixiJS 渲染，**本轮不动**。
3. **SQLite / 向量表 / 业务状态 —— 可忽略**
   - DB 0.5 MB；向量按需检索（最多取 50 条）；Working Memory 滑动窗口 ≤40 条。均非问题。

## 3. 方案（分阶段，先做无质量风险的 P1，按需再推进 P2/P3）

### P1（推荐，本轮实施）：BGE-M3 换成 int8 量化版 ONNX
- **做法**：仍用 `Xenova/bge-m3` 官方导出的 `onnx/model_quantized.onnx`（**570 MB**，uint8 量化，单文件，输出仍是 float32、**维度仍 1024**）。同一模型、同一 tokenizer、同一 1024 维 → **不需要改 schema**。
- **⚠️ 修正①：存量向量必须重嵌入**。量化后查询向量（int8 模型算）与存量向量（fp32 模型算）属于混合向量空间——单向量噪声小，但跨空间余弦分布偏移会悄悄影响依赖绝对相似度阈值的行为（serendipity [0.15,0.45] 带、grounding 相关判定）。**做法**：落地时顺手清空 `episode_vectors` 表，由现成的 `backfill_missing_vectors` 全量重嵌入（语料 <100 条，一分钟内完成），彻底消除混合空间风险；并用 `app_config` 记 `embedding_model_key`（`bge-m3-fp32` / `bge-m3-int8`），模型文件切换时自动清向量 + 重嵌入。
- **预期收益**：desktop-pet.exe 从 1500 MB → 预计 **700-900 MB**；总占用从 ~1.9 GB → 预计 **1.1-1.3 GB**。
- **✅ 实测（2026-08-16 release）**：`desktop-pet.exe` WS **870 MB / Private 841 MB**；总占用（含 WebView2 树 449 MB）**1318 MB**。关键调优：ORT builder 用 `GraphOptimizationLevel::Level1` + `with_device_allocated_initializers()` + `with_inter_threads(1)` + `with_parallel_execution(false)` + `with_memory_pattern(false)`；其中 `device_allocated_initializers`（570MB 权重绕过 arena）与 Level1 是最大变量，`memory_pattern/parallel` 单用无效但与 Level1 组合更稳（内存探针对比 6 组配置，见 commit `bd61d77`）。
- **改动点**：
  - `embedding/download.rs`：`REQUIRED_FILES` 改为量化文件集 `model_quantized.onnx / tokenizer.json / config.json / onnxruntime.dll`；`check_complete()` 兼容存量 fp32（任一文件集齐全即 true）；`download_all()` 改为下载量化文件，不再下载 `model.onnx_data`；`quantized_complete()` 供下载命令强制升级用。
  - `embedding/model.rs`：加载时**优先 `model_quantized.onnx`**，不存在再回退 `model.onnx`（老用户无感）；抽一个纯函数 `choose_model_file(dir)` 便于单测；记录本次加载的 `model_key`。
  - `embedding/mod.rs`：`EmbeddingService` 记录 `model_key`，供启动时向量空间对齐。
  - `mind/store.rs`：新增 `reconcile_vectors_for_model(db, model_key)`——`app_config` 里存的 key 与当前模型不一致时，清空 `episode_vectors` 并更新 key（向量由既有 backfill 线程自动补回）。
  - `lib.rs`：启动加载模型后先 `reconcile_vectors_for_model`，再 spawn backfill。
- **验证**：
  - `cargo test --lib`、`cargo check --tests`、`tsc` 全绿。
  - dev 实跑：日志确认加载的是 `model_quantized.onnx`、`embedding_model_key` 已更新、backfill 重嵌入 N 条；Debug Panel 确认模型就绪；发消息验证记忆召回正常。
  - 内存复测：主进程 WS 应下降 ~800 MB 以上。
  - 质量抽检：重嵌入后，取真实 DB 的若干 query 对比 fp32 旧向量与 int8 新向量的 top-5 召回一致性（一次性诊断，不落库）。

### P2（✅ 已实施并实跑验证，2026-08-17）：embedding 模型懒加载 + 空闲卸载
- **做法**：模型不随启动加载，改为首次需要 embedding 时加载；空闲 N 分钟（默认 30，可配）后卸载，下次用时再加载。
- **落地（commit 见 HANDOFF）**：
  - `EmbeddingService` 加生命周期：`with_lazy(lazy_load, idle_unload_minutes)` 构造链、`ensure_loaded()`（双重检查防并发双载）、`unload_if_idle()`（锁下二次校验 last_used 防 mark-then-clone 竞态）、load/unload 计数器（#11）。
  - `embed()/embed_batch()` 懒模式下自动 `ensure_loaded`；`retrieval.rs`/`store.rs` 的 `is_ready()` 前置门全部移除（否则懒加载永远不触发）；`backfill` 改用 `files_present()` 门 + 先查缺再嵌入（无活不加载）。
  - 启动 reconcile 改用免加载的 `embedding::current_model_key(dir)`（文件探测），模型缺席也能对齐向量空间。
  - 独立看护线程（60s tick）执行 `unload_if_idle`，不挂 life-loop（保持 embedding 生命周期自包含）。
  - config `[embedding] lazy_load`（默认 true）/`idle_unload_minutes`（默认 30，0=常驻）；老配置缺键自动启用。
  - Settings 状态显示三态：Ready / Standby (lazy) / Not downloaded + loads/unloads 计数。
- **实跑验证（dev，2026-08-17）**：启动 64MB 不加载 → 用户真实消息触发 `lazy-loaded in 3385ms (load #1)`（对话管线正常继续，延迟被 reasoning 吞掉）→ 空闲 138s（测试阈值 2min）后 `idle-unloaded (unload #1)` → 闲置 WS **84.5MB**。release 重启闲置态 **49.5MB**。
- **⚠️ 修正②（已证实）：锯齿而非稳态**。调度器路径（60min 冒泡窗口的 retrieve）会周期性拉起模型——设计如此，重载在调度线程用户无感。
- **测试**：lib 单测（懒缺文件报错/卸载三重 no-op/访问器）+ `tests/embedding_lifecycle.rs` 真模型全周期（懒启动→首嵌加载→卸载→重载→向量跨周期一致 <1e-5 + current_model_key 与实载 key 一致）。

### P3（备选，质量有取舍）：换更小的 embedding 模型
- 若 P1+P2 后仍想压"加载时"占用，可评估 `bge-base-zh-v1.5`（int8 ~100 MB，768 维）或 `bge-small-zh-v1.5`（int8 ~25 MB，512 维）。
- **⚠️ 修正③：切换成本比原计划低**。`episode_vectors.embedding` 是维度无关的 f32 blob + 暴力余弦搜索（`vectors.rs`），**没有 sqlite-vec 维度约束**；换维度模型 = 改 `EMBEDDING_DIM` 常量 + 清空向量表 + backfill 重嵌入 + 质量 A/B，**没有"改表/DB 迁移"**。仍需要质量评测，但工程成本很低。

### P4（暂不做）：WebView2 内存压缩
- `--renderer-process-limit` / `--disable-gpu` 之类参数对 PixiJS WebGL 渲染有风险（伤动画是伤"活着 Body"），收益有限（上限 ~400 MB），**本轮明确不做**，避免破坏正常使用。

## 4. 验证清单（合入标准）

1. `cargo test --lib` 全绿；`cargo check --tests` 0 error；`tsc` 0 error。
2. dev 实跑：模型加载成功（日志含 quantized 文件路径）；对话、记忆召回、主动气泡、忘记/检索功能正常。
3. 质量抽检：量化前后对同一批 query 的 top-5 episode 召回的 id 一致率 ≥ 95%（或人工确认无"她失忆"体感）。
4. 内存复测：`Get-Process desktop-pet` 的 WS/Private 显著下降；记录 WebView2 子进程合计。
5. release 构建前先 `taskkill /IM desktop-pet.exe /F`（踩坑#6），`npx tauri build --no-bundle`，重启后复测。
6. 更新 `docs/HANDOFF.md`（§当前任务 + §最近一轮）。

## 5. 风险与回滚

| 风险 | 缓解 |
|---|---|
| 量化模型在 ORT 1.20 加载失败（opset/算子不兼容） | 保留 fp32 回退路径（`choose_model_file`）；失败自动 fallback 且日志明确 |
| 量化后召回质量下降 | P1 验证第 3 条 top-5 一致性抽检；不达标则回滚并转 P3 小模型 A/B |
| 下载 570 MB 文件失败/中断 | 复用现有断点跳过逻辑（已存在文件跳过）；支持 hf-mirror |
| P2 懒加载阻塞命令 | P2 单独评审；必须改异步加载并通过"回忆手感"实跑才合入 |
| 老用户已有 fp32 文件 | `check_complete` 兼容 fp32；升级后先按 fp32 跑，用户手动点下载才换量化（或后台自动下） |

## 6. 预期效果汇总

| 阶段 | desktop-pet.exe | 总计（含 WebView2） | 质量风险 |
|---|---|---|---|
| 现状 | 1500 MB | ~1929 MB | — |
| P1 量化 + ORT 调优（✅ 已实测） | **870 MB** | **1318 MB** | 极低（重嵌入后消除混合空间） |
| P1+P2（✅ 已实测） | 锯齿：闲置卸载后 **50-85 MB**，调度器拉起后回到 P1 水平 | 锯齿 ~480-1320 MB | 低（加载延迟 3.4s 实测被 reasoning 吞掉） |
| P1+P2+P3（如需） | 加载时 ~300 MB 级 | ~700 MB 级 | 需评测 |
