use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::okf::{self, Bundle};
use crate::srt::{self, Cue};

#[derive(Clone, Debug)]
pub struct Recording {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub source: Option<PathBuf>,
    pub srt_path: Option<PathBuf>,
    pub cues: Vec<Cue>,
    pub duration_ms: u64,
    pub peaks: Vec<f32>,
    pub sample: bool,
    pub order: i32,
    pub from_pipeline: bool,
    pub bundle: Bundle,
    pub dir: PathBuf,
}

#[derive(Deserialize)]
struct SessionFile {
    id: Option<String>,
    title: String,
    detail: String,
    source: Option<String>,
    srt: Option<String>,
    duration_ms: Option<u64>,
    order: Option<i32>,
    #[serde(default)]
    sample: bool,
}

pub fn library_root() -> PathBuf {
    if let Ok(path) = std::env::var("SOCIOSCOPE_LIBRARY") {
        return PathBuf::from(path);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../library");
    if dev.exists() {
        return dev;
    }
    PathBuf::from("library")
}

pub fn repo_root() -> PathBuf {
    library_root()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(library_root)
}

pub fn pretty_path(path: &Path) -> String {
    let cleaned = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let root = fs::canonicalize(repo_root()).unwrap_or_else(|_| repo_root());
    cleaned
        .strip_prefix(&root)
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|_| cleaned.display().to_string())
}

pub fn load_library() -> Vec<Recording> {
    let root = library_root();
    let Ok(entries) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut recordings = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let session_path = path.join("session.json");
        if session_path.exists() {
            if let Some(recording) = load_session(&path, &session_path) {
                recordings.push(recording);
            }
        }
    }
    recordings.sort_by(|a, b| a.order.cmp(&b.order).then(a.title.cmp(&b.title)));
    recordings
}

pub fn import_srt(source: &Path) -> Result<String, String> {
    let name = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("captions");
    let slug = slugify(name);
    let dir = library_root().join("imports").join(&slug);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let srt_name = "captions.srt";
    fs::copy(source, dir.join(srt_name)).map_err(|err| err.to_string())?;
    let title = name.replace(['_', '-'], " ");
    let session = serde_json::json!({
        "id": slug,
        "title": title,
        "detail": "Imported captions",
        "srt": srt_name,
        "order": 100,
    });
    fs::write(
        dir.join("session.json"),
        serde_json::to_string_pretty(&session).map_err(|err| err.to_string())? + "\n",
    )
    .map_err(|err| err.to_string())?;
    okf::seed(&dir, &title, "Imported captions").map_err(|err| err.to_string())?;
    Ok(slug)
}

pub fn waveform(path: &Path, buckets: usize) -> Vec<f32> {
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            &path.to_string_lossy(),
            "-ac",
            "1",
            "-ar",
            "4000",
            "-f",
            "f32le",
            "-",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() || output.stdout.len() < 4 {
        return Vec::new();
    }
    let samples = output.stdout.len() / 4;
    let mut values = Vec::with_capacity(samples);
    for chunk in output.stdout.chunks_exact(4) {
        values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).abs());
    }
    if values.is_empty() {
        return Vec::new();
    }
    let buckets = buckets.max(1);
    let size = (values.len() / buckets).max(1);
    let mut peaks = Vec::with_capacity(buckets);
    for index in 0..buckets {
        let start = index * size;
        if start >= values.len() {
            break;
        }
        let end = (start + size).min(values.len());
        let peak = values[start..end].iter().copied().fold(0.0, f32::max);
        peaks.push((peak * 2.6).clamp(0.0, 1.0));
    }
    peaks
}

fn load_session(dir: &Path, session_path: &Path) -> Option<Recording> {
    let raw = fs::read_to_string(session_path).ok()?;
    let session: SessionFile = serde_json::from_str(&raw).ok()?;
    let source = session
        .source
        .as_deref()
        .map(|path| resolve(dir, path))
        .filter(|path| path.exists());
    let authored = session.srt.as_deref().map(|path| resolve(dir, path));
    let pipeline = source.as_ref().and_then(|path| pipeline_srt(path)).filter(|path| path.exists());
    let (srt_path, from_pipeline) = if let Some(path) = pipeline {
        (Some(path), true)
    } else {
        (authored.filter(|path| path.exists()), false)
    };
    let cues = srt_path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| srt::parse_srt(&text))
        .unwrap_or_default();
    let peaks = fs::read_to_string(dir.join("peaks.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let duration_ms = session
        .duration_ms
        .or_else(|| cues.last().map(|cue| cue.end_ms))
        .unwrap_or(0);
    let id = session
        .id
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| dir.file_name().and_then(|name| name.to_str()).unwrap_or("session").to_string());
    Some(Recording {
        id,
        title: session.title,
        detail: session.detail,
        source,
        srt_path,
        cues,
        duration_ms,
        peaks,
        sample: session.sample,
        order: session.order.unwrap_or(50),
        from_pipeline,
        bundle: okf::load(dir),
        dir: dir.to_path_buf(),
    })
}

fn pipeline_srt(source: &Path) -> Option<PathBuf> {
    let stem = source.file_stem()?;
    Some(source.parent()?.join(stem).join("diarized_transcription.srt"))
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { "captions".to_string() } else { slug }
}
