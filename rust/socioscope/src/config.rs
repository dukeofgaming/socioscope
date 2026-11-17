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
}

#[derive(Debug, Deserialize)]
pub struct DiarizationConfig {
    pub segmentation_model: String,
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
        Self::load_from(Path::new("config.yaml"))
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
