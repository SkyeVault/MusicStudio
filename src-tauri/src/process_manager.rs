use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidecarId {
    Voice,
    AudioFx,
    SongGen,
    StemSep,
}

impl SidecarId {
    pub fn port(&self) -> u16 {
        match self {
            SidecarId::Voice => 8001,
            SidecarId::AudioFx => 8002,
            SidecarId::SongGen => 8003,
            SidecarId::StemSep => 8004,
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            SidecarId::Voice => "voice",
            SidecarId::AudioFx => "audio-fx",
            SidecarId::SongGen => "song-gen",
            SidecarId::StemSep => "stem-sep",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SidecarId::Voice => "voice",
            SidecarId::AudioFx => "audio-fx",
            SidecarId::SongGen => "song-gen",
            SidecarId::StemSep => "stem-sep",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidecarStatus {
    Stopped,
    Starting,
    Running,
    Degraded,
    Error,
}

struct SidecarProcess {
    child: Child,
    port: u16,
    started_at: Instant,
    status: SidecarStatus,
}

pub struct ProcessManager {
    processes: HashMap<String, SidecarProcess>,
    app: AppHandle,
    sidecars_dir: PathBuf,
    model_dir: PathBuf,
}

impl ProcessManager {
    pub fn new(app: AppHandle) -> Self {
        let dirs = directories::ProjectDirs::from("com", "musicstudio", "MusicStudio")
            .expect("could not determine app data dir");
        let model_dir = dirs.data_dir().join("models");
        std::fs::create_dir_all(&model_dir).ok();

        // Sidecars live next to the binary during development
        let sidecars_dir = std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .join("sidecars");

        Self {
            processes: HashMap::new(),
            app,
            sidecars_dir,
            model_dir,
        }
    }

    pub fn start(&mut self, id: &SidecarId) -> Result<()> {
        let key = id.as_str().to_string();
        if let Some(proc) = self.processes.get(&key) {
            if proc.status == SidecarStatus::Running || proc.status == SidecarStatus::Starting {
                return Ok(());
            }
        }

        let sidecar_dir = self.sidecars_dir.join(id.dir_name());
        let venv_python = sidecar_dir.join("venv/bin/python");
        let main_py = sidecar_dir.join("main.py");

        // Fall back to system python during development
        let python = if venv_python.exists() {
            venv_python
        } else {
            PathBuf::from("python3")
        };

        let port = id.port();
        log::info!("Starting sidecar {:?} on port {}", id, port);

        let child = std::process::Command::new(&python)
            .arg(&main_py)
            .env("PORT", port.to_string())
            .env("MODEL_DIR", &self.model_dir)
            .env("HF_HUB_OFFLINE", "0") // allow downloads until models are cached
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn {:?}: {e}", id))?;

        self.processes.insert(
            key.clone(),
            SidecarProcess {
                child,
                port,
                started_at: Instant::now(),
                status: SidecarStatus::Starting,
            },
        );

        // Emit status event to frontend
        self.emit_status(id.as_str(), &SidecarStatus::Starting);

        // Spawn a background health-check task
        let app = self.app.clone();
        let id_str = id.as_str().to_string();
        tauri::async_runtime::spawn(async move {
            Self::health_check_loop(app, id_str, port).await;
        });

        Ok(())
    }

    pub fn stop(&mut self, id: &SidecarId) -> Result<()> {
        let key = id.as_str().to_string();
        if let Some(mut proc) = self.processes.remove(&key) {
            log::info!("Stopping sidecar {:?}", id);
            proc.child.kill().ok();
            proc.child.wait().ok();
            self.emit_status(id.as_str(), &SidecarStatus::Stopped);
        }
        Ok(())
    }

    pub fn statuses(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (key, proc) in &self.processes {
            let s = match proc.status {
                SidecarStatus::Running => "running",
                SidecarStatus::Starting => "starting",
                SidecarStatus::Degraded => "degraded",
                SidecarStatus::Error => "error",
                SidecarStatus::Stopped => "stopped",
            };
            map.insert(key.clone(), s.to_string());
        }
        map
    }

    fn emit_status(&self, id: &str, status: &SidecarStatus) {
        let payload = serde_json::json!({ "id": id, "status": status });
        self.app
            .emit("sidecar-status", payload)
            .map_err(|e| log::warn!("Failed to emit sidecar-status: {e}"))
            .ok();
    }

    /// Poll `/health` endpoint until the sidecar responds or times out.
    async fn health_check_loop(app: AppHandle, id: String, port: u16) {
        let url = format!("http://127.0.0.1:{port}/health");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();

        // Wait up to 30 seconds for initial startup
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut confirmed_running = false;

        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false) {
                let payload = serde_json::json!({ "id": id, "status": "running" });
                app.emit("sidecar-status", payload).ok();
                confirmed_running = true;
                break;
            }
        }

        if !confirmed_running {
            let payload = serde_json::json!({ "id": id, "status": "error" });
            app.emit("sidecar-status", payload).ok();
            return;
        }

        // Continue polling every 5 seconds
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let ok = client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false);
            let status = if ok { "running" } else { "degraded" };
            let payload = serde_json::json!({ "id": id, "status": status });
            app.emit("sidecar-status", payload).ok();

            if !ok {
                log::warn!("Sidecar {id} health check failed — marked degraded");
                break;
            }
        }
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        for (_, mut proc) in self.processes.drain() {
            proc.child.kill().ok();
        }
    }
}
