use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    Missing,
    Downloading,
    Ready,
    Corrupt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub hf_repo: String,
    pub filename: String,
    pub size_mb: u64,
    pub sha256: Option<String>,
    pub status: ModelStatus,
    pub local_path: Option<PathBuf>,
    pub required: bool,
}

pub struct ModelManager {
    pub model_dir: PathBuf,
    pub models: HashMap<String, ModelEntry>,
}

impl ModelManager {
    pub fn new(model_dir: PathBuf) -> Self {
        let mut manager = Self {
            model_dir,
            models: HashMap::new(),
        };
        manager.register_known_models();
        manager.scan_local();
        manager
    }

    fn register_known_models(&mut self) {
        let entries = vec![
            ModelEntry {
                id: "demucs-htdemucs".into(),
                display_name: "Demucs v4 (Stem Separation)".into(),
                hf_repo: "facebook/demucs".into(),
                filename: "htdemucs.th".into(),
                size_mb: 85,
                sha256: None,
                status: ModelStatus::Missing,
                local_path: None,
                required: true,
            },
            ModelEntry {
                id: "musicgen-small".into(),
                display_name: "MusicGen Small (Music Generation)".into(),
                hf_repo: "facebook/musicgen-small".into(),
                filename: "model.safetensors".into(),
                size_mb: 1200,
                sha256: None,
                status: ModelStatus::Missing,
                local_path: None,
                required: false,
            },
            ModelEntry {
                id: "rvc-base".into(),
                display_name: "RVC v3 Base (Voice Cloning)".into(),
                hf_repo: "lj1995/VoiceConversionWebUI".into(),
                filename: "hubert_base.pt".into(),
                size_mb: 190,
                sha256: None,
                status: ModelStatus::Missing,
                local_path: None,
                required: false,
            },
        ];

        for entry in entries {
            self.models.insert(entry.id.clone(), entry);
        }
    }

    /// Scan model_dir and update statuses for any models found on disk.
    pub fn scan_local(&mut self) {
        for entry in self.models.values_mut() {
            let path = self.model_dir.join(&entry.id).join(&entry.filename);
            if path.exists() {
                entry.status = ModelStatus::Ready;
                entry.local_path = Some(path);
            }
        }
    }

    pub fn model_path(&self, id: &str) -> Option<&Path> {
        self.models.get(id)?.local_path.as_deref()
    }

    pub fn all_models(&self) -> Vec<&ModelEntry> {
        let mut v: Vec<_> = self.models.values().collect();
        v.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        v
    }
}
