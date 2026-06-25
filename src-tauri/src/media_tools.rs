use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::process::Command as AsyncCommand;

/// Drives `ffmpeg` directly for screen recording. This is a one-shot job
/// (start → stop → file on disk), not a polled HTTP service like the
/// Python sidecars, so it does not reuse `ProcessManager`'s health-check loop.
pub struct RecordingManager {
    child: Option<Child>,
    output_path: Option<String>,
}

impl RecordingManager {
    pub fn new() -> Self {
        Self { child: None, output_path: None }
    }

    pub fn is_recording(&self) -> bool {
        self.child.is_some()
    }

    /// Start capturing the X11 display (plus default audio input, if
    /// requested) to `output_path` via ffmpeg. `fps` controls capture rate.
    pub fn start(&mut self, output_path: &str, fps: u32, capture_audio: bool) -> Result<()> {
        if self.child.is_some() {
            return Err(anyhow!("A recording is already in progress"));
        }

        let (width, height) = screen_size()?;
        // Use the display this process actually runs on — hardcoding ":0.0"
        // fails outright on machines where the active session is on a
        // different display (e.g. ":1" under some VNC/remote-desktop setups),
        // since x11grab can't open a display that doesn't exist.
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0.0".to_string());

        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-f", "x11grab",
            "-framerate", &fps.to_string(),
            "-video_size", &format!("{width}x{height}"),
            "-i", &display,
        ]);

        if capture_audio {
            cmd.args(["-f", "pulse", "-i", "default"]);
        }

        cmd.args([
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-pix_fmt", "yuv420p",
        ]);
        if capture_audio {
            cmd.args(["-c:a", "aac"]);
        }
        cmd.arg(output_path);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| anyhow!("Failed to start ffmpeg: {e}"))?;

        // ffmpeg fails fast on a bad display/device — give it a moment and
        // check it's actually still alive before reporting success, instead
        // of letting the user record for a while and only finding out at
        // Stop that nothing was ever captured.
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Some(status) = child.try_wait().ok().flatten() {
            let mut stderr_text = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                stderr.read_to_string(&mut stderr_text).ok();
            }
            return Err(anyhow!(
                "ffmpeg exited immediately (status {status}): {}",
                stderr_text.trim()
            ));
        }

        self.child = Some(child);
        self.output_path = Some(output_path.to_string());
        Ok(())
    }

    /// Ask ffmpeg to finalize the file gracefully (sending 'q' over stdin,
    /// which ffmpeg treats as a clean-stop request) and wait for it to exit.
    pub fn stop(&mut self) -> Result<String> {
        let mut child = self.child.take().ok_or_else(|| anyhow!("No recording in progress"))?;
        let output_path = self.output_path.take().ok_or_else(|| anyhow!("No output path recorded"))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(b"q").ok();
        }

        let status = child.wait().map_err(|e| anyhow!("Failed to wait for ffmpeg to exit: {e}"))?;

        if !status.success() {
            let mut stderr_text = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                stderr.read_to_string(&mut stderr_text).ok();
            }
            return Err(anyhow!("ffmpeg exited with {status}: {}", stderr_text.trim()));
        }

        match std::fs::metadata(&output_path) {
            Ok(meta) if meta.len() > 0 => Ok(output_path),
            Ok(_) => Err(anyhow!("Recording finished but the output file is empty: {output_path}")),
            Err(e) => Err(anyhow!("Recording finished but the output file is missing: {output_path} ({e})")),
        }
    }
}

// --- MLT export/render pipeline -------------------------------------------
//
// We don't hand-roll ffmpeg filter graphs for multi-track compositing.
// Instead we describe the project as MLT XML (the same format Kdenlive
// saves) and let `melt` do the actual decode/composite/encode, since that's
// a mature, well-tested engine for exactly this job.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectJson {
    bpm: f64,
    tracks: Vec<TrackJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackJson {
    muted: bool,
    clips: Vec<ClipJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClipJson {
    start_beat: f64,
    duration_beats: f64,
    file_path: Option<String>,
    #[serde(rename = "type")]
    clip_type: String,
    video_in_point: Option<f64>,
}

const RENDER_FPS: u32 = 30;

fn seconds_to_frames(seconds: f64, fps: u32) -> i64 {
    (seconds * fps as f64).round().max(0.0) as i64
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build an MLT XML document from the project state, plus the total
/// timeline duration in seconds (used to compute render progress).
pub fn generate_mlt_xml(project_json: &serde_json::Value) -> Result<(String, f64)> {
    let project: ProjectJson =
        serde_json::from_value(project_json.clone()).map_err(|e| anyhow!("Invalid project JSON: {e}"))?;
    let seconds_per_beat = 60.0 / project.bpm;
    let fps = RENDER_FPS;

    let mut producers = String::new();
    let mut playlists = String::new();
    let mut track_refs = String::new();
    let mut producer_idx = 0usize;
    let mut playlist_idx = 0usize;
    let mut total_seconds = 0.0f64;

    for track in &project.tracks {
        if track.muted {
            continue;
        }
        let mut clips: Vec<&ClipJson> = track.clips.iter().filter(|c| c.file_path.is_some()).collect();
        if clips.is_empty() {
            continue;
        }
        clips.sort_by(|a, b| a.start_beat.partial_cmp(&b.start_beat).unwrap());

        let mut entries = String::new();
        let mut cursor_beats = 0.0f64;

        for clip in &clips {
            let gap_beats = clip.start_beat - cursor_beats;
            if gap_beats > 0.0 {
                let blank_frames = seconds_to_frames(gap_beats * seconds_per_beat, fps);
                if blank_frames > 0 {
                    entries.push_str(&format!("    <blank length=\"{blank_frames}\"/>\n"));
                }
            }

            let file_path = clip.file_path.as_ref().unwrap();
            let in_seconds = if clip.clip_type == "video" { clip.video_in_point.unwrap_or(0.0) } else { 0.0 };
            let in_frame = seconds_to_frames(in_seconds, fps);
            let duration_frames = seconds_to_frames(clip.duration_beats * seconds_per_beat, fps);
            let out_frame = (in_frame + duration_frames - 1).max(in_frame);

            let producer_id = format!("producer{producer_idx}");
            producer_idx += 1;
            producers.push_str(&format!(
                "  <producer id=\"{producer_id}\">\n    <property name=\"resource\">{}</property>\n  </producer>\n",
                xml_escape(file_path)
            ));
            entries.push_str(&format!(
                "    <entry producer=\"{producer_id}\" in=\"{in_frame}\" out=\"{out_frame}\"/>\n"
            ));

            cursor_beats = clip.start_beat + clip.duration_beats;
            total_seconds = total_seconds.max(cursor_beats * seconds_per_beat);
        }

        let playlist_id = format!("playlist{playlist_idx}");
        playlist_idx += 1;
        playlists.push_str(&format!(
            "  <playlist id=\"{playlist_id}\">\n{entries}  </playlist>\n"
        ));
        track_refs.push_str(&format!("    <track producer=\"{playlist_id}\"/>\n"));
    }

    if playlist_idx == 0 {
        return Err(anyhow!("No clips with media files to render"));
    }

    // Mix all tracks' audio together and composite video tracks in stacking
    // order. This covers straightforward stacking/mixdown; it does not
    // attempt transforms, blend modes, or transitions beyond a plain overlay.
    let mut transitions = String::new();
    for i in 1..playlist_idx {
        transitions.push_str(&format!(
            "    <transition mlt_service=\"mix\" a_track=\"0\" b_track=\"{i}\" always_active=\"1\" combine=\"1\"/>\n"
        ));
        transitions.push_str(&format!(
            "    <transition mlt_service=\"qtblend\" a_track=\"0\" b_track=\"{i}\"/>\n"
        ));
    }

    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<mlt>\n{producers}{playlists}  <tractor id=\"tractor0\">\n{track_refs}{transitions}  </tractor>\n</mlt>\n"
    );

    Ok((xml, total_seconds))
}

/// Render the project to `output_path` via `melt`, emitting `render-progress`
/// events (`{"progress": 0..100}`) on the given app handle as it runs.
pub async fn render_video_project(
    app: AppHandle,
    project_json: serde_json::Value,
    output_path: String,
) -> Result<String> {
    let (xml, total_seconds) = generate_mlt_xml(&project_json)?;
    let total_frames = seconds_to_frames(total_seconds, RENDER_FPS).max(1);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mlt_path = std::env::temp_dir().join(format!("musicstudio-render-{nanos}.mlt"));
    std::fs::write(&mlt_path, xml).map_err(|e| anyhow!("Failed to write MLT project: {e}"))?;

    let mut child = AsyncCommand::new("melt")
        .arg(&mlt_path)
        .arg("-consumer")
        .arg(format!("avformat:{output_path}"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to start melt: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let app_for_stdout = app.clone();
    let app_for_stderr = app.clone();

    let stdout_task = tokio::spawn(async move {
        if let Some(s) = stdout {
            stream_progress(s, app_for_stdout, total_frames).await;
        }
    });
    let stderr_task = tokio::spawn(async move {
        if let Some(s) = stderr {
            stream_progress(s, app_for_stderr, total_frames).await;
        }
    });

    let status = child.wait().await.map_err(|e| anyhow!("melt did not exit cleanly: {e}"))?;
    stdout_task.await.ok();
    stderr_task.await.ok();
    std::fs::remove_file(&mlt_path).ok();

    if !status.success() {
        return Err(anyhow!("melt exited with status {status}"));
    }

    app.emit("render-progress", serde_json::json!({ "progress": 100 })).ok();
    Ok(output_path)
}

/// Read a process stream incrementally and emit `render-progress` events
/// whenever a `percentage: N` token shows up (melt writes progress without
/// newlines, using carriage returns, so we scan raw bytes rather than lines).
async fn stream_progress<R: tokio::io::AsyncRead + Unpin>(mut reader: R, app: AppHandle, total_frames: i64) {
    let mut buf = [0u8; 4096];
    let mut acc = String::new();
    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        acc.push_str(&String::from_utf8_lossy(&buf[..n]));

        while let Some(pos) = acc.find("percentage:") {
            let rest = &acc[pos + "percentage:".len()..];
            let digits: String = rest.chars().skip_while(|c| c.is_whitespace()).take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(pct) = digits.parse::<u32>() {
                app.emit("render-progress", serde_json::json!({ "progress": pct.min(99) })).ok();
            }
            acc = acc[pos + "percentage:".len()..].to_string();
            if total_frames <= 0 {
                break;
            }
        }
        // Keep the accumulator from growing unbounded if no marker is found.
        if acc.len() > 8192 {
            acc.clear();
        }
    }
}

/// One row of the video-tools diagnostics report (`check_video_tools` command).
#[derive(serde::Serialize)]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub detail: String,
}

/// Probe for the external binaries the Video Studio features depend on, so
/// the UI can show a precise reason ("ffmpeg not found on PATH") instead of
/// a generic failure the first time a user clicks Record or Export.
pub fn check_video_tools() -> Vec<ToolStatus> {
    let checks: [(&str, &[&str]); 4] = [
        ("ffmpeg", &["-version"]),
        ("ffprobe", &["-version"]),
        ("melt", &["-version"]),
        ("xrandr", &["--version"]),
    ];

    checks
        .iter()
        .map(|(name, args)| match Command::new(name).args(*args).output() {
            Ok(out) if out.status.success() => {
                let first_line = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                ToolStatus { name: name.to_string(), available: true, detail: first_line }
            }
            Ok(out) => ToolStatus {
                name: name.to_string(),
                available: false,
                detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            },
            Err(e) => ToolStatus {
                name: name.to_string(),
                available: false,
                detail: format!("not found on PATH ({e})"),
            },
        })
        .collect()
}

/// Query the primary display's resolution via `xrandr`.
fn screen_size() -> Result<(u32, u32)> {
    let output = Command::new("xrandr")
        .arg("--current")
        .output()
        .map_err(|e| anyhow!("Failed to run xrandr: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout);

    for line in text.lines() {
        if line.contains('*') {
            if let Some(token) = line.split_whitespace().next() {
                if let Some((w, h)) = token.split_once('x') {
                    if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                        return Ok((w, h));
                    }
                }
            }
        }
    }
    Err(anyhow!("Could not determine screen resolution from xrandr output"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_project(video_path: &str, audio_path: &str) -> serde_json::Value {
        json!({
            "bpm": 120.0,
            "tracks": [
                {
                    "muted": false,
                    "clips": [{
                        "startBeat": 0.0,
                        "durationBeats": 4.0,
                        "filePath": video_path,
                        "type": "video",
                        "videoInPoint": 0.0
                    }]
                },
                {
                    "muted": false,
                    "clips": [{
                        "startBeat": 0.0,
                        "durationBeats": 4.0,
                        "filePath": audio_path,
                        "type": "audio"
                    }]
                }
            ]
        })
    }

    #[test]
    fn generates_valid_mlt_xml_with_two_tracks() {
        let project = sample_project("/tmp/clip1.mp4", "/tmp/clip1.wav");
        let (xml, total_seconds) = generate_mlt_xml(&project).unwrap();
        assert!(xml.contains("<mlt>"));
        assert!(xml.contains("producer0"));
        assert!(xml.contains("producer1"));
        assert!(xml.contains("qtblend"));
        assert!((total_seconds - 2.0).abs() < 0.01); // 4 beats @ 120bpm = 2s
    }

    #[test]
    fn records_a_real_short_clip_on_the_current_display() {
        if std::env::var("DISPLAY").is_err() {
            eprintln!("skipping: no DISPLAY in this environment");
            return;
        }
        let output_path = format!("/tmp/musicstudio-recording-test-{}.mp4", std::process::id());
        let mut rm = RecordingManager::new();
        rm.start(&output_path, 10, false).expect("recording should start on the current display");
        std::thread::sleep(std::time::Duration::from_millis(500));
        let result_path = rm.stop().expect("recording should stop cleanly and produce a file");
        assert_eq!(result_path, output_path);
        let meta = std::fs::metadata(&output_path).expect("output file should exist");
        assert!(meta.len() > 0, "output file should be non-empty");
        std::fs::remove_file(&output_path).ok();
    }
}
