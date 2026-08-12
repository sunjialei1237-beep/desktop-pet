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
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

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
        // P11.4 Alt+Space: a true system-wide global shortcut that summons the
        // pet to talk from any app. The handler shows+focuses the window (in
        // case it's hidden to the tray) and tells the frontend to open the chat
        // input. The shortcut itself is registered in setup() below.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        let _ = app.emit("show-input", ());
                    }
                })
                .build(),
        )
        .manage(AppState {
            config,
            llm: std::sync::Mutex::new(llm_client),
            embedding: embedding_service,
            working_memory,
            question_pacing: Default::default(),
            last_decision: std::sync::Mutex::new(None),
            last_proactive_bubble: std::sync::Mutex::new(None),
            pending_forget: std::sync::Mutex::new(None),
            clickthrough_diag: std::sync::Mutex::new(None),
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

            // Backfill embeddings for episodes stored before the model was
            // available (first time BGE-M3 is enabled on an existing DB). Runs
            // on a background thread so it never blocks startup.
            if let Some(app_state) = app.try_state::<crate::commands::AppState>() {
                if app_state.embedding.is_ready() {
                    let h = app.handle().clone();
                    std::thread::spawn(move || {
                        let state = h.state::<crate::commands::AppState>();
                        let db = h.state::<db::DbState>();
                        match crate::mind::store::backfill_missing_vectors(&db, &state.embedding) {
                            Ok(n) if n > 0 => {
                                log::info!("[startup] backfilled {} episode vector(s)", n)
                            }
                            Ok(_) => log::info!("[startup] no episode vectors needed backfilling"),
                            Err(e) => {
                                log::warn!("[startup] episode vector backfill failed: {}", e)
                            }
                        }
                    });
                }
            }

            // Register the Alt+Space global shortcut (P11.4). The handler was
            // wired in the plugin builder above; this binds the actual key combo.
            // Failure is non-fatal (logged) — the pet still works, just without
            // the hotkey (e.g. if another app already owns Alt+Space).
            let alt_space = Shortcut::new(Some(Modifiers::ALT), Code::Space);
            if let Err(e) = app.global_shortcut().register(alt_space) {
                log::warn!("[global-shortcut] failed to register Alt+Space: {}", e);
            } else {
                log::info!("[global-shortcut] Alt+Space registered (summon pet to talk)");
            }

            // Start the life loop (background timers).
            lifecycle::start_life_loop(handle.clone());

            // Start the global cursor poll thread for click-through (ADR Phase 2).
            let _cursor_stop = perception::cursor::start(handle.clone());

            // Start the deep-focus sampler (plan P14.3): tracks sustained same
            // Work-app foreground time so proactive bubbles stay quiet during
            // deep focus. Independent of Mind/LLM (Principle 5).
            perception::focus::start();

            // System tray icon. Lets the pet hide to the tray ("暂时离开" in
            // the right-click menu -> hide_to_tray) and restore on click. The
            // Click handler re-shows the window and emits "restore-from-tray"
            // so the front-end clears its awayMode flag.
            let tray_icon = app.default_window_icon().cloned();
            if let Some(icon) = tray_icon {
                if let Err(e) = tauri::tray::TrayIconBuilder::with_id("main-tray")
                    .icon(icon)
                    .tooltip("桌面宠物 · 点击图标恢复")
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                            let _ = app.emit("restore-from-tray", ());
                        }
                    })
                    .build(app)
                {
                    log::warn!("[tray] failed to build system tray icon: {}", e);
                }
            } else {
                log::warn!("[tray] no default window icon available, skipping tray");
            }

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
            commands::welcome_back_bubble,
            commands::lonely_bubble,
            commands::get_llm_status,
            commands::resolve_pending_event,
            commands::get_llm_config,
            commands::update_llm_config,
            commands::get_debug_snapshot,
            commands::set_clickthrough_diag,
            commands::get_clickthrough_diag,
            commands::get_scheduler_stats,
            commands::forget_fact,
            commands::delete_episode,
            commands::set_emotion,
            commands::get_embedding_status,
            commands::download_embedding_model,
           commands::check_cold_start,
           commands::needs_onboarding,
           commands::save_onboarding_answer,
           commands::complete_onboarding,
           commands::get_user_profile,
           commands::open_devtools,
           commands::open_debug_window,
           commands::quit_app,
           commands::hide_to_tray,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
