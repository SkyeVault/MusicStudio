mod commands;
mod media_tools;
mod model_manager;
mod process_manager;

use media_tools::RecordingManager;
use process_manager::ProcessManager;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

pub type AppState = Arc<Mutex<ProcessManager>>;
pub type RecordingState = Arc<Mutex<RecordingManager>>;

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let pm = ProcessManager::new(app.handle().clone());
            app.manage(Arc::new(Mutex::new(pm)) as AppState);
            app.manage(Arc::new(Mutex::new(RecordingManager::new())) as RecordingState);

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
            commands::probe_media_duration,
            commands::generate_video_thumbnail,
            commands::start_screen_recording,
            commands::stop_screen_recording,
            commands::render_video_project,
            commands::check_video_tools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MusicStudio");
}
