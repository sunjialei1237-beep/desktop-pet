mod commands;
mod config;
mod events;

use commands::AppState;
use tauri::Emitter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    // Load configuration (auto-creates from template if missing)
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

    log::info!(
        "DesktopPet starting up | debug={} | log_level={}",
        config.app.debug,
        config.app.log_level
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { config })
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_emotion_state,
            commands::get_debug_data,
            commands::pet_head,
            commands::poke,
        ])
        .setup(|app| {
            log::info!("Tauri app initialized");
            // Greeting: delayed bubble on a background thread
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
