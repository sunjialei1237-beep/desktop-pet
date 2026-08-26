use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    /// Saved LLM profiles ("配置过的模型"): every `update_llm_config` records
    /// the endpoint here so the Settings panel can switch back with one
    /// click. Persisted as `[[llm_profiles]]` in config.toml (gitignored,
    /// same plaintext trust level as `llm.api_key`).
    #[serde(default)]
    pub llm_profiles: Vec<LlmProfile>,
    pub embedding: EmbeddingConfig,
    pub app: AppConfigData,
    #[serde(default)]
    pub perception: PerceptionConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub proactive: ProactiveConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub prompt: PromptConfig,
}

/// LLM API configuration (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub main_model: String,
    pub reflection_model: String,
    /// Per-role endpoint overrides (2026-08-26 cost routing). When present,
    /// gate classification / memory extraction calls go to their OWN
    /// provider+model instead of the main reflection_model — the cost lever:
    /// classification rides a cheap/free tier, rigor-critical extraction and
    /// the main reply stay on quality providers. Absent → current behavior.
    #[serde(default)]
    pub gate: Option<LlmRoleEndpoint>,
    #[serde(default)]
    pub extractor: Option<LlmRoleEndpoint>,
}

/// One role's endpoint override. Sections in config.toml:
/// `[llm.gate]` / `[llm.extractor]` with base_url + api_key + model.
/// A role section with an empty api_key falls back to the main key ONLY
/// when it shares the main base_url; cross-provider roles must carry their
/// own key (empty key + different host → role disabled, fallback used).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRoleEndpoint {
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub model: String,
}

/// One saved switchable LLM configuration (base_url + key + models).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProfile {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub main_model: String,
    pub reflection_model: String,
}

/// Upserts a profile into the list: an entry with the same endpoint
/// (base_url + models) is re-keyed in place; otherwise a new entry is pushed
/// with a collision-free name ("name (2)", "name (3)"…).
pub fn upsert_llm_profile(profiles: &mut Vec<LlmProfile>, mut profile: LlmProfile) {
    for p in profiles.iter_mut() {
        if p.base_url == profile.base_url
            && p.main_model == profile.main_model
            && p.reflection_model == profile.reflection_model
        {
            if !profile.api_key.is_empty() {
                p.api_key = profile.api_key.clone();
            }
            return;
        }
    }
    profile.name = unique_llm_profile_name(profiles, &profile.name);
    profiles.push(profile);
}

fn unique_llm_profile_name(profiles: &[LlmProfile], base: &str) -> String {
    let mut name = base.to_string();
    let mut n = 2usize;
    while profiles.iter().any(|p| p.name == name) {
        name = format!("{} ({})", base, n);
        n += 1;
    }
    name
}

/// Removes the named profile. Returns false when no entry had that name.
pub fn delete_llm_profile(profiles: &mut Vec<LlmProfile>, name: &str) -> bool {
    let before = profiles.len();
    profiles.retain(|p| p.name != name);
    before != profiles.len()
}

/// Local embedding model configuration (BGE-M3 via ONNX Runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub model_dir: String,
    pub model_name: String,
    /// P2 memory reduction: load the model on the first embed instead of at
    /// startup. Old configs without this key default to ON.
    #[serde(default = "default_lazy_load")]
    pub lazy_load: bool,
    /// P2 memory reduction: unload the model after this many idle minutes so
    /// an all-day-running pet doesn't hold ~870 MB while the user is away.
    /// 0 = keep resident once loaded. Scheduler paths (60min proactive window)
    /// will sawtooth reload — that is expected and harmless.
    #[serde(default = "default_idle_unload_minutes")]
    pub idle_unload_minutes: i64,
}

fn default_lazy_load() -> bool {
    true
}

fn default_idle_unload_minutes() -> i64 {
    30
}

/// General application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigData {
    pub db_path: String,
    pub debug: bool,
    pub log_level: String,
}

/// Perception layer toggles (Architecture Principle 6: every feature must be disableable).
/// Missing [perception] section in older config files uses all-enabled defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionConfig {
    pub enable_time: bool,
    pub enable_presence: bool,
    pub enable_window: bool,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        PerceptionConfig {
            enable_time: true,
            enable_presence: true,
            enable_window: true,
        }
    }
}

/// Scheduled Soul/cleanup "capability" toggles (Architecture Principle 6: every
/// capability must be disableable, and turning it off degrades gracefully —
/// "关掉 Reflection, 记忆照常"). Core aliveness (homeostasis / emotion push /
/// pending check) is NOT toggleable: disabling those kills her, which is not
/// graceful. Missing [scheduler] section in older config files uses all-enabled
/// defaults. See `lifecycle/scheduler.rs` + ADR 2026-08-08.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub enable_reflection: bool,
    pub enable_consolidation: bool,
    pub enable_relationship_review: bool,
    pub enable_lifecycle_cleanup: bool,
    pub enable_rituals: bool,
    pub enable_landmarks: bool,
    /// Sunday weekly recap. OFF by default (user decision 2026-08-16: 两个
    /// 朋友聊天不会每周复盘 — a scheduled recap reads as a tool, not a
    /// friend). Independent of `enable_rituals` (早安/晚安 unaffected); set
    /// true to restore it.
    #[serde(default = "default_false")]
    pub enable_weekly_summary: bool,
}

fn default_false() -> bool {
    false
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            enable_reflection: true,
            enable_consolidation: true,
            enable_relationship_review: true,
            enable_lifecycle_cleanup: true,
            enable_rituals: true,
            enable_landmarks: true,
            enable_weekly_summary: default_false(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            llm: LlmConfig {
                base_url: "https://api.deepseek.com/v1".to_string(),
                api_key: String::new(),
                main_model: "deepseek-v4-pro".to_string(),
                reflection_model: "deepseek-v4-flash".to_string(),
                gate: None,
                extractor: None,
            },
            llm_profiles: Vec::new(),
            embedding: EmbeddingConfig {
                model_dir: String::new(),
                model_name: "bge-m3".to_string(),
                lazy_load: default_lazy_load(),
                idle_unload_minutes: default_idle_unload_minutes(),
            },
            app: AppConfigData {
                db_path: String::new(),
                debug: true,
                log_level: "info".to_string(),
            },
            perception: PerceptionConfig::default(),
            scheduler: SchedulerConfig::default(),
            proactive: ProactiveConfig::default(),
            tools: ToolsConfig::default(),
            prompt: PromptConfig::default(),
        }
    }
}

/// Proactive-bubble frequency + content control (Architecture Principle 6: every
/// feature must be disableable/tunable). Missing [proactive] section in older
/// config files uses these defaults. Design doc 9.2: bubbles at most every
/// 30 minutes — raised to 60 (2026-08-14, user feedback: 频率太高).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveConfig {
    /// Minimum seconds between proactive bubbles (default 60 min).
    pub min_interval_secs: i64,
    /// Percent of no-pending proactive bubbles that anchor on a memory (0-100);
    /// the rest are lively, anchorless chatter (default 15: 85% 碎碎念).
    /// User feedback 2026-08-14: 不需要那么多消息带记忆.
    pub memory_bubble_ratio: i64,
    /// Route anchor selection through the LLM selector (flash tier): it sees
    /// the pre-vetted candidate pool + her last bubbles and may decline —
    /// silence instead of a forced trivial memory (2026-08-16 续⁴¹).
    /// false = the mechanical round-robin pick (pre-selector behavior).
    #[serde(default = "default_enable_llm_selector")]
    pub enable_llm_selector: bool,
}

fn default_enable_llm_selector() -> bool {
    true
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        ProactiveConfig {
            min_interval_secs: 60 * 60,
            memory_bubble_ratio: 15,
            enable_llm_selector: default_enable_llm_selector(),
        }
    }
}

/// Tool-layer toggles (Architecture Principle 6: every capability must be
/// disableable). `get_time` / `open_url` are harmless and have no switch
/// (always on); `search_web` / `open_application` can be turned off. Missing
/// [tools] section in older config files uses all-enabled defaults. See
/// `tools/mod.rs` + the tool-layer plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    pub enable_search_web: bool,
    pub enable_open_application: bool,
    /// Read-only filesystem observation tools (plan 2026-08-17 §2.7).
    /// Capability switch only — per-path authorization is the fs_grants
    /// table. Defaults OFF: V1 policy is Inspect = explicit opt-in.
    #[serde(default)]
    pub enable_fs_observe: bool,
    /// Write tools (plan 2026-08-17 §3.6 F1 create_note, F2 edit_file).
    /// Defaults OFF: Mutation = explicit opt-in, every write still gets a
    /// per-call confirmation (Principle #11).
    #[serde(default)]
    pub enable_fs_mutate: bool,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        ToolsConfig {
            enable_search_web: true,
            enable_open_application: true,
            enable_fs_observe: false,
            enable_fs_mutate: false,
        }
    }
}

/// Prompt-layout switches (Soul v2 plan L2a, Architecture Principle 6).
/// Missing [prompt] section in older config files uses the enabled default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptConfig {
    /// Inject time/mood/intent as a trailing system message after the
    /// conversation history (near-end directive, CCv2 post_history_instructions).
    /// false = exact v1 layout (runtime rollback without rebuild).
    pub near_end_directive: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        PromptConfig {
            near_end_directive: true,
        }
    }
}

/// Returns the app data directory for this application.
/// On Windows: %APPDATA%/DesktopPet
pub fn app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DesktopPet")
}

/// Returns the path to the user's config.toml.
pub fn config_path() -> PathBuf {
    app_data_dir().join("config.toml")
}

/// Resolves the database path. Empty string in config means default location.
pub fn resolve_db_path(config: &AppConfig) -> PathBuf {
    if config.app.db_path.is_empty() {
        app_data_dir().join("desktop_pet.db")
    } else {
        PathBuf::from(&config.app.db_path)
    }
}

/// Resolves the embedding model directory.
/// Empty string in config means default location under app data dir.
pub fn resolve_model_dir(config: &AppConfig) -> PathBuf {
    if config.embedding.model_dir.is_empty() {
        app_data_dir().join("models").join(&config.embedding.model_name)
    } else {
        PathBuf::from(&config.embedding.model_dir)
    }
}

/// Loads the configuration from config.toml.
/// If the file does not exist, copies it from the bundled example template
/// and returns the default values.
/// Saves configuration to config.toml.
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let config_file = config_path();
    let content = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&config_file, content)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    log::info!("Config saved to {:?}", config_file);
    Ok(())
}

/// Loads the configuration from config.toml.
/// If the file does not exist, copies it from the bundled example template
/// and returns the default values.
pub fn load_config() -> Result<AppConfig, String> {
    let config_dir = app_data_dir();
    let config_file = config_path();

    if !config_file.exists() {
        log::info!("Config not found, creating from template: {:?}", config_file);
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
        write_default_config(&config_file)?;
    }

    let content = fs::read_to_string(&config_file)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let mut config: AppConfig = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Merge with defaults for any missing fields
    apply_defaults(&mut config);

    Ok(config)
}

fn apply_defaults(config: &mut AppConfig) {
    let defaults = AppConfig::default();
    if config.llm.base_url.is_empty() {
        config.llm.base_url = defaults.llm.base_url;
    }
    if config.llm.main_model.is_empty() {
        config.llm.main_model = defaults.llm.main_model;
    }
    if config.llm.reflection_model.is_empty() {
        config.llm.reflection_model = defaults.llm.reflection_model;
    }
    if config.embedding.model_name.is_empty() {
        config.embedding.model_name = defaults.embedding.model_name;
    }
    if config.app.log_level.is_empty() {
        config.app.log_level = defaults.app.log_level;
    }
}

fn write_default_config(path: &Path) -> Result<(), String> {
    let template = include_str!("../resources/config.example.toml");
    fs::write(path, template)
        .map_err(|e| format!("Failed to write default config: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.llm.main_model, "deepseek-v4-pro");
        assert_eq!(config.embedding.model_name, "bge-m3");
        assert!(config.app.debug);
        // P2 memory reduction defaults: lazy on, unload after 30 idle minutes.
        assert!(config.embedding.lazy_load);
        assert_eq!(config.embedding.idle_unload_minutes, 30);
    }

    #[test]
    fn test_embedding_lazy_keys_optional_and_overridable() {
        // Old config (pre-P2) has no lazy keys -> serde defaults kick in.
        let old = r#"
[llm]
base_url = "https://api.deepseek.com/v1"
api_key = "k"
main_model = "m"
reflection_model = "m"

[embedding]
model_dir = "D:\\models"
model_name = "bge-m3"

[app]
db_path = ""
debug = true
log_level = "info"
"#;
        let config: AppConfig = toml::from_str(old).unwrap();
        assert!(config.embedding.lazy_load);
        assert_eq!(config.embedding.idle_unload_minutes, 30);

        // Explicit opt-out (eager + resident) parses.
        let eager = old.replace(
            "model_name = \"bge-m3\"",
            "model_name = \"bge-m3\"\nlazy_load = false\nidle_unload_minutes = 0",
        );
        let config: AppConfig = toml::from_str(&eager).unwrap();
        assert!(!config.embedding.lazy_load);
        assert_eq!(config.embedding.idle_unload_minutes, 0);
    }

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[llm]
base_url = "https://api.openai.com/v1"
api_key = "sk-test"
main_model = "gpt-4o-mini"
reflection_model = "gpt-4o-mini"

[embedding]
model_dir = "D:\\models"
model_name = "bge-m3"

[app]
db_path = ""
debug = false
log_level = "debug"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.llm.base_url, "https://api.openai.com/v1");
        assert_eq!(config.llm.api_key, "sk-test");
        assert_eq!(config.llm.main_model, "gpt-4o-mini");
        assert!(!config.app.debug);
        assert_eq!(config.embedding.model_dir, "D:\\models");
    }

    #[test]
    fn test_apply_defaults() {
        let toml_str = r#"
[llm]
base_url = ""
api_key = ""
main_model = ""
reflection_model = ""

[embedding]
model_dir = ""
model_name = ""

[app]
db_path = ""
debug = true
log_level = ""
"#;
        let mut config: AppConfig = toml::from_str(toml_str).unwrap();
        apply_defaults(&mut config);
        assert_eq!(config.llm.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.llm.main_model, "deepseek-v4-pro");
        assert_eq!(config.embedding.model_name, "bge-m3");
        assert_eq!(config.app.log_level, "info");
    }

    #[test]
    fn test_resolve_db_path_default() {
        let config = AppConfig::default();
        let path = resolve_db_path(&config);
        assert!(path.to_string_lossy().contains("desktop_pet.db"));
    }

    #[test]
    fn test_resolve_db_path_custom() {
        let mut config = AppConfig::default();
        config.app.db_path = "D:\\custom\\pet.db".to_string();
        let path = resolve_db_path(&config);
        assert_eq!(path, PathBuf::from("D:\\custom\\pet.db"));
    }

    #[test]
    fn test_llm_profiles_missing_section_defaults_empty() {
        // Old config.toml (pre-profiles) must keep parsing with an empty list.
        let old = r#"
[llm]
base_url = "https://api.deepseek.com/v1"
api_key = "k"
main_model = "m"
reflection_model = "m"

[embedding]
model_dir = ""
model_name = "bge-m3"

[app]
db_path = ""
debug = true
log_level = "info"
"#;
        let config: AppConfig = toml::from_str(old).unwrap();
        assert!(config.llm_profiles.is_empty());
    }

    #[test]
    fn test_llm_profiles_roundtrip() {
        let mut config = AppConfig::default();
        config.llm_profiles.push(LlmProfile {
            name: "glm-4.7".to_string(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key: "sk-1".to_string(),
            main_model: "glm-4.7".to_string(),
            reflection_model: "glm-4.7-flash".to_string(),
        });
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed.llm_profiles.len(), 1);
        assert_eq!(parsed.llm_profiles[0].name, "glm-4.7");
        assert_eq!(parsed.llm_profiles[0].api_key, "sk-1");
    }

    fn sample_profile(name: &str, base_url: &str, key: &str) -> LlmProfile {
        LlmProfile {
            name: name.to_string(),
            base_url: base_url.to_string(),
            api_key: key.to_string(),
            main_model: name.to_string(),
            reflection_model: format!("{}-flash", name),
        }
    }

    #[test]
    fn test_upsert_llm_profile_rekeys_same_endpoint() {
        let mut profiles = vec![sample_profile("m1", "https://a/v1", "sk-old")];
        upsert_llm_profile(&mut profiles, sample_profile("m1", "https://a/v1", "sk-new"));
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].api_key, "sk-new");
    }

    #[test]
    fn test_upsert_llm_profile_suffixes_name_collision() {
        // Same model name at a different provider = a distinct switchable
        // entry, not an overwrite.
        let mut profiles = vec![sample_profile("m1", "https://a/v1", "sk-a")];
        upsert_llm_profile(&mut profiles, sample_profile("m1", "https://b/v1", "sk-b"));
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[1].name, "m1 (2)");
        // And a third one keeps counting.
        upsert_llm_profile(&mut profiles, sample_profile("m1", "https://c/v1", "sk-c"));
        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[2].name, "m1 (3)");
    }

    #[test]
    fn test_delete_llm_profile() {
        let mut profiles = vec![sample_profile("m1", "https://a/v1", "sk-a")];
        assert!(delete_llm_profile(&mut profiles, "m1"));
        assert!(profiles.is_empty());
        assert!(!delete_llm_profile(&mut profiles, "m1"));
    }
}
