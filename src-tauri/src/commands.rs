use crate::process_manager::SidecarId;
use crate::{AppState, RecordingState};
use std::collections::HashMap;
use tauri::{AppHandle, State};

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

/// Probe a media file's duration (seconds) via ffprobe.
#[tauri::command]
pub async fn probe_media_duration(file_path: String) -> Result<f64, String> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &file_path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|e| format!("Failed to parse ffprobe duration: {e}"))
}

/// Extract a single frame from a video file as a thumbnail JPEG, saved next to
/// the app's data dir under `thumbnails/`. Returns the thumbnail's absolute path.
#[tauri::command]
pub async fn generate_video_thumbnail(file_path: String) -> Result<String, String> {
    let dirs = directories::ProjectDirs::from("com", "musicstudio", "MusicStudio")
        .ok_or("Could not find data dir")?;
    let thumb_dir = dirs.data_dir().join("thumbnails");
    std::fs::create_dir_all(&thumb_dir).map_err(|e| e.to_string())?;

    let stem = std::path::Path::new(&file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");
    let thumb_path = thumb_dir.join(format!("{stem}-{}.jpg", uuid_like_suffix()));

    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-ss", "0.5",
            "-i", &file_path,
            "-frames:v", "1",
            "-vf", "scale=160:-1",
        ])
        .arg(&thumb_path)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffmpeg thumbnail extraction failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(thumb_path.to_string_lossy().to_string())
}

/// Small unique suffix so repeated thumbnail requests for the same source
/// file don't collide or get served stale from disk.
fn uuid_like_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// Start recording the screen to `output_path` via ffmpeg (x11grab + optional
/// default pulse audio input). One recording at a time.
#[tauri::command]
pub async fn start_screen_recording(
    output_path: String,
    fps: u32,
    capture_audio: bool,
    state: State<'_, RecordingState>,
) -> Result<(), String> {
    let mut rm = state.lock().await;
    rm.start(&output_path, fps, capture_audio).map_err(|e| e.to_string())
}

/// Stop the in-flight screen recording and return the finalized file path.
#[tauri::command]
pub async fn stop_screen_recording(state: State<'_, RecordingState>) -> Result<String, String> {
    let mut rm = state.lock().await;
    rm.stop().map_err(|e| e.to_string())
}

/// Render the full project (audio + video tracks) to a single output file
/// via the MLT (`melt`) engine. Emits `render-progress` events as it runs.
#[tauri::command]
pub async fn render_video_project(
    app: AppHandle,
    project: serde_json::Value,
    output_path: String,
) -> Result<String, String> {
    crate::media_tools::render_video_project(app, project, output_path)
        .await
        .map_err(|e| e.to_string())
}

/// Report whether ffmpeg/ffprobe/melt/xrandr are available, so the frontend
/// can show a precise diagnostic instead of a generic error the first time
/// a video feature (recording, export, thumbnails) is used.
#[tauri::command]
pub async fn check_video_tools() -> Result<Vec<crate::media_tools::ToolStatus>, String> {
    Ok(crate::media_tools::check_video_tools())
}

fn parse_sidecar_id(s: &str) -> Result<SidecarId, String> {
    match s {
        "voice" => Ok(SidecarId::Voice),
        "audio-fx" => Ok(SidecarId::AudioFx),
        "song-gen" => Ok(SidecarId::SongGen),
        "stem-sep" => Ok(SidecarId::StemSep),
        "video-ai" => Ok(SidecarId::VideoAi),
        other => Err(format!("Unknown sidecar id: {other}")),
    }
}
