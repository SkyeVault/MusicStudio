use crate::process_manager::SidecarId;
use crate::AppState;
use std::collections::HashMap;
use tauri::State;

/// Start a sidecar by ID string (matches SidecarId kebab-case names).
#[tauri::command]
pub async fn start_sidecar(sidecar_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = parse_sidecar_id(&sidecar_id)?;
    let mut pm = state.lock().await;
    pm.start(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_sidecar(sidecar_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let id = parse_sidecar_id(&sidecar_id)?;
    let mut pm = state.lock().await;
    pm.stop(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_sidecar_statuses(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let pm = state.lock().await;
    Ok(pm.statuses())
}

/// Save project JSON to disk. The frontend passes the full project state.
#[tauri::command]
pub async fn save_project(project: serde_json::Value, path: Option<String>) -> Result<String, String> {
    let save_path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let dirs = directories::ProjectDirs::from("com", "musicstudio", "MusicStudio")
                .ok_or("Could not find data dir")?;
            let dir = dirs.data_dir().join("projects");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            dir.join("project.json")
        }
    };

    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    std::fs::write(&save_path, json).map_err(|e| e.to_string())?;
    Ok(save_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn load_project(path: String) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

/// Export a mix of audio clips to WAV by merging them client-side via the browser's
/// Web Audio API. This command just picks a save path via the OS dialog.
/// The actual rendering is done in the frontend using OfflineAudioContext,
/// then the bytes are passed back here to write to disk.
#[tauri::command]
pub async fn export_wav(output_path: String, audio_data: Vec<u8>) -> Result<String, String> {
    std::fs::write(&output_path, &audio_data).map_err(|e| e.to_string())?;
    Ok(output_path)
}

/// Open a file-picker dialog for the export path (WAV).
#[tauri::command]
pub async fn pick_export_path() -> Result<Option<String>, String> {
    // Frontend uses @tauri-apps/plugin-dialog directly; this is a no-op shim
    // kept for potential future use from Rust context.
    Ok(None)
}

fn parse_sidecar_id(s: &str) -> Result<SidecarId, String> {
    match s {
        "voice" => Ok(SidecarId::Voice),
        "audio-fx" => Ok(SidecarId::AudioFx),
        "song-gen" => Ok(SidecarId::SongGen),
        "stem-sep" => Ok(SidecarId::StemSep),
        other => Err(format!("Unknown sidecar id: {other}")),
    }
}
