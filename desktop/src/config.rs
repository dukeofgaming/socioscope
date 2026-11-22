use std::fs;
use std::path::PathBuf;

use crate::library::repo_root;
use crate::okf::{self, Value};

#[derive(Clone, Debug)]
pub struct Models {
    pub whisper_path: String,
    pub diarization_id: String,
    pub token_set: bool,
    token: String,
}

impl Models {
    pub fn missing() -> Self {
        Self {
            whisper_path: String::new(),
            diarization_id: "pyannote/speaker-diarization-3.1".to_string(),
            token_set: false,
            token: String::new(),
        }
    }
}

pub fn path() -> PathBuf {
    repo_root().join("config.yaml")
}

pub fn load() -> Models {
    let Ok(text) = fs::read_to_string(path()) else {
        return Models::missing();
    };
    parse(&text)
}

pub fn parse(text: &str) -> Models {
    let map = okf::parse_map_text(text);
    let token = text_at(&map, "huggingface_token");
    let whisper = nested(&map, "models", "transcription", "path")
        .filter(|value| !value.is_empty())
        .or_else(|| nested(&map, "transcription", "model", "").filter(|value| !value.is_empty()))
        .unwrap_or_default();
    let diarization = nested(&map, "models", "diarization", "id")
        .filter(|value| !value.is_empty())
        .or_else(|| nested(&map, "diarization", "model", "").filter(|value| !value.is_empty()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "pyannote/speaker-diarization-3.1".to_string());
    Models {
        whisper_path: whisper,
        diarization_id: diarization,
        token_set: !token.is_empty() && !token.contains('<'),
        token,
    }
}

pub fn save(whisper_path: &str, diarization_id: &str, replacement_token: &str) -> Result<(), String> {
    let current = load();
    let token = {
        let replacement = replacement_token.trim();
        if replacement.is_empty() {
            current.token
        } else {
            replacement.to_string()
        }
    };
    let diarization_id = {
        let id = diarization_id.trim();
        if id.is_empty() {
            "pyannote/speaker-diarization-3.1".to_string()
        } else {
            id.to_string()
        }
    };
    let whisper_path = whisper_path.trim();
    let body = format!(
        "huggingface_token: {}\n\ntranscription:\n  model: {}\n\ndiarization:\n  model: {}\n\nmodels:\n  transcription:\n    name: Whisper\n    engine: whisper.cpp\n    path: {}\n  diarization:\n    name: pyannote speaker diarization\n    engine: pyannote.audio\n    id: {}\n",
        quote(&token),
        quote(whisper_path),
        quote(&diarization_id),
        quote(whisper_path),
        quote(&diarization_id),
    );
    fs::write(path(), body).map_err(|err| err.to_string())
}

pub fn whisper_status(whisper_path: &str) -> String {
    let whisper_path = whisper_path.trim();
    if whisper_path.is_empty() {
        return "No model file yet".to_string();
    }
    let path = PathBuf::from(whisper_path);
    let path = if path.is_absolute() { path } else { repo_root().join(path) };
    if path.is_file() {
        "File is on disk".to_string()
    } else {
        "File is not on disk".to_string()
    }
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn text_at(map: &[(String, Value)], key: &str) -> String {
    okf::field(map, key)
        .and_then(|value| match value {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn nested(map: &[(String, Value)], first: &str, second: &str, third: &str) -> Option<String> {
    let first = match okf::field(map, first)? {
        Value::Map(pairs) => pairs,
        _ => return None,
    };
    if third.is_empty() {
        return match okf::field(first, second)? {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        };
    }
    let second = match okf::field(first, second)? {
        Value::Map(pairs) => pairs,
        _ => return None,
    };
    match okf::field(second, third)? {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_and_models_sections() {
        let legacy = "huggingface_token: \"hf_test\"\ntranscription:\n  model: \"/models/ggml.bin\"\n";
        let parsed = parse(legacy);
        assert!(parsed.token_set);
        assert_eq!(parsed.whisper_path, "/models/ggml.bin");
        assert_eq!(parsed.diarization_id, "pyannote/speaker-diarization-3.1");

        let modern = "huggingface_token: \"<huggingface token>\"\nmodels:\n  transcription:\n    path: \"/opt/whisper.bin\"\n  diarization:\n    id: pyannote/speaker-diarization-3.1\n";
        let parsed = parse(modern);
        assert!(!parsed.token_set);
        assert_eq!(parsed.whisper_path, "/opt/whisper.bin");
        assert_eq!(parsed.diarization_id, "pyannote/speaker-diarization-3.1");
    }

    #[test]
    fn placeholder_token_is_not_set() {
        let parsed = parse("huggingface_token: \"\"\n");
        assert!(!parsed.token_set);
    }
}
