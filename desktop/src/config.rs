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

#[derive(Clone, Debug, PartialEq)]
pub struct PluginSettings {
    pub fallacies: bool,
    pub dsm: bool,
    pub laya_path: String,
    pub laya_command: String,
    pub bonsai_path: String,
    pub bonsai_endpoint: String,
}

impl PluginSettings {
    pub fn fresh() -> Self {
        Self {
            fallacies: true,
            dsm: true,
            laya_path: String::new(),
            laya_command: "laya".into(),
            bonsai_path: String::new(),
            bonsai_endpoint: String::new(),
        }
    }
}

pub fn load_plugins() -> PluginSettings {
    let Ok(text) = fs::read_to_string(path()) else {
        return PluginSettings::fresh();
    };
    plugins_from(&text)
}

pub fn plugins_from(text: &str) -> PluginSettings {
    let map = okf::parse_map_text(text);
    let mut settings = PluginSettings::fresh();
    settings.fallacies = flag(&map, "fallacies");
    settings.dsm = flag(&map, "dsm");
    if let Some(path) = nested(&map, "engines", "laya", "path") {
        settings.laya_path = path;
    }
    if let Some(command) = nested(&map, "engines", "laya", "command") {
        if !command.trim().is_empty() {
            settings.laya_command = command;
        }
    }
    if let Some(path) = nested(&map, "engines", "bonsai", "path") {
        settings.bonsai_path = path;
    }
    if let Some(endpoint) = nested(&map, "engines", "bonsai", "endpoint") {
        settings.bonsai_endpoint = endpoint;
    }
    settings
}

pub fn save_studio(
    whisper_path: &str,
    diarization_id: &str,
    replacement_token: &str,
    plugins: &PluginSettings,
) -> Result<(), String> {
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
    let body = render(&token, whisper_path, &diarization_id, plugins);
    fs::write(path(), body).map_err(|err| err.to_string())
}

pub fn render(token: &str, whisper_path: &str, diarization_id: &str, plugins: &PluginSettings) -> String {
    format!(
        "huggingface_token: {}\n\ntranscription:\n  model: {}\n\ndiarization:\n  model: {}\n\nmodels:\n  transcription:\n    name: Whisper\n    engine: whisper.cpp\n    path: {}\n  diarization:\n    name: pyannote speaker diarization\n    engine: pyannote.audio\n    id: {}\n\nplugins:\n  fallacies:\n    enabled: {}\n  dsm:\n    enabled: {}\n\nengines:\n  laya:\n    path: {}\n    command: {}\n  bonsai:\n    path: {}\n    endpoint: {}\n",
        quote(token),
        quote(whisper_path),
        quote(diarization_id),
        quote(whisper_path),
        quote(diarization_id),
        if plugins.fallacies { "true" } else { "false" },
        if plugins.dsm { "true" } else { "false" },
        quote(&plugins.laya_path),
        quote(&plugins.laya_command),
        quote(&plugins.bonsai_path),
        quote(&plugins.bonsai_endpoint),
    )
}

fn flag(map: &[(String, okf::Value)], plugin: &str) -> bool {
    match nested(map, "plugins", plugin, "enabled").as_deref() {
        None => true,
        Some("false" | "no" | "0") => false,
        Some(_) => true,
    }
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

    #[test]
    fn plugins_default_on_and_can_be_turned_off() {
        let fresh = plugins_from("huggingface_token: \"\"\n");
        assert!(fresh.fallacies);
        assert!(fresh.dsm);
        assert_eq!(fresh.laya_command, "laya");
        let off = plugins_from(
            "plugins:\n  fallacies:\n    enabled: false\n  dsm:\n    enabled: true\nengines:\n  laya:\n    path: \"/models/laya-f16.gguf\"\n    command: laya\n  bonsai:\n    endpoint: \"http://127.0.0.1:8080\"\n",
        );
        assert!(!off.fallacies);
        assert!(off.dsm);
        assert_eq!(off.laya_path, "/models/laya-f16.gguf");
        assert_eq!(off.bonsai_endpoint, "http://127.0.0.1:8080");
        let rendered = render("", "/models/ggml.bin", "pyannote/speaker-diarization-3.1", &off);
        let round = plugins_from(&rendered);
        assert!(!round.fallacies);
        assert_eq!(round.laya_path, off.laya_path);
    }
}
