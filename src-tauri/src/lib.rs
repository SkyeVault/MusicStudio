mod commands;
mod model_manager;
mod process_manager;

use process_manager::ProcessManager;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

pub type AppState = Arc<Mutex<ProcessManager>>;

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let pm = ProcessManager::new(app.handle().clone());
            app.manage(Arc::new(Mutex::new(pm)) as AppState);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_sidecar,
            commands::stop_sidecar,
            commands::get_sidecar_statuses,
            commands::save_project,
            commands::load_project,
            commands::export_wav,
            commands::pick_export_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MusicStudio");
}
