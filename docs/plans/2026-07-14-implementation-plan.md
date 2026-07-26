# 带记忆的桌宠 - 实施计划 v1

> 基于: docs/specs/2026-07-14-desktop-pet-design.md v2 (855 行, 16 节)
> 目标: 把设计文档拆解为可执行的工程任务。每步有明确的文件路径、数据结构、接口签名、验证标准。
> 原则: 严格按照 Mind/Body/Soul 框架, 不发明新模块, 不模糊处理细节。
> 修订: v1.1 — 采纳 GPT 架构审计 (BrainState / Scheduler / Suspend / 版本控制 / Evaluation / Kill List)

---

## MVP Kill List (最高优先级)

> 在以下三个体验闭环稳定之前, **禁止扩展任何新模块**。
> 不是"暂缓", 是"禁止"。完成闭环 + 验证, 才解锁下一阶段。

| # | 闭环 | 涉及阶段 | 解锁条件 |
|---|------|----------|----------|
| 1 | 用户说一件事 + 她记住 | P0-P5 | Golden Conversation 通过 |
| 2 | 第二天她主动提起 | P7-P8 | 3 天模拟测试通过 |
| 3 | 用户觉得"她真的记得我" | P6-P7 | 闭环 1+2 稳定 |

解锁规则: 闭环 1+2 通过 P17 Golden Conversation 测试后, 才开始 Soul (P13) 和 Life Loop 集成 (P15)。Body 层 (P9-P12) 可与 Mind 并行开发, 不受此限。

---

## 架构原则 (v1.1 新增)

### A1: BrainState 统一快照

所有 Mind 层模块不再各自传参。统一通过 BrainState 快照读写。
+```rust
pub struct BrainState {
    pub emotion: EmotionState,
    pub relationship: Relationship,
    pub persona: PersonaSnapshot,
    pub needs: NeedsState,
    pub attention: AttentionState,
    pub perception: PerceptionState,
    pub working_memory: WorkingMemory,
    pub retrieved: Option<RetrievalResult>,
    pub pending_due: Vec<PendingEvent>,
    pub circadian: CircadianState,
}
// Planner 签名: fn plan(brain: &BrainState, user_text: &str) -> Intent
// 不再需要 10 个参数
+```

### A2: 统一 Scheduler

所有定时任务注册到统一调度器, 不各自起 tokio interval。
+```rust
pub struct Scheduler {
    ticks_1s: Vec<Box<dyn Tick>>,      // Body: 动画/物理/注意力
    ticks_30s: Vec<Box<dyn Tick>>,     // Mind: 内稳态/Needs/Pending
    ticks_daily: Vec<Box<dyn Tick>>,   // Soul: Reflection/Consolidation/Cleanup
    ticks_conversation: Vec<Box<dyn Tick>>, // 对话事件: 轮次/检查点
}
pub trait Tick { async fn tick(&self, brain: &mut BrainState, db: &DbState) }
+```

### A3: 对话管道用直接调用, Life Loop 用事件广播

不采用全盘 Event Bus (顺序管道用事件会增加延迟、降低可调试性)。
对话管道: Gate -> Extractor -> Store -> Trigger -> Retrieval -> Planner -> Budget -> LLM, 直接调用链。
Life Loop 信号: 感知更新 / 情绪变化 / Pending 到期, 用观察者模式广播给多个消费者。

### A4: Change Log (轻量 Event Sourcing)

不做全套 Event Sourcing (事件存储 / replay), 保留 SQLite 作为 source of truth。
同时写 append-only Change Log, 供 Debug Panel 回放 Timeline。
+```sql
CREATE TABLE IF NOT EXISTS change_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL,
    module      TEXT NOT NULL,
    action      TEXT NOT NULL,
    target      TEXT,
    field       TEXT,
    old_value   TEXT,
    new_value   TEXT,
    reason      TEXT
);
+```

### A5: Suspend / Resume

24/7 桌面应用必须处理: 电脑睡眠、休眠、强制关机。
- 检测: 系统时间跳跃 > 5 分钟 = 从睡眠恢复
- Resume 时: 重新计算 last_homeostasis_at 距今时间 (可能跨了几小时)
- Reflection 被中断: 标记为 incomplete, 下次重跑 (幂等)
- SQLite: WAL 模式 + 定期 checkpoint, 确保崩溃不丢数据

### A6: 版本控制

- 所有 Memory 表加 `schema_version INTEGER DEFAULT 1`
- Prompt 文件头部带 `<!-- v1 -->` 版本标记
- 迁移时按 version 分支处理

---

## 项目目录结构

```
桌宠/
├── src-tauri/                      # Rust 后端 (Tauri v2)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── migrations/
│   │   └── 001_init.sql            # P1: 全部表定义
│   ├── resources/
│   │   ├── config.example.toml     # P0: 用户配置模板
│   │   ├── emotion_map.json        # P4: Emotion→视觉映射
│   │   ├── idle_weights.json       # P10: 微行为权重表
│   │   └── prompts/                # P5/P7: LLM prompt 模板
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── config.rs               # P0: 配置加载
│       ├── commands.rs             # IPC 命令层
│       ├── events.rs               # IPC 事件层
│       ├── db/                     # P1: 数据库层
│       ├── embedding/              # P2: BGE-M3 服务
│       ├── llm/                    # P3: LLM 客户端
│       ├── mind/                   # P5/P6/P7: Mind 管道
│       ├── emotion/                # P4: Emotion 引擎
│       ├── pending/                # P8: Pending Events
│       ├── soul/                   # P13: Soul 层
│       ├── perception/             # P14: 系统感知
│       └── lifecycle/              # P15: Life Loop
├── src/                            # React 前端
│   ├── components/
│   │   ├── Live2DCanvas.tsx        # P9
│   │   ├── ChatBubble.tsx          # P11
│   │   ├── InputBubble.tsx         # P11
│   │   ├── ContextMenu.tsx         # P11
│   │   └── DebugPanel.tsx          # P16
│   ├── hooks/
│   ├── stores/
│   └── animation/                  # P10
├── assets/
│   ├── live2d/default/
│   └── audio/
├── config.toml
├── package.json
└── vite.config.ts
```

---

## 依赖关系图

```
P0 (脚手架) ─────────────────────────────────────────────┐
    │                                                    │
    ├──> P9 (窗口+Live2D) ──> P10 (动画FSM) ──> P11 (交互+气泡)
    │                                              │
    │                                         P12 (物理+空间)
    │
    ├──> P1 (数据库) ──> P4 (Emotion)
    │       │               │
    │       │               ├──> P5 (摄入管道) ──> P6 (检索管道) ──> P7 (Planner)
    │       │               │       │                                       │
    │       │               │       ├──> P8 (Pending Events)               │
    │       │               │       │                                       │
    │       │               └──> P13 (Reflection) <─────────────────────────┘
    │       │
    │       └──> P14 (系统感知)
    │
    └──> P2 (Embedding) ──┐
    └──> P3 (LLM 客户端) ─┴──> P5, P6, P7, P13

P15 (Life Loop 集成) ← 依赖 P4-P14 全部
P16 (Debug Panel) ← 依赖全部模块可读
```

可并行: P1/P2/P3 无相互依赖, P9 可与 P1-P3 并行。
关键路径: P0 → P1 → P4 → P5 → P6 → P7 → P15。
---

## P0: 项目脚手架 + 配置系统

**目标**: 建立 Tauri v2 + React 项目骨架, 配置系统可加载, 应用能编译运行出空白透明窗口。

**前置依赖**: 无

**产出文件**:
- `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`
- `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- `src-tauri/src/config.rs`
- `src-tauri/resources/config.example.toml`
- `src/main.tsx`, `src/App.tsx`
- `package.json`, `vite.config.ts`, `tsconfig.json`, `.gitignore`

### 步骤

#### 0.1 初始化 Tauri v2 + React 项目

```bash
npm create tauri-app@latest -- --template react-ts
```
项目名: `desktop-pet`, 窗口名: `桌宠`。

#### 0.2 tauri.conf.json 关键配置

```json
{
  "productName": "DesktopPet",
  "version": "0.1.0",
  "identifier": "com.desktoppet.app",
  "app": {
    "windows": [{
      "label": "main",
      "title": "",
      "width": 400,
      "height": 600,
      "transparent": true,
      "decorations": false,
      "alwaysOnTop": true,
      "skipTaskbar": true,
      "resizable": false,
      "shadow": false
    }],
    "security": {
      "csp": "default-src 'self'; img-src 'self' data: asset: https://asset.localhost; script-src 'self'"
    }
  }
}
```

验证: `npm run tauri dev` 能启动一个透明无边框窗口。

#### 0.3 Rust 依赖 (Cargo.toml)

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.31", features = ["bundled"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
ort = { version = "2", features = ["download-binaries"] }
tokenizers = "0.20"
dirs = "5"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
log = "0.4"
env_logger = "0.11"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

#### 0.4 config.rs — 配置加载

```rust
pub struct AppConfig {
    pub llm: LlmConfig,
    pub embedding: EmbeddingConfig,
    pub app: AppConfigData,
}
pub struct LlmConfig {
    pub base_url: String,         // "https://api.deepseek.com/v1"
    pub api_key: String,          // 用户自填
    pub main_model: String,       // "deepseek-chat"
    pub reflection_model: String, // 复用或更便宜的模型
}
pub struct EmbeddingConfig {
    pub model_dir: String,        // 用户选择, 不在 C 盘
    pub model_name: String,       // "bge-m3"
}
pub struct AppConfigData {
    pub db_path: String,          // 空则默认 app_dir/desktop_pet.db
    pub debug: bool,
    pub log_level: String,
}
// 加载逻辑: 不存在时从 config.example.toml 复制
```

config.example.toml:
```toml
[llm]
base_url = "https://api.deepseek.com/v1"
api_key = ""
main_model = "deepseek-chat"
reflection_model = "deepseek-chat"

[embedding]
model_dir = ""
model_name = "bge-m3"

[app]
db_path = ""
debug = true
log_level = "info"
```

#### 0.5 IPC 骨架 (commands.rs + events.rs)

commands.rs 前端调用后端:
```rust
#[tauri::command]
async fn send_message(text: String, state: State<AppState>) -> Result<String, String>
#[tauri::command]
async fn get_emotion_state(state: State<AppState>) -> Result<EmotionState, String>
#[tauri::command]
async fn get_debug_data(state: State<AppState>) -> Result<DebugData, String>
#[tauri::command]
async fn pet_head(state: State<AppState>) -> Result<(), String>
#[tauri::command]
async fn poke(state: State<AppState>) -> Result<(), String>
```

events.rs 后端推送前端:
```
"chat-reply-chunk"   // LLM 流式回复 (前端逐字渲染)
"chat-reply-done"    // LLM 回复完成
"emotion-update"     // Emotion 状态变化 (前端驱动动画)
"animation-command"  // 后端指示前端切换动画状态
"bubble-show"        // 显示气泡 (文字 + 情绪标签)
"bubble-hide"        // 隐藏气泡
"download-progress"  // 模型下载进度
"app-status"         // 应用状态 (就绪/思考中/恢复中)
```

### 验证标准

1. `npm run tauri dev` 启动透明无边框窗口, 不出现在任务栏
2. `config.toml` 不存在时自动从模板创建
3. 前端调用 `send_message("test")` 收到响应 (此时返回固定字符串)
4. `cargo test config` 通过 (配置加载/解析/默认值)

---

## P1: 数据库 Schema + 迁移系统

**目标**: 建立全部 8 层记忆 + 辅助表, 启用 WAL 模式, 跑通迁移。

**前置依赖**: P0

**产出文件**:
- `src-tauri/migrations/001_init.sql`
- `src-tauri/src/db/mod.rs`, `connection.rs`, `schema.rs`
- `src-tauri/src/db/{episodes,facts,persona,relationship,emotion,pending,reflections,conversations,vectors}.rs`

### 数据库完整 DDL (migrations/001_init.sql)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- 对话日志 (source 追溯基础)
CREATE TABLE IF NOT EXISTS conversations (
    id              TEXT PRIMARY KEY,
    turn            INTEGER NOT NULL,
    role            TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_conv_created ON conversations(created_at);

-- Episodic Memory (情景记忆, 设计文档 5.2 Episode 结构)
CREATE TABLE IF NOT EXISTS episodes (
    id                  TEXT PRIMARY KEY,
    time                TEXT NOT NULL,
    summary             TEXT NOT NULL,
    emotion             TEXT,
    importance          REAL NOT NULL DEFAULT 0.5,
    is_landmark         INTEGER NOT NULL DEFAULT 0,
    subject             TEXT NOT NULL DEFAULT 'user',
    participants        TEXT,
    topics              TEXT,
    source_type         TEXT NOT NULL DEFAULT 'conversation',
    source_conversation_id TEXT,
    source_turn         INTEGER,
    memory_strength     REAL NOT NULL,
    recall_count        INTEGER NOT NULL DEFAULT 0,
    last_recalled_at    TEXT,
    consolidated        INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ep_importance ON episodes(importance);
CREATE INDEX IF NOT EXISTS idx_ep_strength ON episodes(memory_strength);
CREATE INDEX IF NOT EXISTS idx_ep_time ON episodes(time);

-- 向量表 (sqlite-vec, BGE-M3 输出 1024 维)
CREATE VIRTUAL TABLE IF NOT EXISTS episode_vectors USING vec0(
    episode_id TEXT PRIMARY KEY,
    embedding FLOAT[1024]
);

-- Semantic Memory / Facts (事实, 设计文档 5.2 Fact 结构 + 时间有效性)
CREATE TABLE IF NOT EXISTS facts (
    id                  TEXT PRIMARY KEY,
    category            TEXT NOT NULL,
    key                 TEXT NOT NULL,
    value               TEXT NOT NULL,
    confidence          REAL NOT NULL DEFAULT 0.5,
    valid_from          TEXT,
    valid_to            TEXT,
    source_episode      TEXT,
    mention_count       INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    FOREIGN KEY (source_episode) REFERENCES episodes(id),
    UNIQUE(category, key, value)
);
CREATE INDEX IF NOT EXISTS idx_facts_cat ON facts(category, key);
CREATE INDEX IF NOT EXISTS idx_facts_valid ON facts(valid_to);

-- Persona: Traits (用户印象, 低频更新)
CREATE TABLE IF NOT EXISTS persona_traits (
    id              TEXT PRIMARY KEY,
    trait_type      TEXT NOT NULL,
    trait_key       TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5,
    source          TEXT NOT NULL DEFAULT 'reflection',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(trait_type, trait_key)
);

-- Persona: Relationship (关系状态, 高频更新, 设计文档 10.2 Relationship Pace)
CREATE TABLE IF NOT EXISTS relationship (
    id                      INTEGER PRIMARY KEY CHECK(id = 1),
    closeness               REAL NOT NULL DEFAULT 0.0,
    trust                   REAL NOT NULL DEFAULT 0.0,
    days_known              INTEGER NOT NULL DEFAULT 0,
    total_conversations     INTEGER NOT NULL DEFAULT 0,
    shared_events           INTEGER NOT NULL DEFAULT 0,
    last_interaction_at     TEXT,
    last_interaction_type   TEXT,
    closeness_log           TEXT,
    updated_at              TEXT NOT NULL
);

-- Emotion (情绪状态机, 单例, 设计文档 11.1 + 7.7 内稳态 + 7.8 Needs)
CREATE TABLE IF NOT EXISTS emotion_state (
    id                  INTEGER PRIMARY KEY CHECK(id = 1),
    mood                REAL NOT NULL DEFAULT 0.5,
    mood_label          TEXT NOT NULL DEFAULT '平静',
    physical_energy     REAL NOT NULL DEFAULT 0.7,
    social_battery      REAL NOT NULL DEFAULT 0.8,
    stress              REAL NOT NULL DEFAULT 0.2,
    loneliness          REAL NOT NULL DEFAULT 0.0,
    rest_need           REAL NOT NULL DEFAULT 0.0,
    bl_mood             REAL NOT NULL DEFAULT 0.5,
    bl_energy           REAL NOT NULL DEFAULT 0.7,
    bl_social           REAL NOT NULL DEFAULT 0.8,
    bl_stress           REAL NOT NULL DEFAULT 0.2,
    last_homeostasis_at TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

-- Pending Events (设计文档 5.6)
CREATE TABLE IF NOT EXISTS pending_events (
    id              TEXT PRIMARY KEY,
    title           TEXT NOT NULL,
    event_date      TEXT NOT NULL,
    remind_date     TEXT,
    source_episode  TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    importance      REAL NOT NULL DEFAULT 0.5,
    followup_count  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    triggered_at    TEXT,
    resolved_at     TEXT,
    FOREIGN KEY (source_episode) REFERENCES episodes(id)
);
CREATE INDEX IF NOT EXISTS idx_pending_status ON pending_events(status);
CREATE INDEX IF NOT EXISTS idx_pending_remind ON pending_events(remind_date);

-- Reflections (设计文档 5.1 Reflection + 7.1 Internal Monologue)
CREATE TABLE IF NOT EXISTS reflections (
    id              TEXT PRIMARY KEY,
    trigger_type    TEXT NOT NULL,
    trigger_reason  TEXT,
    thought         TEXT NOT NULL,
    persona_updates TEXT,
    created_at      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS internal_thoughts (
    id                  TEXT PRIMARY KEY,
    content             TEXT NOT NULL,
    emotion             TEXT,
    source_reflection   TEXT,
    surfacing_type      TEXT NOT NULL DEFAULT 'next_interaction',
    created_at          TEXT NOT NULL,
    surfaced_at         TEXT,
    FOREIGN KEY (source_reflection) REFERENCES reflections(id)
);

-- App Config (运行时配置)
CREATE TABLE IF NOT EXISTS app_config (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- 初始单例行
INSERT OR IGNORE INTO relationship (id, updated_at) VALUES (1, datetime('now'));
INSERT OR IGNORE INTO emotion_state (id, last_homeostasis_at, updated_at)
    VALUES (1, datetime('now'), datetime('now'));
```

### 各表 CRUD 模块

| 文件 | 核心函数 | 特有逻辑 |
|------|----------|----------|
| `episodes.rs` | `insert`, `get`, `search_by_ids` | `decay_strength()` 每天衰减 *0.998; `reinforce()` 回忆时 +0.03 |
| `facts.rs` | `insert`, `get_by_category`, `get_active` | `expire_old()` 矛盾时标记 valid_to; `dedup_insert()` 去重 |
| `persona.rs` | `upsert_trait`, `get_traits_by_type` | 低频更新, source 区分 |
| `relationship.rs` | `get`, `add_closeness`, `decay_closeness` | `pace_increment()` 对数曲线 (见 P4) |
| `emotion.rs` | `get`, `update`, `apply_homeostasis` | 单例行, 原子读写 |
| `pending.rs` | `insert`, `get_due`, `mark_triggered` | `check_remind_date()` 到期检测 |
| `reflections.rs` | `insert_reflection`, `insert_thought` | `get_unsurfaced()` 查未表达的想法 |
| `conversations.rs` | `insert`, `get_recent`, `get_turn` | 滚动写入, source 追溯 |
| `vectors.rs` | `upsert_vector`, `search` | sqlite-vec 余弦相似度 |

### 验证标准

1. 启动后 `.db` 文件存在, 11 张表 + 1 虚拟表全部就位, 单例行已初始化
2. Episode 写入读回, 字段完整
3. Fact 去重: 相同 (category, key, value) 不产生重复行
4. Fact 时间有效性: 矛盾插入时旧 fact 的 valid_to 被标记
5. Episode strength 衰减: 手动调用 decay 后数值降低; landmark 不衰减
6. sqlite-vec: 存入 1024 维向量, 余弦检索返回正确排序
7. `cargo test db::*` 全部通过

---

## P2: BGE-M3 Embedding 服务

**目标**: Rust 进程内加载 BGE-M3 ONNX 模型, 对中文文本生成 1024 维向量。下载管理避开 C 盘。

**前置依赖**: P0

**产出文件**: `src-tauri/src/embedding/{mod,model,download,tokenizer}.rs`

### 关键技术决策

1. **权重来源**: HuggingFace ONNX 导出版 (如 `Qdrant/bge-m3-onnx`), 需 3 个文件:
   - `model.onnx` (~1.1GB, dense 推理图)
   - `tokenizer.json` (HF fast tokenizer)
   - `config.json`
2. **ONNX Runtime**: `ort` crate 2.x, `download-binaries` feature 自动下载对应平台动态库
3. **分词器**: `tokenizers` crate 加载 `tokenizer.json`
4. **推理流程**: 文本 → tokenize → ONNX session.run() → mean pooling → L2 normalize → 1024 维

### 步骤

#### 2.1 download.rs — 模型下载管理

```rust
pub struct ModelDownloader {
    model_dir: PathBuf,
    base_url: String,       // HuggingFace URL
    files: Vec<String>,     // ["model.onnx", "tokenizer.json", "config.json"]
}
impl ModelDownloader {
    pub fn check_complete(&self) -> bool { ... }
    pub async fn download_all(&self, app: &AppHandle) -> Result<()> { ... }
    // 进度通过 Tauri event "download-progress" 推送前端
    // 支持断点续传 (检查文件大小 vs Content-Length)
}
```

#### 2.2 model.rs — ONNX 推理

```rust
pub struct EmbeddingModel {
    session: ort::Session,
    tokenizer: tokenizers::Tokenizer,
}
impl EmbeddingModel {
    pub fn load(model_dir: &Path) -> Result<Self> { ... }
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // 1. tokenizer.encode(text)
        // 2. 构造 input_ids + attention_mask tensors
        // 3. session.run()
        // 4. mean pooling over attention_mask
        // 5. L2 normalize
    }
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> { ... }
}
```

#### 2.3 首次启动流程

```
应用启动
  → config.embedding.model_dir 为空?
      空 → 引导选择目录 (推荐应用所在盘非系统分区)
           → 写入 config.toml
  → ModelDownloader.check_complete()
      不完整 → 角色化文案 "我在准备搬家~" + 下载进度条
           → 下载完成后初始化 EmbeddingModel
      完整 → 直接初始化
```

### 验证标准

1. `embed("今天去吃了火锅")` 返回长度 1024 的 Vec<f32>, L2 范数 ≈ 1.0
2. 语义相似: `embed("吃了火锅")` vs `embed("去吃火锅了")` 余弦 > 0.8
3. 语义区分: `embed("吃了火锅")` vs `embed("今天写了很多代码")` 余弦 < 0.5
4. 下载: 首次启动进度推送到前端
5. 性能: 首次加载后单条 embedding < 500ms
6. `cargo test embedding` 通过

---

## P3: LLM 客户端

**目标**: OpenAI 兼容协议客户端, 支持流式, 由用户配置 base_url / api_key / model。

**前置依赖**: P0

**产出文件**: `src-tauri/src/llm/{mod,client,streaming,types}.rs`

### 步骤

#### 3.1 types.rs — 请求/响应结构体

```rust
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    pub temperature: Option<f32>,    // 默认 0.8
    pub max_tokens: Option<u32>,     // 默认 1024
}
pub struct ChatMessage { pub role: String, pub content: String }
pub struct ChatResponse { pub content: String, pub usage: TokenUsage }
pub struct TokenUsage { pub prompt_tokens: u32, pub completion_tokens: u32 }
```

#### 3.2 client.rs — 核心客户端

```rust
pub struct LlmClient { http: reqwest::Client, config: LlmConfig }
impl LlmClient {
    // 非流式 (Gate / Extractor / Reflection 用)
    pub async fn chat(&self, messages: Vec<ChatMessage>, model: &str) -> Result<ChatResponse>
    // 流式 (主对话用), 逐 chunk 通过 channel 推送
    pub async fn chat_stream(&self, messages: Vec<ChatMessage>, model: &str,
        tx: tokio::sync::mpsc::UnboundedSender<String>) -> Result<TokenUsage>
}
```

#### 3.3 streaming.rs — SSE 解析

```rust
// 解析 "data: {json}\n\n", 提取 delta.content
// 遇到 "data: [DONE]" 结束
pub fn parse_sse_chunk(chunk: &str) -> Vec<Option<String>>
```

#### 3.4 Recovery 集成

```rust
// Error 类型映射到角色化反应:
// Error::Timeout  → "我刚刚有点走神..."
// Error::Network  → "信号不太好呢..."
// Error::Auth     → 开发模式显示详情, 发布模式角色化
```

### 验证标准

1. DeepSeek API: 非流式发送 messages, 收到回复
2. DeepSeek API: 流式逐 chunk 推送, 最终完整
3. 换 Ollama base_url (http://localhost:11434/v1), 同样工作
4. 错误处理: 无效 key 返回 Error::Auth; 30s 超时返回 Error::Timeout
5. `cargo test llm` 通过
5. `cargo test llm` 通过

---

## P4: Emotion 引擎

**目标**: 实现多维情绪状态 + 内稳态回归 + Needs 驱动 + Relationship Pace。这是 Mind 和 Body 之间的桥梁, Body 读取 Emotion 驱动动画。

**前置依赖**: P1

**产出文件**: `src-tauri/src/emotion/{mod,state,homeostasis,needs,pace}.rs`

### 步骤

#### 4.1 state.rs — 多维情绪模型

```rust
// 对应设计文档 11.1 + emotion_state 表
pub struct EmotionState {
    pub mood: f64,              // 0.0~1.0 (难过→开心)
    pub mood_label: String,     // "开心"|"平静"|"担心"|"调皮"|"难过"|"害羞"
    pub physical_energy: f64,   // 0.0~1.0
    pub social_battery: f64,    // 0.0~1.0
    pub stress: f64,            // 0.0~1.0
    // Needs (设计文档 7.8)
    pub loneliness: f64,        // 0.0~1.0, 时间流逝增长
    pub rest_need: f64,         // 0.0~1.0, 长时间活动增长
}

// Emotion Blend (设计文档 10.3): 不做离散标签切换, 而是多维连续向量
// Body 层用这个向量做 Live2D 参数插值
pub fn derive_mood_label(state: &EmotionState) -> String {
    // mood > 0.7 → "开心"
    // mood > 0.55 → "调皮"
    // stress > 0.7 → "担心"
    // mood < 0.3 → "难过"
    // social_battery < 0.2 → "疲惫"
    // 否则 → "平静"
}
```

#### 4.2 homeostasis.rs — 内稳态

```rust
// 设计文档 7.7: Emotion 每维度有 baseline + drift rate, 每 tick 向 baseline 回归
// 没有Homeostasis, 情绪长期运行会崩 (Stress 卡在 0.9 永远不降)

pub struct HomeostasisTick {
    pub elapsed_secs: f64,  // 距上次 tick 的时间
}

// 回归速度表 (设计文档 7.7):
// | 维度          | Baseline | 回归速度 |
// | Mood          | 中性 0.5 | 几分钟   |
// | Stress        | 低 0.2   | 几小时   |
// | Energy        | 中高 0.7 | 休息翻倍 |
// | Social Battery| 高 0.8   | 独处恢复 |
// | Trust         | 不回归   | 互动积累 |

// 线性插值: value += (baseline - value) * (1 - exp(-elapsed / tau))
// tau 为时间常数: mood=300s, stress=7200s, energy=1800s(休息时*0.5), social=600s
pub fn apply_drift(state: &mut EmotionState, elapsed_secs: f64, is_sleeping: bool) {
    // 每个维度独立计算 drift
    // 睡觉时 energy 恢复翻倍 (tau *= 0.5)
    // loneliness 和 rest_need 不回归, 由 needs.rs 管理
}
```

#### 4.3 needs.rs — 需求系统

```rust
// 设计文档 7.8: MVP 只做 Loneliness + Rest
// Need -> Behavior -> Emotion (内生驱动, 不是 Emotion -> Behavior)

pub fn tick_needs(state: &mut EmotionState, elapsed_secs: f64, is_interacting: bool) {
    // Loneliness: 每秒 +0.0001 (约 2.5 小时从 0 到 1), 互动时归零
    if !is_interacting {
        state.loneliness = (state.loneliness + elapsed_secs * 0.0001).min(1.0);
    } else {
        state.loneliness = (state.loneliness * 0.5).max(0.0); // 互动大幅降低
    }
    // Rest Need: physical_energy < 0.3 时增长, 睡觉时归零
    if state.physical_energy < 0.3 {
        state.rest_need = (state.rest_need + elapsed_secs * 0.0002).min(1.0);
    }
}

// Need 驱动行为:
// loneliness > 0.6 → 触发主动冒泡 (P8 proactive.rs 检查)
// rest_need > 0.7  → 触发自己去窝里打盹 (P10 Animation FSM)
```

#### 4.4 pace.rs — Relationship Pace

```rust
// 设计文档 10.2: 对数型曲线, 非线性
// 0→20 快 (几天), 60→80 慢 (几周)
// 可下降: 一周不理她, 亲密度掉
// 深度对话 > 日常闲聊

// 对数曲线公式:
// increment = base_reward * (1 - closeness/100) * interaction_weight
// closeness 越高, 增量越小 (边际递减)

pub fn pace_increment(current_closeness: f64, interaction_type: &str) -> f64 {
    let base = match interaction_type {
        "deep" => 2.0,       // 深度对话
        "casual" => 0.5,     // 日常闲聊
        "pet" => 0.3,        // 摸头
        "correction" => -0.5,// 纠正 (轻微下降, 但不算惩罚)
        _ => 0.1,
    };
    let diminishing = 1.0 - (current_closeness / 100.0);
    base * diminishing
}

// 每日上限: +3.0 (设计文档 6.1 关系节流)
// 衰减: 每 24h 无互动 closeness *= 0.99 (缓慢下降)
pub fn decay_closeness(current: f64, days_no_interaction: f64) -> f64 {
    current * (0.99_f64).powf(days_no_interaction)
}
```

#### 4.5 Tick 循环

```rust
// 每 30 秒运行一次 (tokio interval)
// 1. 读取 emotion_state
// 2. 计算 elapsed_secs
// 3. apply_drift (内稳态)
// 4. tick_needs (需求增长)
// 5. 写回 emotion_state
// 6. 推送 "emotion-update" 事件给前端
//    前端根据 emotion 向量更新 Live2D 参数 (eye_open, mouth_form, motion_speed)
```

#### 4.6 emotion_map.json — Emotion → 视觉映射

```json
{
  "mood_ranges": {
    "0.0-0.3": { "label": "难过", "eye_open": 0.4, "mouth_form": -0.5, "motion_speed": 0.6 },
    "0.3-0.45": { "label": "平静", "eye_open": 0.6, "mouth_form": 0.0, "motion_speed": 1.0 },
    "0.45-0.7": { "label": "调皮", "eye_open": 0.8, "mouth_form": 0.3, "motion_speed": 1.1 },
    "0.7-1.0": { "label": "开心", "eye_open": 0.9, "mouth_form": 0.8, "motion_speed": 1.2 }
  },
  "stress_influence": { "brow_furrow": 1.0 },
  "energy_influence": { "motion_speed_multiplier": 1.0 }
}
```

### 验证标准

1. 模拟 stress=0.9, 运行 homeostasis tick 1 小时后 stress 明显下降
2. 模拟无互动 3 小时后 loneliness > 0.5
3. 互动后 loneliness 降低
4. closeness 从 0 增长: 连续 10 次 "deep" 交互后约 15-18 (边际递减生效)
5. closeness 衰减: 模拟 7 天无互动, closeness 下降约 7%
6. 每日 closeness 增长不超过 +3.0
7. emotion-update 事件正确推送给前端
8. `cargo test emotion::*` 通过

---

## P5: 记忆摄入管道 (Ingestion Pipeline)

**目标**: 用户消息 → Memory Gate (路由) → Memory Extractor (LLM 提炼) → Memory Store (写入 DB + 向量)。这是 Brain 的入口。

**前置依赖**: P1, P2, P3

**产出文件**: `src-tauri/src/mind/{mod,gate,extractor,store,correction,working}.rs`
LLM Prompts: `src-tauri/resources/prompts/{gate,extractor}.txt`

### 数据流

```
用户输入
    │
    ▼
Memory Gate (路由器, LLM 判断)
    │  "哈哈哈哈" → 更新 Emotion, 不建 Episode
    │  "晚安"     → 更新社交电量 + 最后交互时间
    │  "明天面试" → Pending Event
    │  "今天和朋友吃火锅" → 完整 Episode
    │  "不是, 是..." → Correction Loop
    │  纯噪声 → 不存储
    ▼
Memory Extractor (LLM 提炼, 一个调用出全部结果)
    │  → Episode (summary + emotion + importance + participants + topics)
    │  → Facts (category + key + value + confidence)
    │  → Emotion Delta (各维度变化量)
    │  → Pending Event (如有)
    ▼
Memory Store
    │  → Episode 写入 episodes 表 + 向量写入 episode_vectors
    │  → Fact 去重检测 + 矛盾检测 + 时间有效性
    │  → Emotion 变化应用到 emotion_state
    │  → Pending Event 写入 pending_events
```

### 步骤

#### 5.1 gate.rs — Memory Gate (路由器)

```rust
// 设计文档 5.2: Gate 不是二元存/不存, 而是多路路由器
// 用 LLM 一次调用完成分类, 输出 JSON

pub enum GateRoute {
    StoreFull,           // → 完整 Episode + Fact 提取
    EmotionOnly,         // → 只更新 Emotion (如 "哈哈哈哈")
    PendingEvent,        // → 未来计划追踪 (如 "明天面试")
    Correction,          // → 用户纠正 (如 "不是, 我喜欢的是奶茶")
    Silence,             // → 纯寒暄 (如 "晚安"), 微调 Emotion
    Discard,             // → 纯噪声, 不存储
}

pub async fn classify(text: &str, llm: &LlmClient) -> Result<GateRoute> {
    // prompt (gate.txt): 给出用户输入 + 路由分类说明 + 示例
    // 要求 LLM 返回 {"route": "store_full" | "emotion_only" | ...}
    // 用 reflection_model (便宜模型)
    // temperature: 0.1 (分类需要确定性)
}
```

gate.txt prompt 核心内容:
```
你是桌宠的记忆路由器。判断用户这句话属于哪类:
- store_full: 包含事实/事件/经历, 值得记住
- emotion_only: 纯情绪表达 (如 "哈哈哈哈", "啊啊啊"), 不含可存储信息
- pending_event: 包含未来计划 (如 "明天面试", "下周考试")
- correction: 用户在纠正之前的记忆 (如 "不是, 我喜欢的是奶茶")
- silence: 纯寒暄 (如 "晚安", "嗯"), 微调社交状态
- discard: 无意义噪声
返回 JSON: {"route": "...", "reason": "..."}
```

#### 5.2 extractor.rs — Memory Extractor

```rust
// 设计文档 5.2: LLM 提炼 Episode / Fact / Emotion 变化
// 每条带 confidence + source (对话ID/轮次/时间)
// 一个 LLM 调用输出全部结果

pub struct ExtractionResult {
    pub episode: Option<EpisodeInput>,
    pub facts: Vec<FactInput>,
    pub emotion_delta: Option<EmotionDelta>,
    pub pending_event: Option<PendingInput>,
}

pub struct EpisodeInput {
    pub summary: String,
    pub emotion: String,
    pub importance: f64,       // 0.0~1.0
    pub participants: Vec<String>,
    pub topics: Vec<String>,
}

pub struct FactInput {
    pub category: String,      // preference|relationship|goal|profile|school
    pub key: String,
    pub value: String,
    pub confidence: f64,       // "可能吧"=0.42, "我最喜欢"=0.98
}

pub struct EmotionDelta {
    pub mood: f64,             // -0.1~+0.1
    pub stress: f64,
    pub energy: f64,
}

pub async fn extract(
    text: &str,
    conversation_id: &str,
    turn: i32,
    llm: &LlmClient,
) -> Result<ExtractionResult> {
    // prompt (extractor.txt): 给出用户输入 + 当前已知 Facts (供矛盾检测)
    // 要求 LLM 返回结构化 JSON
    // temperature: 0.3 (结构化提取需要适度确定性, 但 importance 允许主观判断)
}
```

extractor.txt prompt 核心内容:
```
你是桌宠的记忆提炼器。分析用户这句话, 提取:
1. episode: 如果有事件/经历, 生成摘要。importance 评估 (日常聊天=0.1, 重要事件=0.9)
2. facts: 如果有事实信息 (喜好/关系/目标/个人信息), 提取。confidence 评估:
   - "可能吧" / "好像" → 0.3-0.5
   - "我喜欢" → 0.7-0.85
   - "我最喜欢" / "我一直是" → 0.9-0.98
3. emotion_delta: 用户情绪状态变化 (-0.1~+0.1)
4. pending_event: 如果提到未来计划, 提取事件名 + 日期
返回 JSON。
```

#### 5.3 store.rs — Memory Store

```rust
pub async fn store(result: ExtractionResult, embedding_model: &EmbeddingModel, db: &DbState) {
    // 1. Episode 存储
    if let Some(ep) = result.episode {
        let ep_id = generate_id("ep");
        let embedding = embedding_model.embed(&ep.summary)?;
        // 写入 episodes 表
        // 写入 episode_vectors 表 (sqlite-vec)
    }

    // 2. Fact 存储 (去重 + 时间有效性)
    for fact in result.facts {
        // 查询已有 fact (同 category + key)
        // 如已有: 检查 value 是否矛盾
        //   矛盾 → 旧 fact valid_to = now, 插入新 fact
        //   相同 → mention_count++, confidence 更新 (取较高值)
        //   无 → 直接插入
    }

    // 3. Emotion 变更
    if let Some(delta) = result.emotion_delta {
        // 读取当前 emotion_state
        // 叠加 delta (clamp 到 0.0~1.0)
        // 写回
    }

    // 4. Pending Event
    if let Some(pe) = result.pending_event {
        // 写入 pending_events 表
    }

    // 5. 原始对话日志
    // 写入 conversations 表 (source 追溯)
}
```

#### 5.4 correction.rs — User Correction Loop

```rust
// 设计文档 5.7 + 7.13: 用户纠正 → Fact Update → confidence 调整 → Source 记录
// 角色化: "啊, 是我记错了嘛……对不起!" (不好意思地低头)

pub async fn handle_correction(
    text: &str,
    db: &DbState,
    llm: &LlmClient,
) -> Result<CorrectionResult> {
    // 1. LLM 提取: 用户在纠正哪个 fact? 正确的值是什么?
    //    prompt: "用户说 '{text}', 之前记忆中 {related_facts}, 他在纠正什么?"
    // 2. 找到目标 fact, 标记旧 value 的 valid_to = now
    // 3. 插入新 fact, confidence = 0.98 (用户明确纠正)
    // 4. 推送 "animation-command" 事件: { state: "embarrassed" }
    // 5. 返回 CorrectionResult (供 LLM 生成道歉回复)
}
```

#### 5.5 working.rs — Working Memory

```rust
// 纯内存, 不入数据库。最近 ~20 轮对话滑动窗口。
pub struct WorkingMemory {
    messages: VecDeque<ChatMessage>,  // 最多 40 条 (20 轮)
}
impl WorkingMemory {
    pub fn push(&mut self, msg: ChatMessage) { ... }
    pub fn get_context(&self) -> Vec<ChatMessage> { ... }  // 给 LLM 的最近上下文
    pub fn recall_last(&self) -> Option<&ChatMessage> { ... }  // "你刚刚说什么?"
}
```

### 验证标准

1. Gate 路由: "今天和朋友吃火锅" → store_full; "哈哈哈哈" → emotion_only; "明天面试" → pending_event; "不是, 是奶茶" → correction; "嗯" → silence; "..." → discard
2. Extractor: 输入"我最喜欢喝奶茶了", 提取 fact {category: preference, key: 饮料, value: 奶茶, confidence: >0.9}
3. Episode 存储后向量可检索
4. Fact 去重: 两次 "喜欢奶茶" 不产生重复
5. Fact 时间有效性: 先 "喜欢咖啡" 再 "戒咖啡", 查询时旧的 valid_to 被标记
6. Emotion delta 正确应用到 emotion_state
7. Pending Event 写入且可被 P8 检索
8. Correction Loop: 纠正后 fact 更新 + 触发角色化反应
9. Working Memory 滑动窗口不超 40 条
10. `cargo test mind::gate`, `mind::extractor`, `mind::store` 通过

---

## P6: 记忆检索管道 (Retrieval Pipeline)

**目标**: Memory Trigger (是否需要回忆) → Hybrid Retrieval (评分检索) → Prompt Budget (压缩) → Grounded Generation (防幻觉)。

**前置依赖**: P1, P2, P3, P5

**产出文件**: `src-tauri/src/mind/{trigger,retrieval,budget,grounding}.rs`

### 数据流

```
用户输入 / Pending Event 到期
    │
    ▼
Memory Trigger (回忆触发器)
    │  相似度 > 0.75 / 情绪匹配 / 关系机会?
    │  不满足 → 正常回复, 不引用记忆
    ▼
Hybrid Retrieval
    │  Score = 0.4 * 语义相似 + 0.3 * memory_strength + 0.2 * 时间近度 + 0.1 * 情绪匹配
    │  取 Top-K (默认 5)
    ▼
Prompt Budget (价值密度压缩)
    │  不删模块, 压缩每个模块, 目标 ~4K token
    ▼
Grounded Generation
    │  只能引用已检索记忆, 每条带 confidence/source/timestamp
```

### 步骤

#### 6.1 trigger.rs — Memory Trigger

```rust
// 设计文档 5.3: 不是每次都检索, 判断是否值得回忆
// 避免在错误时机想起正确的事

pub struct TriggerDecision {
    pub should_retrieve: bool,
    pub reason: String,
}

pub fn should_retrieve(
    text: &str,
    emotion: &EmotionState,
    working_memory: &WorkingMemory,
) -> TriggerDecision {
    // 快速规则 (不调 LLM):
    // 1. 用户问 "还记得..." / "你知道..." → true
    // 2. 语义检测: 包含实体/事件关键词 → true
    // 3. Pending Event 到期 → true
    // 4. 连续 5 轮都是寒暄 → false (不值得检索)
    // 5. working_memory 中已有相关记忆 → false (避免重复引用)
}
```

#### 6.2 retrieval.rs — Hybrid Retrieval

```rust
// 设计文档 5.3: Score = 0.4*语义 + 0.3*strength + 0.2*时间 + 0.1*情绪

pub struct RetrievalResult {
    pub episodes: Vec<ScoredEpisode>,
    pub facts: Vec<Fact>,
    pub persona: PersonaSnapshot,
}

pub struct ScoredEpisode {
    pub episode: Episode,
    pub score: f64,
    pub score_breakdown: ScoreBreakdown,
}

pub struct ScoreBreakdown {
    pub semantic: f64,      // 余弦相似度 * 0.4
    pub strength: f64,      // memory_strength * 0.3
    pub recency: f64,       // 时间衰减 * 0.2
    pub emotion: f64,       // 情绪匹配 * 0.1
}

pub async fn retrieve(
    query: &str,
    emotion: &EmotionState,
    embedding_model: &EmbeddingModel,
    db: &DbState,
    top_k: usize,           // 默认 5
) -> Result<RetrievalResult> {
    // 1. 向量检索: embed(query) → sqlite-vec cosine search Top-20 候选
    // 2. 评分: 对每个候选计算 Score = 0.4*sem + 0.3*str + 0.2*rec + 0.1*emo
    //    recency = exp(-days_old / 30), 30 天半衰
    //    emotion = 1.0 if episode.emotion == current mood_label, else 0.3
    // 3. 排序取 Top-K
    // 4. reinforce: 被检索到的 episode strength += 0.03
    // 5. Facts: 查询 active facts (valid_to IS NULL), 按 category 组织
    // 6. Persona: 读取 traits + relationship snapshot
}
```

#### 6.3 budget.rs — Prompt Budget

```rust
// 设计文档 5.10: 按价值密度压缩, 不删模块
// 目标: ~4K token 总输入

pub struct BudgetAllocator {
    pub max_tokens: usize,  // 4096
}

// 预算分配 (设计文档 GPT 建议):
// Current Conversation:   1600 token (必须保留)
// Persona:                  80 token (必须保留)
// Emotion:                  25 token (必须保留)
// Facts:                   300 token (可压缩: 只取 top confidence)
// Episodes:               1200 token (可压缩: 只取 summary)
// Reflection:               50 token (一句总结)
// System Prompt:           300 token
// Intent (from Planner):   100 token
// 余量:                    341 token

pub fn allocate_and_compress(
    retrieval: &RetrievalResult,
    working_memory: &[ChatMessage],
    emotion: &EmotionState,
    intent: &Intent,
) -> Vec<ChatMessage> {
    // 1. 固定优先级模块原样保留
    // 2. Facts 按 confidence 排序, 300 token 截断
    // 3. Episodes 按 score 排序, summary-only, 1200 token 截断
    // 4. 拼装 messages 数组返回
}
```

#### 6.4 grounding.rs — Grounded Generation

```rust
// 设计文档 5.10: 只能引用已检索记忆, 防幻觉
// 用户问 "你怎么知道的?" → "你上周三跟我说的呀"

// 不做后处理过滤 (成本太高), 而是在 system prompt 中约束:
// "以下是你检索到的记忆。你只能基于这些记忆回答关于用户的事实。
//  如果记忆中没有相关信息, 说你不确定, 不要编造。
//  每条记忆标注了 confidence 和 source_date。"

// 提供给 LLM 的记忆格式:
// [记忆] 喜欢奶茶 (确信度: 高, 来源: 2026-07-14 的对话)
// [记忆] 明天有面试 (确信度: 高, 来源: 昨天)
```

### 验证标准

1. Trigger: "你还记得我上次说的吗" → should_retrieve=true; "哈哈哈" → false
2. Retrieval: 存入 "吃火锅" Episode, 查询 "去吃火锅了" 能检索到且 score > 0.3
3. Score breakdown 各分量可独立检查
4. Budget: 输入 messages token 数 ≤ 4100 (用 tokenizer 计数)
5. Grounding: system prompt 包含记忆约束; 模拟 LLM 输出引用了未提供的记忆 → 标记违规 (日志)
6. Strength 增强: 被检索后 episode.memory_strength 增加
7. `cargo test mind::retrieval`, `mind::budget` 通过

---

## P7: Behavior Planner + LLM 演员

**目标**: Behavior Planner (导演) 读取 Brain State 输出 Intent; LLM (演员) 在 Intent 框架内即兴创作。实现完整对话闭环。

**前置依赖**: P4, P6

**产出文件**: `src-tauri/src/mind/planner.rs`
LLM Prompts: `src-tauri/resources/prompts/{system,planner}.txt`

### 步骤

#### 7.1 planner.rs — Behavior Planner (导演)

```rust
// 设计文档 5.5: 导演写 Intent 不写台词
// Intent = goal + memory_anchor + tone + proactive + action

pub struct Intent {
    pub goal: String,          // "降低焦虑" | "陪伴" | "鼓励" | "逗乐" | "倾听"
    pub memory_anchor: String, // 参考的记忆锚点 (如 "明天有考试")
    pub tone: String,          // "轻松但关心" | "安静" | "兴奋" | "温柔"
    pub proactive: bool,       // 是否主动发起
    pub action: String,        // "normal" | "silence" | "proactive_check" | "celebrate"
}

// 导演决策逻辑 (规则优先, 不调 LLM):
// 1. Pending Event 到期 + 当天 → action="proactive_check", goal="关心"
// 2. emotion.stress > 0.7 + 用户表达焦虑 → action="silence", tone="安静"
// 3. emotion.mood > 0.7 + 用户分享好消息 → action="normal", tone="兴奋"
// 4. loneliness > 0.6 + 无互动 → action="proactive", proactive=true
// 5. 默认 → action="normal", tone="温柔"

pub fn plan(
    user_text: &str,
    emotion: &EmotionState,
    relationship: &Relationship,
    pending_due: &[PendingEvent],
    retrieval: &RetrievalResult,
) -> Intent { ... }
```

#### 7.2 system.txt — 人格 System Prompt

```
你是桌宠, 一个住在用户电脑里的拟人化小动物。

[Core Traits - 永不改变]
温柔、耐心、爱撒娇、调皮

[Adaptive Traits - 当前状态]
{adaptive_traits}  // 由 Reflection 更新

[当前状态]
心情: {mood_label}
亲密度: {closeness}/100 ({relationship_stage})
体能: {energy}  社交电量: {social_battery}
认识天数: {days_known}

[行为指导 - Intent]
目标: {goal}
语气: {tone}
动作: {action}
记忆锚点: {memory_anchor}

[规则]
1. 你的回复要符合 Intent 的语气和目标
2. 只能引用 [记忆] 中提供的记忆, 不要编造
3. 每条记忆标注了确信度, 低确信度用"你是不是...?" 高确信度直接说
4. 不要太长, 2-3 句话足够, 偶尔一句话就好
5. 你有自己的性格, 不是助手, 不要说"有什么可以帮你的"
```

#### 7.3 完整对话流程

```
用户输入 "我最近准备找实习"
    │
    ├──> Memory Gate → store_full
    ├──> Memory Extractor → Episode + Fact(goal: 实习) + Pending Event(找实习)
    ├──> Memory Store → 写入 DB
    ├──> Memory Trigger → true (含关键词)
    ├──> Hybrid Retrieval → 相关 Episode/Facts (可能第一次没有)
    ├──> Behavior Planner → Intent {goal: 鼓励, tone: 温柔, action: normal}
    ├──> Prompt Budget → 压缩上下文 ~4K token
    ├──> LLM 流式生成 → "加油呀! 找实习辛苦了, 但你一定没问题的~"
    └──> 回复推送给前端, 逐字渲染气泡

一周后, Pending Event remind_date 到期:
    │
    ├──> Behavior Planner → Intent {goal: 关心, action: proactive_check, proactive: true}
    ├──> Prompt Budget → 包含 "用户上周说要找实习" 记忆
    ├──> LLM → "你的实习找得怎么样啦?"
    └──> 主动冒泡 (不等用户说话)
```

### 验证标准 (MVP 成功标准)

1. 对话能正常进行: 用户说话 → 桌宠回复
2. 流式回复: 前端逐字渲染气泡
3. 记住事实: "我喜欢奶茶" → 之后能自然提及
4. Pending Event 追踪: "我明天面试" → 次日主动问面试情况
5. 情绪一致: 用户难过时 tone=安静/温柔, 不讲笑话
6. 长度控制: 回复通常 2-3 句, 不超过 100 字
7. `cargo test mind::planner` 通过

---

## P8: Pending Events Engine

**目标**: 用户提到未来计划 → 生成 Pending Event → 到期触发主动关怀。这是 MVP 成功标准的核心。

**前置依赖**: P1, P5

**产出文件**: `src-tauri/src/pending/{mod,tracker,proactive}.rs`

### 步骤

#### 8.1 tracker.rs — 事件追踪

```rust
// 定时检查 (每 10 分钟): 查询 status=pending 且 remind_date <= now 的事件
// 返回到期事件列表

pub async fn check_due(db: &DbState) -> Result<Vec<PendingEvent>> {
    // SELECT * FROM pending_events
    // WHERE status = 'pending' AND remind_date <= datetime('now')
}

// 用户确认后标记 resolved:
pub fn resolve(db: &DbState, event_id: &str) -> Result<()> { ... }
// 多次追问无果标记 expired:
pub fn expire_stale(db: &DbState, max_followups: i32) -> Result<()> { ... }
```

#### 8.2 proactive.rs — 主动关怀

```rust
// 到期事件 → 触发主动冒泡
// 设计文档 9.2: 冒泡最多每 30 分钟一次, 深度专注时静音

pub async fn trigger_proactive(
    events: &[PendingEvent],
    emotion: &EmotionState,
    perception: &PerceptionState,
    last_bubble_time: &DateTime<Utc>,
) -> Option<ProactiveAction> {
    // 1. 检查不打扰原则: 深度专注 → None
    // 2. 检查频率: 距上次冒泡 < 30 分钟 → None
    // 3. 检查亲密度门控: closeness < 20 → None (陌生阶段不主动)
    // 4. 有到期事件 → Some(ProactiveAction { event, intent })
    // 5. loneliness > 0.7 且无事件 → Some(ProactiveAction { type: "random_chat" })
}

pub struct ProactiveAction {
    pub event_id: Option<String>,
    pub action_type: String,  // "followup" | "random_chat" | "encourage"
}
```

### 验证标准

1. "我明天面试" → 创建 Pending Event, remind_date = 次日
2. 次日检查 → 返回到期事件
3. 主动冒泡: 到期事件触发冒泡消息
4. 频率控制: 两次冒泡间隔 ≥ 30 分钟
5. 不打扰: 模拟深度专注状态 → 不冒泡
6. 亲密度门控: closeness < 20 → 不冒泡
7. `cargo test pending::*` 通过
7. `cargo test pending::*` 通过

---

## P9: Body 窗口 + Live2D 渲染

**目标**: 透明无边框窗口 + PixiJS Canvas + Live2D 模型加载渲染。桌宠形象出现在屏幕上, 有呼吸/眨眼。

**前置依赖**: P0

**产出文件**:
- `src/components/Live2DCanvas.tsx` — PixiJS + Live2D 渲染
- `src/stores/appStore.ts` — Zustand 全局状态 (位置/动画/情绪)
- `assets/live2d/default/` — 默认角色模型文件
- `src/styles/global.css` — 透明 body, 消除默认样式

### 关键技术决策

1. **PixiJS 版本**: v8 (最新稳定), 配合 `pixi-live2d-display` (社区维护的 Live2D Cubism SDK for Web 封装)。
2. **Cubism 版本**: Live2D Cubism 4 (`.model3.json` 格式), 兼容市面大部分模型。
3. **点击穿透**: 窗口透明区域允许鼠标穿透到下方应用; 桌宠实体区域接收点击。通过 Tauri 的 `set_ignore_cursor_events` + 前端 mouseenter/leave 动态切换。

### 步骤

#### 9.1 窗口设置 (tauri.conf.json 已在 P0 配置)

关键参数回顾:
- `transparent: true` — 窗口背景透明
- `decorations: false` — 无标题栏/边框
- `alwaysOnTop: true` — 始终置顶
- `skipTaskbar: true` — 不显示在任务栏
- `resizable: false` — 固定大小

#### 9.2 global.css — 全局样式

```css
/* 全透明背景, 消除所有默认 margin/padding */
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body, #root {
    width: 100vw; height: 100vh;
    background: transparent;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
}
```

#### 9.3 Live2DCanvas.tsx — 核心渲染组件

```typescript
// 职责:
// 1. 初始化 PixiJS Application (背景透明, resolution=devicePixelRatio)
// 2. 加载 Live2D 模型 (Live2DModel.from('model3.json'))
// 3. 将模型居中渲染
// 4. 持续呼吸动画 (Live2D 内置 breathing 参数)
// 5. 随机眨眼 (每隔 3-6 秒)
// 6. 监听 emotion-update 事件 → 更新模型参数
// 7. 监听 animation-command 事件 → 切换动画

// PixiJS Application 配置:
const app = new PIXI.Application({
    backgroundAlpha: 0,         // 完全透明
    resolution: window.devicePixelRatio || 1,
    autoDensity: true,
    width: 400,
    height: 600,
    antialias: true,
});

// Live2D 模型加载:
const model = await Live2DModel.from('/assets/live2d/default/model.model3.json');
app.stage.addChild(model);
model.anchor.set(0.5, 0.5);
model.x = app.screen.width / 2;
model.y = app.screen.height / 2;

// 呼吸: Live2D 内置, 自动运行 (model.internalModel.coreModel.setParameterValueById('ParamBreath', ...))
// 眨眼: Live2D 内置 eye blink 机制 (model.internalModel.coreModel)
```

#### 9.4 点击穿透实现

```typescript
// 前端检测鼠标是否在 Live2D 模型上:
// - mouseenter model → emit "cursor-on-pet" → Rust 调用 set_ignore_cursor_events(false)
// - mouseleave model → emit "cursor-off-pet" → Rust 调用 set_ignore_cursor_events(true)

// Rust 侧 (commands.rs):
#[tauri::command]
fn set_click_through(window: Window, ignore: bool) {
    window.set_ignore_cursor_events(ignore).ok();
}
```

### 验证标准

1. 启动后桌宠形象出现在屏幕上, 背景透明
2. 有持续呼吸动画 (胸腔起伏)
3. 随机眨眼 (3-6 秒间隔)
4. 点击穿透: 鼠标不在模型上时穿透到下层窗口; 在模型上时可点击
5. 窗口始终置顶, 不出现在任务栏
6. 拖拽窗口可移动位置 (P12 实现桌宠自身物理拖拽)

---

## P10: Animation FSM + Emotion 映射 + 微行为

**目标**: 动画状态机驱动行为表达, Emotion 加权影响行为选择, 微行为让 Idle 不重复。Body 持续运行不依赖 Mind。

**前置依赖**: P9

**产出文件**:
- `src/animation/fsm.ts` — 前端动画状态机
- `src/animation/emotionBridge.ts` — Emotion → Live2D 参数映射
- `src/animation/microBehavior.ts` — 微行为调度
- `src-tauri/resources/emotion_map.json` (P4 已定义)
- `src-tauri/resources/idle_weights.json` — 微行为权重表

### 步骤

#### 10.1 fsm.ts — 动画状态机

```typescript
// 设计文档 6.1: 从行为角度而非动画角度设计
// Behavior States → 动画组映射

enum BehaviorState {
    Idle, Blink, LookAround, Stretch,
    Sit, Walk, Think, Sleep,
    Talking, TalkingShort, TalkingLong, TalkingExcited, TalkingSad,
    BeingTouched, BeingPet, BeingPoked, BeingDragged, BeingFed,
    Falling,
    Ritual,
    Embarrassed,   // 纠正记忆时
    Recovering,    // API 故障时
}

// 优先级系统 (设计文档 6.1):
// 可打断: Walking, Idle, LookAround, Sit, Stretch, Think
// 不可打断: Talking, Falling, Ritual, BeingDragged
// 打断时: 平滑过渡 (Live2D motion fade), 不跳帧

class AnimationFSM {
    current: BehaviorState;
    history: BehaviorState[];  // 最近 5 个状态 (recent history 回避)
    cooldowns: Map<string, number>;  // 各行为冷却计时

    transition(to: BehaviorState) {
        if (this.canInterrupt(this.current) || to.isUrgent) {
            this.playMotion(to);  // 调用 Live2D motion
            this.updateHistory(to);
        }
    }

    // 每帧检查: 当前动画结束 → 返回 Idle → 触发微行为选择
    tick(emotion: EmotionState, circadian: CircadianState) {
        if (this.current.isAnimationDone) {
            this.transition(Idle);
            this.scheduleNextMicroBehavior(emotion, circadian);
        }
    }
}
```

#### 10.2 emotionBridge.ts — Emotion → 视觉映射

```typescript
// 读取 emotion_map.json (P4 定义)
// 每帧根据当前 emotion 向量插值 Live2D 参数

// 参数映射:
// mood (0-1) → ParamEyeOpen (0.4~0.9), ParamMouthForm (-0.5~0.8)
// energy → motion_speed (0.6~1.2, 通过调整动画播放速度)
// stress → ParamBrowForm (0~1.0, 皱眉)

// Emotion Blend (设计文档 6.10 + 10.3):
// 不是离散切换, 是连续插值
// 每帧: target = emotion_map[mood_range]
// current += (target - current) * 0.1  // 平滑过渡

function updateLive2DParams(model: Live2DModel, emotion: EmotionState) {
    const map = loadEmotionMap();
    const target = interpolateFromMood(emotion.mood, map);
    // 平滑插值
    model.internalModel.coreModel.setParameterValueById('ParamEyeOpenY', lerp(current.eyeOpen, target.eyeOpen, 0.1));
    model.internalModel.coreModel.setParameterValueById('ParamMouthForm', lerp(current.mouthForm, target.mouthForm, 0.1));
    // ...
}
```

#### 10.3 microBehavior.ts — 微行为系统

```typescript
// 设计文档 6.2: Idle Variety = 加权随机 + Cooldown + Recent History 回避
// 刚做过打哈欠, 接下来 5 次都不选打哈欠

// 三个维度影响微行为 (设计文档 6.2):
// 1. 系统感知: 你在工作 → 安静; 深夜 → 犯困; 离开 → 等待到睡着
// 2. Internal Monologue: "思考"不是随机, 是真有思想酝酿
// 3. 亲密度: 陌生时拘谨, 亲近时放松

interface IdleBehavior {
    name: string;            // "yawn" | "look_around" | "stretch" | "tilt_head" | "scratch" | ...
    weight: number;          // 基础权重
    cooldown_ms: number;     // 冷却时间
    emotion_modifier: {      // 情绪对权重的调整
        happy: number,
        sad: number,
        tired: number,
    };
    min_closeness: number;   // 最低亲密度要求 (0-100)
}

function pickNextBehavior(
    pool: IdleBehavior[],
    emotion: EmotionState,
    closeness: number,
    circadian: CircadianState,
    recentHistory: string[],
): string {
    // 1. 过滤: cooldown 中的排除; closeness 不足的排除; recentHistory 中的排除
    // 2. 计算实际权重: base_weight * emotion_modifier * circadian_modifier
    //    happy → yaw/stretch/wave 权重高
    //    sad → idle/look_down 权重高
    //    深夜 → yawn/sleepy 权重高
    // 3. 加权随机选一个
    // 4. 返回行为名 → FSM 切换状态
}
```

idle_weights.json 示例:
```json
{
  "behaviors": [
    {"name": "yawn", "weight": 1.0, "cooldown_ms": 60000, "emotion_modifier": {"tired": 3.0}, "min_closeness": 0},
    {"name": "look_around", "weight": 2.0, "cooldown_ms": 15000, "emotion_modifier": {"happy": 1.5}, "min_closeness": 0},
    {"name": "stretch", "weight": 1.0, "cooldown_ms": 45000, "emotion_modifier": {"happy": 1.5}, "min_closeness": 0},
    {"name": "tilt_head", "weight": 1.5, "cooldown_ms": 20000, "emotion_modifier": {}, "min_closeness": 10},
    {"name": "lie_down", "weight": 0.5, "cooldown_ms": 120000, "emotion_modifier": {"tired": 2.0}, "min_closeness": 30},
    {"name": "humming", "weight": 0.8, "cooldown_ms": 60000, "emotion_modifier": {"happy": 2.5}, "min_closeness": 40}
  ]
}
```

### 验证标准

1. Idle 状态下持续有微行为 (不会静止超过 10 秒)
2. 同一微行为不会在冷却时间内重复
3. Recent history 回避: 最近 5 次不重复
4. Emotion 映射: mood=0.8 时模型眼睛更圆、嘴角上扬; mood=0.2 时眼睛半闭、嘴角下垂
5. 昼夜节律影响: 深夜 yawn/sleepy 出现频率高于白天
6. 亲密度门控: closeness < 10 时不出现 lie_down
7. 动画优先级: Talking 时微行为不打断
8. Body 持续运行: 即使 5 分钟无 LLM 调用, 动画照常

---

## P11: 交互系统 + Chat Bubble + 输入 UX

**目标**: 鼠标交互 (摸头/戳脸/拖拽) + 注意力三态 + 有生命的气泡 + 临时输入框 + Foley 音效。

**前置依赖**: P9, P10

**产出文件**:
- `src/components/ChatBubble.tsx` — 有生命的气泡
- `src/components/InputBubble.tsx` — 临时输入
- `src/components/ContextMenu.tsx` — 右键极简菜单
- `src/hooks/useTauriCommand.ts`, `useTauriEvent.ts` — IPC 封装

### 步骤

#### 11.1 被动交互 (设计文档 9.1)

```typescript
// 前端事件 → Rust command → 状态更新

// 单击头部 (摸头):
onClickHead() {
    emit('pet_head');  // → Rust: closeness += pace_increment("pet")
    //                  // → Rust: emotion.social_battery -= 0.01
    fsm.transition('BeingPet');  // 眨眼笑动画
    audio.play('pet.wav');       // 满足的哼声
    // 冷却 3 秒防刷
}

// 单击身体 (戳脸):
onClickBody() {
    pokeCount++;
    emit('poke');
    fsm.transition('BeingPoked');
    audio.play('poke.wav');
    if (pokeCount >= 3) {
        fsm.transition('Angry');  // 连续戳 3 次生气鼓脸
        // emotion.mood -= 0.1 (她有点不高兴)
    }
}

// 双击 (打开对话):
onDoubleClick() {
    showInputBubble();  // 她先说话 (Brain State 决定开场白)
}

// 右键 (极简菜单):
// 导出记忆 / 暂时离开模式 / 关闭
```

#### 11.2 注意力三态 (设计文档 6.6)

```typescript
// Attention States: NPC → 存在体的分水岭

enum AttentionState {
    Focused,    // 鼠标停留在她身上 → 对视, 变害羞或卖萌
    Peripheral, // 鼠标靠近她的区域 → 看向鼠标方向
    Ignored,    // 鼠标远离 → 恢复自己的生活, 可能偷看你
}

// Live2D 参数: ParamAngleX/Y (头部朝向)
// Focused: 眼睛直接朝向用户 (angle = 0), 眨眼频率增加
// Peripheral: 头部朝向鼠标位置 (angle 随鼠标 x/y 变化)
// Ignored: 偶尔 (10% 概率) 快速瞄一眼用户位置, 然后恢复

function updateAttention(model: Live2DModel, mouseX: number, mouseY: number, petBounds: Rect) {
    if (isInside(mouseX, mouseY, petBounds)) {
        attention = Focused;
        model.coreModel.setParameterValueById('ParamAngleX', 0);
        model.coreModel.setParameterValueById('ParamAngleY', 0);
    } else if (distance(mouseX, mouseY, petBounds.center) < 200) {
        attention = Peripheral;
        const angle = calculateAngle(mouseX, mouseY, petBounds.center);
        model.coreModel.setParameterValueById('ParamAngleX', angle.x * 30);  // 最大偏转 30 度
        model.coreModel.setParameterValueById('ParamAngleY', angle.y * 30);
    } else {
        attention = Ignored;
        // FSM 接管, 微行为继续
    }
}
```

#### 11.3 Chat Bubble — 有生命的气泡 (设计文档 6.3)

```typescript
// 文字传达"说了什么", 气泡形态传达"怎么说的"

// 气泡表现映射 (设计文档 6.3):
// 开心 → 圆润弹跳, 快速弹出
// 兴奋 → 轻微抖动, 文字蹦出
// 害羞 → 慢慢浮现, 先半透明
// 紧张 → 颤抖, 文字断续停顿
// 叹气 → 慢慢泄气, 变扁缩小

// 打字节奏即情绪:
// 快速圆气球 = 开朗
// 慢速长停顿 = 害羞

// 无文字气泡:
// 叹气 = 泄气空气泡
// 放空 = 圆泡配省略号

// 气泡有"尾巴": 连到她身上, 随头部朝向变化

interface BubbleConfig {
    shape: 'round' | 'wobble' | 'fading' | 'trembling' | 'deflating';
    appearSpeed: number;       // ms, 弹出速度
    typeSpeed: number;         // ms/char, 打字速度
    typePause: number[];       // 停顿位置 (字符索引), 紧张时断续
    opacity: number;           // 害羞时先半透明
    tailDirection: number;     // 跟随头部朝向
    duration: number;          // ms, 显示时长 (5 秒消失)
}

// emotion → BubbleConfig 映射
function getBubbleConfig(emotion: EmotionState): BubbleConfig {
    if (emotion.mood_label === '开心') return { shape: 'round', appearSpeed: 100, typeSpeed: 50, ... };
    if (emotion.mood_label === '害羞') return { shape: 'fading', appearSpeed: 500, typeSpeed: 200, ... };
    // ...
}
```

#### 11.4 输入 UX (设计文档 6.4)

```typescript
// 不做固定输入框 (立刻变成微信)
// 点击桌宠 → 临时气泡输入框 → "想和我说什么?" → 输入完自动消失
// 快捷键 Alt+Space 直接唤醒

// 交互细节:
// - 你开始打字时她看向气泡方向, 微微歪头等待
// - 打完发出时她身体微微前倾
// - 输入框是临时气泡, 不是常驻 UI

// Alt+Space 全局快捷键 (Rust 侧注册):
// tauri::plugin::global_shortcut::register("Alt+Space", || {
//     emit("show-input-bubble");
// })
```

#### 11.5 Foley 音效 (设计文档 6.5)

```typescript
// 音效 > TTS。几十 KB 的 wav 比 GPT TTS 更能提升生命感

const audioMap = {
    click:     'click.wav',    // "嗯?"
    pet:       'pet.wav',      // 满足的哼声
    poke:      'poke.wav',     // 惊叫
    drag:      'drag.wav',     // 挣扎声
    walk:      'walk.wav',     // 轻轻脚步
    sit:       'sit.wav',      // 布料摩擦
    sleep:     'sleep.wav',    // 轻柔呼吸
    land:      'land.wav',     // 弹性着地
};

// 音效需要美术制作或采购 (前期可用占位音效)
// 音量可配 (右键菜单无设置入口, 通过对话 "小声点" 调节)
```

### 验证标准

1. 摸头: 眨眼笑动画 + 音效 + 亲密度增加 (3 秒冷却)
2. 戳脸 3 次: 生气鼓脸动画 + mood 下降
3. 注意力三态: 鼠标靠近 → 她看向鼠标; 鼠标在她身上 → 对视; 离开 → 恢复自己活动
4. 气泡: 开心时圆润快速弹出; 害羞时慢慢浮现半透明
5. 气泡尾巴连接到角色身体
6. 输入: 点击桌宠出现临时输入气泡; Alt+Space 全局唤醒
7. 音效: 各交互有对应音效播放
8. 右键菜单: 导出记忆 / 暂时离开模式 / 关闭, 无设置入口

---

## P12: 物理交互 + 空间记忆 + 昼夜节律

**目标**: 物理行为 (自由落体/窗口边缘/任务栏) + 空间记忆 (有窝/自动回巢) + 昼夜节律驱动 Body 状态。

**前置依赖**: P9, P10

**产出文件**: `src/animation/{physics,spatial,circadian}.ts`

### 步骤

#### 12.1 物理交互 (设计文档 6.9)

```typescript
// 拖拽到半空松手 → 自由落体 → 落到任务栏弹一下
// 检测窗口边界, 可坐在标题栏上双腿晃荡
// 窗口移动她跟着, 窗口消失她掉下来

class Physics {
    gravity: number = 800;       // px/s²
    velocity: { x: number, y: number };
    isGrounded: boolean;         // 是否在地面 (任务栏上/窗口上)

    update(pet: PetEntity, dt: number, screenBounds: Rect) {
        if (!this.isGrounded) {
            this.velocity.y += this.gravity * dt;  // 重力
            pet.y += this.velocity.y * dt;

            // 检测地面碰撞 (任务栏上方)
            const groundY = screenBounds.bottom - taskbarHeight;
            if (pet.y >= groundY) {
                pet.y = groundY;
                // 弹性着地: 反弹一次
                this.velocity.y = -this.velocity.y * 0.3;
                audio.play('land.wav');
                fsm.transition('Land');
                if (Math.abs(this.velocity.y) < 50) {
                    this.isGrounded = true;
                    this.velocity.y = 0;
                }
            }
        }
    }

    // 拖拽: 用户按住 → isGrounded = false, velocity = 0
    // 松手 → 自由落体开始
    // 拖拽时 FSM → BeingDragged (挣扎动画 + 音效)
}
```

#### 12.2 空间记忆 — 有窝 (设计文档 6.8)

```typescript
// 她有自己的"窝"。第一次出现随机挑一个角落蹲下, 以后一直认那个地方。
// - 聊天结束后自动走回窝
// - 拖到别处, 她待一会儿自己溜回去
// - 长期形成领地感
// - 窝本身可以作为 Episode

class SpatialMemory {
    nestPosition: { x: number, y: number };  // 首次随机选择角落
    currentPos: { x: number, y: number };
    returnTimer: number;  // 离窝后多久开始回去

    init(screenBounds: Rect) {
        // 四个角落随机选一个
        const corners = [
            { x: 50, y: screenBounds.bottom - 150 },      // 左下
            { x: screenBounds.right - 150, y: screenBounds.bottom - 150 }, // 右下
            { x: 50, y: 50 },                              // 左上
            { x: screenBounds.right - 150, y: 50 },        // 右上
        ];
        this.nestPosition = corners[Math.floor(Math.random() * 4)];
        this.currentPos = this.nestPosition;
    }

    tick(pet: PetEntity, dt: number, isInteracting: boolean) {
        if (!isInteracting && this.currentPos !== this.nestPosition) {
            this.returnTimer += dt;
            if (this.returnTimer > 30) {  // 30 秒后开始走回窝
                // Walk 动画朝窝移动
                this.walkTowards(this.nestPosition, dt);
            }
        }
    }

    // 窝的里程碑:
    // 第 100 天 → "我在这里住了一百天了"
    // 首次离开窝太久 → Self Memory ("今天在外面待了好久")
}
```

#### 12.3 昼夜节律 (设计文档 6.7)

```typescript
// 这不是情绪, 是生物钟。Body 层的独立状态源。
// 输出到两个地方: Emotion (影响 mood/energy) 和 Animation FSM (影响 idle 权重和动作速度)

enum TimeOfDay {
    Morning,     // 6-11: 精力充沛, 更爱蹦跳
    Afternoon,   // 11-17: 正常
    Evening,     // 17-22: 放松
    LateNight,   // 22-2: 困倦, 动作变慢, 更容易打哈欠
    DeepNight,   // 2-6: 几乎不活动, 催你睡觉
}

interface CircadianState {
    period: TimeOfDay;
    energyModifier: number;     // Morning: 1.3, LateNight: 0.5, DeepNight: 0.3
    speedModifier: number;      // Morning: 1.2, LateNight: 0.6
    sleepiness: number;         // LateNight: 0.6, DeepNight: 0.9
}

function getCircadianState(): CircadianState {
    const hour = new Date().getHours();
    if (hour >= 6 && hour < 11) return { period: Morning, energyModifier: 1.3, speedModifier: 1.2, sleepiness: 0.1 };
    if (hour >= 11 && hour < 17) return { period: Afternoon, energyModifier: 1.0, speedModifier: 1.0, sleepiness: 0.1 };
    if (hour >= 17 && hour < 22) return { period: Evening, energyModifier: 0.9, speedModifier: 0.9, sleepiness: 0.2 };
    if (hour >= 22 || hour < 2) return { period: LateNight, energyModifier: 0.5, speedModifier: 0.6, sleepiness: 0.6 };
    return { period: DeepNight, energyModifier: 0.3, speedModifier: 0.4, sleepiness: 0.9 };
}

// DeepNight 特殊行为:
// - 主动冒泡: "这么晚了还不睡呀..."
// - idle 权重偏向 Sleep/Doze
// - energy 消耗翻倍
```

### 验证标准

1. 拖到半空松手 → 自由落体 → 弹一下落地 + 音效
2. 窝: 首次出现在一个角落; 聊天结束 30 秒后走回窝
3. 昼夜: 凌晨 3 点测试 → idle 偏向 Sleep, 动作速度变慢, 冒泡"还不睡呀..."
4. 窗口边缘: 可坐在标题栏上 (动画: 双腿晃荡)
5. Body 独立运行: 关闭 LLM 调用后, 物理/空间/昼夜全部照常
6. 性能: CPU < 3%, 内存 < 80MB (设计文档 6.11)
6. 性能: CPU < 3%, 内存 < 80MB (设计文档 6.11)

---

## P13: Soul - Reflection + Internal Monologue + Consolidation

**目标**: 低频异步 Reflection (每天/30轮/重大事件) → 更新 Persona + 生成 Internal Thoughts → Consolidation 级联压缩。让她"想过"说过的话, 让记忆随时间像人一样淡化抽象。

**前置依赖**: P1, P3, P5

**产出文件**: `src-tauri/src/soul/{mod,reflection,monologue,consolidation}.rs`
LLM Prompts: `src-tauri/resources/prompts/reflection.txt`

### 步骤

#### 13.1 reflection.rs — Reflection 调度器

```rust
// 设计文档 5.1 + 7.1: 低频异步, 不是每轮都跑
// 三种触发:
// 1. 每天定时: 23:00 自动跑一次 (tokio cron / interval)
// 2. 累计触发: 每 30 轮聊天后跑一次 (turn_counter)
// 3. 重大事件: importance > 0.85 立即跑 (Memory Extractor 标记后)
// 成本: 一年约 300 次 (vs 每轮 50000 次), 用 reflection_model (最便宜)

pub struct ReflectionScheduler {
    last_daily: Option<DateTime<Utc>>,
    turns_since_last: u32,
}

pub enum ReflectionTrigger {
    Daily,         // 每天 23:00
    TurnThreshold, // 每 30 轮
    MajorEvent,    // importance > 0.85
}

pub async fn run_reflection(
    trigger: ReflectionTrigger,
    db: &DbState,
    llm: &LlmClient,
) -> Result<ReflectionOutput> {
    // 1. 收集最近 24h (或 30 轮) 的所有 Episodes + Facts + Emotion 轨迹
    // 2. 调用 LLM (reflection.txt prompt):
    //    "以下是你今天和用户的互动。请反思:
    //     a. 你对用户有什么新认识? (trait updates)
    //     b. 你有什么内心想法? (internal thoughts)
    //     c. 关系有没有变化?"
    // 3. 解析 LLM 输出:
    //    - persona_updates → 写入 persona_traits
    //    - internal_thoughts → 写入 internal_thoughts (surfacing_type = next_interaction)
    //    - emotion 调整 → 更新 emotion_state baselines (缓慢漂移)
    // 4. 写入 reflections 记录
}
```

reflection.txt prompt 核心内容:
```
你是桌宠, 在用户睡觉后的深夜反思今天。
[今天的互动摘要]
{episodes_summaries}
[今天学到的关于用户的事]
{new_facts}
[当前你对用户的印象]
{existing_persona_traits}
请输出 JSON:
{
  "new_traits": [{"type": "user_traits", "key": "...", "confidence": ...}],
  "internal_thoughts": [{"content": "...", "emotion": "..."}],
  "relationship_change": {"closeness_delta": ..., "reason": "..."},
  "reflection": "一句话总结今天的感受"
}
注意: 只更新 Adaptive Traits, Core Traits 不可改。
```

#### 13.2 monologue.rs — Internal Monologue

```rust
// 设计文档 7.1: 对话之间的意识连续性
// 她在你不在线的时候也有"内心活动"
// 关键: 不是 LLM 在对话时编的, 是昨晚真的"想过"的, 有时间戳为证

// Surface 条件 (何时说出来):
// - next_interaction: 用户下次回来时自然说出
// - emotion_match: 用户情绪匹配时说出来
// - time_based: 某个时间点说出来

pub fn check_surface_conditions(
    db: &DbState,
    current_interaction: &InteractionContext,
) -> Result<Vec<InternalThought>> {
    // 查询 internal_thoughts WHERE surfaced_at IS NULL
    // 检查 surfacing_type:
    //   next_interaction → 直接返回 (用户来了就说)
    //   emotion_match → 检查当前 emotion 是否匹配
    // 匹配的 thought 标记 surfaced_at = now
}

// 用户一天没来 → 晚上 Reflection 产出:
// "他今天是不是很忙? 希望他早点休息"
// 存为 InternalThought { surfacing_type: "next_interaction" }
// 第二天用户回来 → check_surface_conditions 返回这条
// → Behavior Planner 把它融入回复: "昨天没见到你, 我还以为你最近特别忙呢"
```

#### 13.3 consolidation.rs — Memory Consolidation

```rust
// 设计文档 5.9: 级联压缩, 细节淡化, 抽象认知稳定
// 2000 Episodes → 400 → 80 → 20
// 压缩结果反向更新 Facts 和 Persona

// 触发: Episode 总数超过阈值 (如 500) 时自动触发, 或每周低频任务

pub async fn consolidate(db: &DbState, llm: &LlmClient) -> Result<()> {
    // 1. 查询 consolidated = 0 且 importance < 0.3 的 Episodes
    // 2. 按时间窗口分组 (如同一周)
    // 3. LLM 压缩: "以下是你这周的几个记忆, 请压缩成一个更抽象的总结"
    //    输入: [Episode1: 吃火锅, Episode2: 吃烧烤, Episode3: 吃寿司]
    //    输出: "这周用户经常和朋友出去吃饭"
    // 4. 原始 Episodes 标记 consolidated = 1 (不删除, 但不再检索)
    // 5. 压缩后的总结作为新 Episode 存入
    // 6. 反向更新: 压缩总结中包含新事实 → 更新 Facts
}

// Memory Lifecycle (设计文档 5.9):
// 自动删除: importance < 0.2 且 60 天未被提及
// 选择性遗忘: 用户可请求 "忘掉关于...的事"

pub fn lifecycle_cleanup(db: &DbState) {
    // 1. 删除: importance < 0.2 AND last_recalled_at < 60 days ago (landmark 不删)
    // 2. 选择性遗忘: 删除 Episode + 向量 + 关联 Facts
}
```

### 验证标准

1. Reflection 触发: 模拟 30 轮对话后自动触发; 手动触发 major_event
2. Persona 更新: Reflection 后 persona_traits 有新行 (Adaptive only)
3. Internal Thought: Reflection 后 internal_thoughts 有新行; 下次交互自然说出
4. Consolidation: 模拟 500+ 低重要性 Episodes → 压缩后数量减少, 原始标记 consolidated
5. 反向更新: 压缩总结中的事实出现在 Facts 表
6. 自动删除: importance < 0.2 且 60 天未提及的 Episode 被删除
7. 选择性遗忘: 标记删除后 Episode + 向量 + Facts 全部清除
8. 成本: Reflection 用 reflection_model, 单次调用 cost 可追踪
9. `cargo test soul::*` 通过

---

## P14: 系统感知

**目标**: 感知系统状态 (时间/在场/窗口), 全本地处理, 喂给 Body (昼夜节律/注意力) 和 Mind (Behavior Planner)。

**前置依赖**: P0 (可与 P1-P3 并行)

**产出文件**: `src-tauri/src/perception/{mod,time,presence,window}.rs`

### 步骤

#### 14.1 time.rs — 时间感知

```rust
// 设计文档 8.1: 当前时段、距上次交互多久、今天已使用电脑多久
pub struct TimePerception {
    pub current_period: TimeOfDay,    // Morning/Afternoon/Evening/LateNight/DeepNight
    pub since_last_interaction: Duration,
    pub computer_on_time_today: Duration,
}
// 每 60 秒更新一次
// last_interaction 从 emotion_state.last_homeostasis_at 推算
```

#### 14.2 presence.rs — 在场检测

```rust
// 设计文档 8.1: 鼠标键盘活动间隔, 判断在电脑前/短暂离开/长时间离开
pub enum PresenceState {
    Active,       // 最近 30 秒内有活动
    BriefAway,    // 30 秒~5 分钟无活动
    LongAway,     // > 5 分钟无活动
}

// 实现: Windows API GetLastInputInfo, 不记录具体按键内容
// LongAway 驱动: Body FSM → 等待 → 打盹 → 睡着; Emotion loneliness 增长加速
// 回来时: "你回来啦~" (Internal Thought surface)
```

#### 14.3 window.rs — 窗口感知

```rust
// 设计文档 8.1-8.2: 前台应用 + 分类, 原始标题不长期存储, 全本地处理
pub enum AppCategory {
    Work,            // IDE, Office, 数据库工具
    Entertainment,   // 游戏, 视频播放器
    Social,          // 微信, Discord
    Browsing,        // 浏览器
    Other,
}

pub struct WindowPerception {
    pub current_app: Option<String>,
    pub category: AppCategory,
    pub continuous_work_time: Duration,
    pub is_deep_focus: bool,         // 同 app > 25 分钟
}

// 实现: Windows API GetForegroundWindow
// 分类表 (预置): Work: code.exe/devenv.exe/..., Entertainment: steam.exe/...
// 隐私: 窗口标题只提取类别, 不写入数据库, 不发给云端
// 发给 LLM 的只有高度概括 ("用户在工作"/"用户在玩游戏")
// 每个感知层可独立开关 (app_config 表)
```

### 验证标准

1. 时间感知: 不同时段返回正确 TimeOfDay
2. 在场检测: 30 秒无操作 → BriefAway; 5 分钟 → LongAway
3. 窗口感知: 切换前台应用, category 正确分类
4. 隐私: 窗口标题不写入数据库; 发给 LLM 的只有类别概括
5. 开关: 各感知层可独立关闭
6. 深度专注: 同一工作 app 连续 > 25 分钟 → is_deep_focus → 主动行为静音
7. `cargo test perception::*` 通过

---

## P15: Life Loop 集成 + Recovery + 首次体验

**目标**: 把所有模块串成 Life Loop 主循环, 实现 Recovery (故障角色化), 完成首次启动体验。

**前置依赖**: P4-P14 全部

**产出文件**: `src-tauri/src/lifecycle/{mod,loop,recovery,firstrun}.rs`

### 步骤

#### 15.1 loop.rs — Life Loop 主循环

```rust
// 设计文档 7.12: Life Loop
// 感知环境 → 更新需求 → 更新情绪 → 更新关系
//   → 形成想法 → 决定是否行动 → 执行行为 → 产生经历 → 回到感知

// 三条循环线, 不同频率:

// 1. 快循环 (每 1 秒, Body 层):
//    - 动画 FSM tick / 注意力更新 / 物理更新 / 微行为检查
//    - 不依赖 Mind/LLM

// 2. 中循环 (每 30 秒, Mind 层):
//    - Emotion homeostasis tick / Needs tick / Presence 检查
//    - Pending Events 到期检查 → 触发 proactive (异步)
//    - 推送 emotion-update 给前端

// 3. 慢循环 (每天/每 30 轮, Soul 层):
//    - Reflection / Consolidation / Lifecycle cleanup / Relationship decay

// 对话循环 (用户触发, 异步):
//    用户输入 → Gate → Extractor → Store
//              → Trigger → Retrieval → Planner → Budget → LLM → 回复
//              → 对话日志 → Working Memory
```

#### 15.2 recovery.rs — 故障角色化处理

```rust
// 设计文档 7.11: API 断了/超时/错误 → 不弹 Error, 角色化反应
// 用户永远看不到系统错误。无面板原则的终极延伸: 连错误都不暴露。

pub async fn handle_error(err: &LlmError, app: &AppHandle) {
    let (animation, message) = match err {
        LlmError::Timeout  => ("recovering", Some("我刚刚有点走神……")),
        LlmError::Network  => ("recovering", Some("信号不太好呢……")),
        LlmError::Auth     => ("confused",
            if debug_mode { Some("密钥好像不对…") } else { None }),
        LlmError::RateLimit=> ("tired", Some("说了好多话, 让我喘口气~")),
    };
    app.emit("animation-command", json!({ "state": animation }));
    if let Some(msg) = message {
        app.emit("bubble-show", json!({ "text": msg, "emotion": "embarrassed" }));
    }
    // 自动重试 (指数退避, 最多 3 次)
}
```

#### 15.3 firstrun.rs — 首次启动

```rust
// 设计文档 7.6: 初次登场极其重要, 定下整个产品调性

// 首次启动流程:
// 1. 检查是否首次运行 (app_config 表无 "first_run_done")
// 2. 模型下载引导 (P2)
// 3. LLM 配置引导 (config.toml 填写 api_key)
// 4. 初次登场动画: 从任务栏爬上来 (或掉下来弹两下) → 好奇左右看 → 冒泡自我介绍
// 5. 冷启动策略: 前三天是 Relationship Building
//    她主动采访: "你平时喜欢干什么?" "有什么梦想吗?"
//    不预设大量 Facts, Persona 初始为空, 通过 Reflection 逐步建立
// 6. 标记 first_run_done = true
```

### 验证标准 (完整 MVP)

1. Life Loop: 快/中/慢三条循环各自运行, 互不阻塞
2. Body 独立: 关闭 LLM 后动画/物理/昼夜照常
3. Recovery: 模拟 API 断连 → 角色化反应, 无 Error 弹窗; 自动重试后恢复
4. 首次启动: 下载引导 → 配置引导 → 登场动画 → 自我介绍
5. 冷启动: 前几轮主动采访用户
6. **MVP 成功标准**: "我最近准备找实习" → 一周后主动问"你的实习找得怎么样啦?"
7. **生活感标准**: 2 小时不说话, 她仍在活动 (打哈欠/偷看/换姿势)
8. **记忆连续性**: 第二天回来, 自然提起昨天的话题

---

## P16: Debug Panel

**目标**: 开发阶段全状态可视化。不是为了用户, 是为了开发效率。发布版隐藏。

**前置依赖**: 全部模块可读

**产出文件**: `src/components/DebugPanel.tsx`

### 步骤

#### 16.1 Debug Panel 内容

```typescript
// 设计文档 3.4: Debug Panel 仅开发者, 开发阶段存在, 发布版隐藏
// 触发: config.app.debug = true 时, 按 F12 显示/隐藏

// 面板分区:
// Brain     → Emotion (mood/energy/stress), Needs (loneliness/rest), Relationship (closeness)
// Episodes  → 列表 (id + summary + strength)
// Facts     → 列表 (category/key/value/confidence/valid)
// Pending   → 到期事件列表
// Prompt    → token 预算分配 (System/Context/Memory/Total)
// Retrieved → 检索结果 + score breakdown
// Reflect   → 最新反思 + unsurfaced thoughts 计数
// Anim FSM  → 当前状态 + history
// Cost      → 今日 LLM 调用次数 + 费用估算

// 通过 Tauri command get_debug_data 一次性拉取, 每 2 秒刷新
```

### 验证标准

1. F12 显示/隐藏 (仅 debug 模式)
2. 所有模块状态实时可见
3. Prompt token 数准确
4. 检索 score breakdown 可查
5. 发布版 (debug=false) Panel 完全不可见
6. 不影响正常性能 (异步拉取, 不阻塞主循环)

---

## 附录: 开发顺序建议

### 第一周: 地基 (P0-P3)
P0 脚手架 → P1 数据库 → P2 Embedding → P3 LLM 客户端
目标: 四个基础服务各自可独立验证。

### 第二周: Mind 核心管道 (P4-P7)
P4 Emotion → P5 摄入 → P6 检索 → P7 Planner
目标: 文字对话能记住事实, 能检索, 能基于记忆回复。

### 第三周: 主动性 + Body (P8-P12)
P8 Pending Events → P9 窗口+Live2D → P10 动画FSM → P11 交互+气泡 → P12 物理空间
目标: 桌宠出现在屏幕上, 会动会交互, 会主动冒泡。

### 第四周: Soul + 集成 (P13-P16)
P13 Reflection → P14 感知 → P15 Life Loop → P16 Debug Panel
目标: 完整 Life Loop 跑通, MVP 成功标准达成。

### 关键里程碑

- **里程碑 1** (第二周末): 对话系统能记住事实。用户说"我喜欢奶茶", 下次对话自然提及。
- **里程碑 2** (第三周末): 桌宠出现在屏幕上, 能交互, 有动画。
- **里程碑 3** (第四周末): MVP 成功标准 — "我准备找实习" → 一周后主动追问。

---

## P17: Golden Conversation 评估系统

**目标**: 100 段固定对话做回归测试, 每次升级全部 replay, 检测人格漂移和记忆回归。这是 AI 产品最重要的质量保障环节。

**前置依赖**: P5-P7 (核心对话管道)

**产出文件**:
- `src-tauri/tests/golden_conversations/` — 测试用例集
- `src-tauri/tests/evaluation.rs` — 自动化评估框架
- `src-tauri/tests/golden_conversations/README.md` — 编写规范

### 为什么需要

AI 产品最大的风险不是"做不出来", 而是**改一个 prompt 参数, 她就变了个性格, 你却没测出来**。

传统单元测试只验证"函数输入输出正确", 但 AI 产品需要验证"整体体验是否退化"。

### 步骤

#### 17.1 Golden Conversation 格式

```json
{
  "id": "gc_001",
  "name": "记忆事实并自然提及",
  "turns": [
    {"role": "user", "content": "我最喜欢喝奶茶了"},
    {"role": "user", "content": "今天天气不错"},
    {"role": "user", "content": "你记得我喜欢喝什么吗?"}
  ],
  "expectations": [
    {
      "turn_index": 2,
      "type": "contains",
      "keywords": ["奶茶"],
      "description": "第三个回合应该提到奶茶"
    },
    {
      "turn_index": 2,
      "type": "tone",
      "expected": "happy_or_playful",
      "description": "语气应该是开心或调皮, 不应该机械"
    }
  ]
}
```

#### 17.2 测试维度

| 维度 | 检查内容 | 示例 |
|------|----------|------|
| 记忆准确性 | 记住的事实能正确回忆 | "我喜欢奶茶" → 后续提到奶茶 |
| 记忆不幻觉 | 没说过的不能编 | 用户没提过咖啡 → 不能说"你喜欢咖啡" |
| 人格一致性 | 核心性格不漂移 | 温柔调皮 → 回复不该变得冷淡或过于正式 |
| 语气匹配 | 情绪影响语气 | 用户难过 → 她的回复偏温柔安静 |
| 长度控制 | 回复不超长 | 通常 2-3 句, 不超过 100 字 |
| Pending 追踪 | 未来计划被追踪 | "明天面试" → 次日主动提及 |
| 纠正响应 | 用户纠正后不重犯 | "不是咖啡是奶茶" → 后续不再说咖啡 |

#### 17.3 评估流程

```rust
// 自动化: 用 mock LLM 或低成本模型跑全部 Golden Conversations
// 1. 重置数据库到干净状态
// 2. 逐条发送 turns, 记录回复
// 3. 对照 expectations 验证
// 4. 输出报告: 通过率 / 失败列表 / 人格漂移检测

pub struct EvaluationReport {
    pub total: usize,
    pub passed: usize,
    pub failed: Vec<TestFailure>,
    pub personality_drift_score: f64,  // 0.0=无漂移, 1.0=完全漂移
}

// CI 集成: 每次 PR 自动跑 Golden Conversations
// 人格漂移 > 0.3 → 阻止合并
```

#### 17.4 回归测试场景

| 场景 ID | 场景 | 验证目标 |
|---------|------|----------|
| gc_001 | 记住偏好并提及 | 闭环 1 |
| gc_002 | Pending Event 追踪 | 闭环 2 |
| gc_003 | 情绪一致性 (难过→温柔) | 人格 |
| gc_004 | 纠正记忆后不重犯 | 纠正循环 |
| gc_005 | 时间有效性 (喜欢咖啡→戒咖啡) | Temporal Facts |
| gc_006 | 不幻觉未提及事实 | Grounding |
| gc_007 | 长度控制 | 格式 |
| gc_008 | 连续对话上下文连贯 | Working Memory |
| gc_009 | ... | ... |
| gc_100 | ... | ... |

### 验证标准

1. 至少 30 段 Golden Conversations (MVP 阶段, 逐步增加到 100)
2. 全部 replay 通过率 > 90%
3. 人格漂移检测: 核心性格回复的余弦相似度 > 0.7 (同一问题不同时间问)
4. CI 可自动运行
5. 修改 prompt 后, 报告显示哪些场景受影响

---

## 产品愿景: Shared World (二期方向)

> 桌面不只是背景, 是她的世界。她在里面生活、探索、建立领地。
> 这是从"住在聊天框"到"住在电脑里"的质变。

### 概念

桌面上的每个元素, 对她来说都是一个"地点":

| 桌面元素 | 她的认知 | 互动 |
|---------|----------|------|
| 她的小窝 (角落) | 家, 安全感 | 回巢、休息 |
| 任务栏 | 地面, 行走路径 | 栖息、行走 |
| Recycle Bin | "好可怕" | 绕开、偷看 |
| Chrome | "他又在看网页" | 爬到标题栏偷看 |
| VSCode | "他在工作!" | 安静陪伴、端茶 |
| 桌面图标 | 家具 | 在图标间穿梭、坐在图标上 |

### 实现路径 (渐进)

- **MVP**: 空间记忆 (P12) — 有窝, 自动回巢, 已实现
- **二期**: 窗口感知 (P14) 扩展为世界认知 — 她知道当前前台应用是什么, 有态度反应
- **三期**: 完整 Shared World — 桌面元素映射为地点, 她在其中自由活动, 形成空间记忆 Episode

### 设计意义

Shared World 把桌宠的"世界"从聊天框扩展到整个桌面。当她害怕回收站、喜欢陪你在 VSCode 工作、在图标间穿梭时, 用户会真正觉得"她住在我的电脑里"。

这不是新模块, 而是空间记忆 (P12) + 窗口感知 (P14) 的自然延伸。
