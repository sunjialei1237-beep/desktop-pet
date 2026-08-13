use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub embedding: EmbeddingConfig,
    pub app: AppConfigData,
    #[serde(default)]
    pub perception: PerceptionConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub proactive: ProactiveConfig,
}

/// LLM API configuration (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub main_model: String,
    pub reflection_model: String,
}

/// Local embedding model configuration (BGE-M3 via ONNX Runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub model_dir: String,
    pub model_name: String,
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
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            enable_reflection: true,
            enable_consolidation: true,
            enable_relationship_review: true,
            enable_lifecycle_cleanup: true,
            enable_rituals: true,
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
            },
            embedding: EmbeddingConfig {
                model_dir: String::new(),
                model_name: "bge-m3".to_string(),
            },
            app: AppConfigData {
                db_path: String::new(),
                debug: true,
                log_level: "info".to_string(),
            },
            perception: PerceptionConfig::default(),
            scheduler: SchedulerConfig::default(),
            proactive: ProactiveConfig::default(),
        }
    }
}

/// Proactive-bubble frequency control (Architecture Principle 6: every feature
/// must be disableable/tunable). Missing [proactive] section in older config
/// files uses the 30-minute default. Design doc 9.2: bubbles at most every
/// 30 minutes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveConfig {
    /// Minimum seconds between proactive bubbles (default 30 min).
    pub min_interval_secs: i64,
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        ProactiveConfig {
            min_interval_secs: 30 * 60,
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
}
