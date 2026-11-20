use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub huggingface_token: Option<String>,
    pub transcription: TranscriptionConfig,
    #[serde(default)]
    pub diarization: Option<DiarizationConfig>,
}

#[derive(Debug, Deserialize)]
pub struct TranscriptionConfig {
    pub model: String,
    /// whisper, cactus, audiocpp, or desertant. Empty means whisper.
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub family: String,
}

#[derive(Debug, Deserialize)]
pub struct DiarizationConfig {
    /// sherpa, audiocpp, or desertant. Empty means sherpa.
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub family: String,
    /// Single weight file, or the model directory for Desert Ant.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub segmentation_model: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default = "default_num_clusters")]
    pub num_clusters: i32,
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

fn default_num_clusters() -> i32 {
    -1
}

fn default_threshold() -> f32 {
    0.5
}

impl Config {
    /// `config.yaml` in the working directory, same place the Python code opens it.
    pub fn load() -> Result<Self, Error> {
        let path = std::env::var("SOCIOSCOPE_CONFIG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| Path::new("config.yaml").to_path_buf());
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::new(format!(
                    "[Errno 2] No such file or directory: '{}'",
                    path.display()
                ))
            } else {
                Error::new(error.to_string())
            }
        })?;
        serde_yaml::from_str(&text).map_err(|error| Error::new(error.to_string()))
    }
}
