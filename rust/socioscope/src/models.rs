//! Curated transcription and diarization weights.
//!
//! The CLI downloads and selects them. The web app calls the CLI.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Error;

const CATALOG: &str = include_str!("../models/catalog.yaml");

#[derive(Debug, Deserialize, Clone)]
pub struct Catalog {
    pub note: String,
    pub models: Vec<Model>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub role: String,
    pub badge: String,
    pub runtime: String,
    pub summary: String,
    pub license: String,
    pub released: String,
    pub runnable: bool,
    pub files: Vec<Weight>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Weight {
    pub url: String,
    pub filename: String,
    pub bytes: u64,
    pub slot: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub role: String,
    pub badge: String,
    pub runtime: String,
    pub runnable: bool,
    pub summary: String,
    pub license: String,
    pub released: String,
    pub size: String,
    pub state: String,
    pub active: bool,
    pub received: u64,
    pub total: u64,
    pub detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Shelf {
    pub note: String,
    pub models: Vec<Listed>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Active {
    #[serde(default)]
    transcription: String,
    #[serde(default)]
    diarization: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Progress {
    state: String,
    received: u64,
    total: u64,
    detail: String,
    updated_ms: u64,
}

pub fn catalog() -> Result<Catalog, Error> {
    serde_yaml::from_str(CATALOG).map_err(|error| Error::new(error.to_string()))
}

pub fn models_dir_from_env() -> PathBuf {
    std::env::var("SOCIOSCOPE_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("socioscope-models"))
}

pub fn config_path_from_env() -> PathBuf {
    std::env::var("SOCIOSCOPE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config.yaml"))
}

pub fn list(dir: &Path) -> Result<Shelf, Error> {
    let catalog = catalog()?;
    let active = read_active(dir);
    let models = catalog
        .models
        .iter()
        .map(|model| listed(model, dir, &active))
        .collect();
    Ok(Shelf {
        note: catalog.note,
        models,
    })
}

pub fn download(id: &str, dir: &Path) -> Result<String, Error> {
    let model = find(id)?;
    download_model(&model, dir)?;
    Ok(format!("Downloaded {}.", model.name))
}

pub fn activate(id: &str, dir: &Path, config_path: &Path) -> Result<String, Error> {
    let model = find(id)?;
    activate_model(&model, dir, config_path)
}

fn find(id: &str) -> Result<Model, Error> {
    catalog()?
        .models
        .into_iter()
        .find(|model| model.id == id)
        .ok_or_else(|| Error::new(format!("No model named {id}.")))
}

fn listed(model: &Model, dir: &Path, active: &Active) -> Listed {
    let total = model.files.iter().map(|file| file.bytes).sum();
    let received = model
        .files
        .iter()
        .map(|file| {
            placed(dir, &model.id, &file.filename)
                .ok()
                .and_then(|path| fs::metadata(path).ok())
                .map(|meta| meta.len().min(file.bytes))
                .unwrap_or(0)
        })
        .sum();
    let ready = model.files.iter().all(|file| file_ready(dir, &model.id, file));
    let progress = read_progress(dir, &model.id);
    let (state, detail) = if ready {
        ("ready".to_string(), String::new())
    } else if let Some(progress) = progress {
        let fresh = now_ms().saturating_sub(progress.updated_ms) < 20_000;
        if progress.state == "downloading" && fresh {
            ("downloading".to_string(), progress.detail)
        } else if progress.state == "failed" {
            ("failed".to_string(), progress.detail)
        } else if received > 0 {
            ("failed".to_string(), "The download stopped.".into())
        } else {
            ("missing".to_string(), String::new())
        }
    } else {
        ("missing".to_string(), String::new())
    };
    let chosen = match model.role.as_str() {
        "transcription" => active.transcription == model.id,
        "diarization" => active.diarization == model.id,
        _ => false,
    };
    Listed {
        id: model.id.clone(),
        name: model.name.clone(),
        role: model.role.clone(),
        badge: model.badge.clone(),
        runtime: model.runtime.clone(),
        runnable: model.runnable,
        summary: model.summary.clone(),
        license: model.license.clone(),
        released: model.released.clone(),
        size: format_size(total),
        state,
        active: chosen && ready,
        received,
        total,
        detail,
    }
}

fn download_model(model: &Model, dir: &Path) -> Result<(), Error> {
    if model.files.iter().all(|file| file_ready(dir, &model.id, file)) {
        return Ok(());
    }
    if let Some(progress) = read_progress(dir, &model.id) {
        if progress.state == "downloading" && now_ms().saturating_sub(progress.updated_ms) < 20_000 {
            return Ok(());
        }
    }
    let total = model.files.iter().map(|file| file.bytes).sum();
    for file in &model.files {
        if file_ready(dir, &model.id, file) {
            continue;
        }
        let dest = placed(dir, &model.id, &file.filename)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| Error::new(error.to_string()))?;
        }
        let partial = PathBuf::from(format!("{}.partial", dest.display()));
        write_progress(dir, &model.id, "downloading", 0, total, &file.filename)?;
        let mut child = Command::new("curl")
            .args([
                "-L",
                "--fail",
                "-sS",
                "--retry",
                "3",
                "--retry-delay",
                "1",
                "-C",
                "-",
                "--output",
                &partial.display().to_string(),
                &file.url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Error::new("curl is not installed")
                } else {
                    Error::new(error.to_string())
                }
            })?;
        loop {
            let received = model
                .files
                .iter()
                .map(|item| {
                    if item.filename == file.filename {
                        fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0)
                    } else {
                        placed(dir, &model.id, &item.filename)
                            .ok()
                            .and_then(|path| fs::metadata(path).ok())
                            .map(|meta| meta.len())
                            .unwrap_or(0)
                    }
                })
                .sum();
            write_progress(dir, &model.id, "downloading", received, total, &file.filename)?;
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        let mut detail = String::new();
                        if let Some(mut stderr) = child.stderr.take() {
                            let _ = stderr.read_to_string(&mut detail);
                        }
                        let detail = detail.trim();
                        let detail = if detail.is_empty() {
                            format!("curl failed while fetching {}", file.filename)
                        } else {
                            detail.to_string()
                        };
                        write_progress(dir, &model.id, "failed", received, total, &detail)?;
                        return Err(Error::new(detail));
                    }
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(200)),
                Err(error) => return Err(Error::new(error.to_string())),
            }
        }
        if let Some(expected) = &file.sha256 {
            let actual = sha256_file(&partial)?;
            if !actual.eq_ignore_ascii_case(expected) {
                let detail = format!("{id} {name} hash mismatch", id = model.id, name = file.filename);
                write_progress(dir, &model.id, "failed", 0, total, &detail)?;
                let _ = fs::remove_file(&partial);
                return Err(Error::new(detail));
            }
        }
        let size = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
        if file.bytes > 0 && size != file.bytes {
            let detail = format!(
                "{} is {} bytes, expected {}",
                file.filename, size, file.bytes
            );
            write_progress(dir, &model.id, "failed", size, total, &detail)?;
            return Err(Error::new(detail));
        }
        fs::rename(&partial, &dest).map_err(|error| Error::new(error.to_string()))?;
    }
    let _ = fs::remove_file(progress_path(dir, &model.id));
    Ok(())
}

fn activate_model(model: &Model, dir: &Path, config_path: &Path) -> Result<String, Error> {
    if !model.files.iter().all(|file| file_ready(dir, &model.id, file)) {
        return Err(Error::new(format!("Download {} first.", model.name)));
    }
    if !model.runnable {
        return Err(Error::new(format!(
            "{} is on disk. This build transcribes with Whisper and diarizes with pyannote, so it was not switched.",
            model.name
        )));
    }
    let mut value = if config_path.is_file() {
        let text = fs::read_to_string(config_path).map_err(|error| Error::new(error.to_string()))?;
        serde_yaml::from_str::<serde_yaml::Value>(&text).unwrap_or(serde_yaml::Value::Null)
    } else {
        serde_yaml::Value::Null
    };
    let root = mapping(&mut value);
    if model.role == "transcription" {
        let path = slot(model, dir, "weights")?;
        let transcription = child(root, "transcription");
        transcription.insert(
            serde_yaml::Value::String("model".into()),
            serde_yaml::Value::String(path.display().to_string()),
        );
    } else if model.role == "diarization" {
        let segmentation = slot(model, dir, "segmentation")?;
        let embedding = slot(model, dir, "embedding")?;
        let diarization = child(root, "diarization");
        diarization.insert(
            serde_yaml::Value::String("segmentation_model".into()),
            serde_yaml::Value::String(segmentation.display().to_string()),
        );
        diarization.insert(
            serde_yaml::Value::String("embedding_model".into()),
            serde_yaml::Value::String(embedding.display().to_string()),
        );
    } else {
        return Err(Error::new(format!("{} has no role this CLI can select.", model.name)));
    }
    if let Some(parent) = config_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| Error::new(error.to_string()))?;
        }
    }
    let text = serde_yaml::to_string(&value).map_err(|error| Error::new(error.to_string()))?;
    fs::write(config_path, text).map_err(|error| Error::new(error.to_string()))?;
    let mut active = read_active(dir);
    if model.role == "transcription" {
        active.transcription = model.id.clone();
    } else {
        active.diarization = model.id.clone();
    }
    write_active(dir, &active)?;
    Ok(format!("{} is the {} model now.", model.name, model.role))
}

fn slot(model: &Model, dir: &Path, slot_name: &str) -> Result<PathBuf, Error> {
    let file = model
        .files
        .iter()
        .find(|file| file.slot == slot_name)
        .ok_or_else(|| Error::new(format!("{} has no {slot_name} file.", model.name)))?;
    placed(dir, &model.id, &file.filename)
}

fn mapping(value: &mut serde_yaml::Value) -> &mut serde_yaml::Mapping {
    if !matches!(value, serde_yaml::Value::Mapping(_)) {
        *value = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    match value {
        serde_yaml::Value::Mapping(map) => map,
        _ => unreachable!("mapping was just installed"),
    }
}

fn child<'a>(
    root: &'a mut serde_yaml::Mapping,
    key: &str,
) -> &'a mut serde_yaml::Mapping {
    let key = serde_yaml::Value::String(key.into());
    if !matches!(root.get(&key), Some(serde_yaml::Value::Mapping(_))) {
        root.insert(key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    match root.get_mut(&key) {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => unreachable!("child mapping was just installed"),
    }
}

fn file_ready(dir: &Path, id: &str, file: &Weight) -> bool {
    placed(dir, id, &file.filename)
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .is_some_and(|meta| file.bytes == 0 || meta.len() == file.bytes)
}

fn placed(dir: &Path, id: &str, filename: &str) -> Result<PathBuf, Error> {
    if !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err(Error::new(format!("Model id {id} is not a safe name.")));
    }
    if filename.is_empty()
        || filename.starts_with('/')
        || filename.contains('\\')
        || filename.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Error::new(format!("Refusing to store {filename}.")));
    }
    Ok(dir.join(id).join(filename))
}

fn progress_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.status.json"))
}

fn read_progress(dir: &Path, id: &str) -> Option<Progress> {
    let text = fs::read_to_string(progress_path(dir, id)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_progress(
    dir: &Path,
    id: &str,
    state: &str,
    received: u64,
    total: u64,
    detail: &str,
) -> Result<(), Error> {
    fs::create_dir_all(dir).map_err(|error| Error::new(error.to_string()))?;
    let progress = Progress {
        state: state.into(),
        received,
        total,
        detail: detail.into(),
        updated_ms: now_ms(),
    };
    let text = serde_json::to_string(&progress).map_err(|error| Error::new(error.to_string()))?;
    fs::write(progress_path(dir, id), text).map_err(|error| Error::new(error.to_string()))
}

fn active_path(dir: &Path) -> PathBuf {
    dir.join("active.yaml")
}

fn read_active(dir: &Path) -> Active {
    fs::read_to_string(active_path(dir))
        .ok()
        .and_then(|text| serde_yaml::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_active(dir: &Path, active: &Active) -> Result<(), Error> {
    fs::create_dir_all(dir).map_err(|error| Error::new(error.to_string()))?;
    let text = serde_yaml::to_string(active).map_err(|error| Error::new(error.to_string()))?;
    fs::write(active_path(dir), text).map_err(|error| Error::new(error.to_string()))
}

fn sha256_file(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path).map_err(|error| Error::new(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| Error::new(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 100.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn render_text(shelf: &Shelf) -> String {
    let mut out = format!("{}\n", shelf.note);
    for role in ["transcription", "diarization"] {
        out.push_str(&format!("\n{role}\n"));
        for model in shelf.models.iter().filter(|model| model.role == role) {
            let use_label = if model.active {
                "in use"
            } else if model.runnable {
                "can switch"
            } else {
                "download only"
            };
            out.push_str(&format!(
                "  {}  {}  {}  {}  {}  {use_label}\n    {}\n",
                model.id, model.size, model.badge, model.runtime, model.state, model.summary
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn the_shelf_names_the_models_worth_trying() {
        let shelf = list(Path::new("/tmp/socioscope-models-missing")).unwrap();
        let ids: Vec<_> = shelf.models.iter().map(|model| model.id.as_str()).collect();
        for id in [
            "cactus-whistle",
            "whisper-tiny-en",
            "whisper-large-v3-turbo-q8",
            "desertant-voz",
            "parakeet-tdt-v3-q8",
            "who-coreml-int8",
            "pyannote-3dspeaker",
            "nemotron-3-diarization-q8",
        ] {
            assert!(ids.contains(&id), "{ids:?}");
        }
        assert!(shelf.note.contains("QVAC"));
        assert!(shelf.models.iter().any(|model| model.runnable && model.role == "transcription"));
        assert!(shelf.models.iter().any(|model| model.id == "nemotron-3-diarization-q8" && !model.runnable));
    }

    #[test]
    fn a_download_checks_the_hash_and_use_points_config_at_it() {
        let dir = std::env::temp_dir().join(format!("socioscope-models-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let body = b"hello model";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = socket.read(&mut buf);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(header.as_bytes()).unwrap();
            socket.write_all(body).unwrap();
        });
        let model = Model {
            id: "toy-whisper".into(),
            name: "Toy Whisper".into(),
            role: "transcription".into(),
            badge: "Fast".into(),
            runtime: "whisper.cpp".into(),
            summary: "A test weight.".into(),
            license: "MIT".into(),
            released: "2026".into(),
            runnable: true,
            files: vec![Weight {
                url: format!("http://{addr}/toy.bin"),
                filename: "toy.bin".into(),
                bytes: body.len() as u64,
                slot: "weights".into(),
                sha256: Some(sha256_bytes(body)),
            }],
        };
        download_model(&model, &dir).unwrap();
        assert_eq!(fs::read(dir.join("toy-whisper/toy.bin")).unwrap(), body);
        let config = dir.join("config.yaml");
        fs::write(&config, "huggingface_token: \"keep\"\ntranscription:\n  model: old\n").unwrap();
        let message = activate_model(&model, &dir, &config).unwrap();
        assert!(message.contains("Toy Whisper"));
        let text = fs::read_to_string(&config).unwrap();
        assert!(text.contains("keep"), "{text}");
        assert!(text.contains("toy.bin"), "{text}");
        assert!(!text.contains("model: old"), "{text}");

        let parked = Model {
            id: "toy-nemotron".into(),
            name: "Toy Nemotron".into(),
            role: "diarization".into(),
            badge: "Best".into(),
            runtime: "Nemotron".into(),
            summary: "Not wired.".into(),
            license: "OpenMDW-1.1".into(),
            released: "2026".into(),
            runnable: false,
            files: vec![Weight {
                url: "http://127.0.0.1/unused".into(),
                filename: "n.gguf".into(),
                bytes: 4,
                slot: "weights".into(),
                sha256: None,
            }],
        };
        fs::create_dir_all(dir.join("toy-nemotron")).unwrap();
        fs::write(dir.join("toy-nemotron/n.gguf"), b"gguf").unwrap();
        let error = activate_model(&parked, &dir, &config).unwrap_err();
        assert!(error.to_string().contains("was not switched"), "{error}");
        assert!(!fs::read_to_string(&config).unwrap().contains("n.gguf"));
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }
}
