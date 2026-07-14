mod commands;
mod config;
mod db;
mod events;
mod llm;
mod emotion;
mod embedding;
mod mind;
mod pending;

use commands::AppState;
use std::sync::Mutex;
use tauri::Emitter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    let config = match config::load_config() {
        Ok(c) => {
            log::info!("Configuration loaded from {:?}", config::config_path());
            c
        }
        Err(e) => {
            log::error!("Failed to load config: {}. Using defaults.", e);
            config::AppConfig::default()
        }
    };

    let db_path = config::resolve_db_path(&config);
    let db_state = match db::DbState::open(&db_path) {
        Ok(db) => {
            log::info!("Database opened at {:?}", db_path);
            db
        }
        Err(e) => {
            log::error!("Failed to open database: {}. Falling back to in-memory.", e);
            db::DbState::open(std::path::Path::new(":memory:"))
                .expect("Failed to open in-memory database")
        }
    };

    log::info!(
        "DesktopPet starting up | debug={} | log_level={}",
        config.app.debug,
        config.app.log_level
    );

    // Initialize LLM client if configured.
    let llm = crate::llm::client::LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .ok();
    if llm.is_some() {
        log::info!("LLM client initialized (model: {})", config.llm.main_model);
    } else {
        log::warn!("LLM not configured — conversation will fail until API key is set");
    }

    let working_memory = Mutex::new(crate::mind::working::WorkingMemory::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            config,
            llm,
            working_memory,
        })
        .manage(db_state)
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_emotion_state,
            commands::get_debug_data,
            commands::pet_head,
            commands::poke,
            commands::check_proactive,
            commands::get_llm_status,
            commands::resolve_pending_event,
            commands::get_llm_config,
            commands::update_llm_config,
        ])
        .setup(|app| {
            log::info!("Tauri app initialized");
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = handle.emit("bubble-show", "ni hao ya! wo shi ni de zhuo chong.");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
