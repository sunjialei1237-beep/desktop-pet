pub mod commands;
pub mod config;
pub mod db;
pub mod events;
pub mod llm;
pub mod emotion;
pub mod embedding;
pub mod mind;
pub mod pending;
pub mod lifecycle;
pub mod soul;
pub mod perception;

use commands::AppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

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
    let llm_client = crate::llm::client::LlmClient::new(
        &config.llm.base_url,
        &config.llm.api_key,
        &config.llm.main_model,
        &config.llm.reflection_model,
    )
    .ok();
    if llm_client.is_some() {
        log::info!("LLM client initialized (model: {})", config.llm.main_model);
    } else {
        log::warn!("LLM not configured — conversation will fail until API key is set");
    }

    let working_memory = Mutex::new(crate::mind::working::WorkingMemory::new());

    // Initialize embedding service. Try to load if model files already exist.
    let model_dir = config::resolve_model_dir(&config);
    let embedding_service = crate::embedding::EmbeddingService::new(&model_dir);
    {
        let downloader = crate::embedding::ModelDownloader::new(&model_dir);
        if downloader.check_complete() {
            match embedding_service.load() {
                Ok(()) => log::info!("Embedding model loaded from {:?}", model_dir),
                Err(e) => log::warn!("Failed to load embedding model: {}", e),
            }
        } else {
            log::info!(
                "Embedding model not found at {:?}. Use Settings to download.",
                model_dir
            );
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            config,
            llm: std::sync::Mutex::new(llm_client),
            embedding: embedding_service,
            working_memory,
            question_pacing: Default::default(),
        })
        .manage(db_state)
        .setup(|app| {
            log::info!("Tauri app initialized");

            let handle = app.handle().clone();

            // First run checks: seed persona traits if needed.
            if let Some(db_state) = app.try_state::<db::DbState>() {
                match lifecycle::run_firstrun_checks(&db_state) {
                    Ok(true) => log::info!("First run initialization completed"),
                    Ok(false) => log::info!("Not first run, skipping initialization"),
                    Err(e) => log::error!("First run check failed: {}", e),
                }
            }

            // Start the life loop (background timers).
            lifecycle::start_life_loop(handle.clone());

            // Start the global cursor poll thread for click-through (ADR Phase 2).
            let _cursor_stop = perception::cursor::start(handle.clone());

            // Welcome bubble after a short delay.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = handle.emit("bubble-show", "你好呀！我是你的桌宠。");
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_emotion_state,
            commands::get_perception,
            commands::trigger_reflection_if_due, 
            commands::get_pending_thoughts,       
            commands::force_reflection,
           commands::get_debug_data,
            commands::pet_head,
            commands::poke,
            commands::check_proactive,
            commands::proactive_bubble,
            commands::get_llm_status,
            commands::resolve_pending_event,
            commands::get_llm_config,
            commands::update_llm_config,
            commands::get_debug_snapshot,
            commands::get_embedding_status,
            commands::download_embedding_model,
           commands::check_cold_start,
           commands::needs_onboarding,
           commands::save_onboarding_answer,
           commands::complete_onboarding,
           commands::get_user_profile,
           commands::open_devtools,
           commands::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
