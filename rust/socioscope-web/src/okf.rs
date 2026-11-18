//! One capture is one OKF v0.2 bundle: markdown notes plus the original audio.

use std::fs;
use std::path::{Path, PathBuf};

use crate::srt::{self, Cue};
use crate::timeutil::{self, log_date};

pub struct Summary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub kind: String,
    pub recorded_at: String,
    pub analysis: String,
}

pub struct Concept {
    pub relative: String,
    pub title: String,
    pub type_name: String,
    pub description: String,
    pub frontmatter: String,
    pub body: String,
    pub raw: String,
}

#[derive(Clone)]
pub struct Note {
    pub title: String,
    pub description: String,
    pub status: String,
    pub kind: String,
    pub source_title: String,
    pub source_url: String,
    pub created_at: String,
    pub audio_rel: String,
    pub duration_ms: u64,
    pub cues: Vec<Cue>,
    pub analysis: String,
    pub analysis_error: String,
    pub byte_size: u64,
}

pub fn bundle_dir(data: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("unknown package".into());
    }
    Ok(data.join("bundles").join(id))
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn write_notes(dir: &Path, note: &Note) -> Result<(), String> {
    fs::create_dir_all(dir.join("references")).map_err(|error| error.to_string())?;
    fs::write(dir.join("index.md"), index_md(note)).map_err(|error| error.to_string())?;
    fs::write(dir.join("session.md"), session_md(note)).map_err(|error| error.to_string())?;
    fs::write(dir.join("recording.md"), recording_md(note)).map_err(|error| error.to_string())?;
    fs::write(dir.join("transcript.md"), transcript_md(note)).map_err(|error| error.to_string())?;
    fs::write(
        dir.join("references").join("diarized_transcription.srt"),
        srt::write(&note.cues),
    )
    .map_err(|error| error.to_string())?;
    write_speakers(dir, note)?;
    if !dir.join("log.md").is_file() {
        fs::write(dir.join("log.md"), log_header()).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn append_log(dir: &Path, at: &str, line: &str) -> Result<(), String> {
    if !dir.join("log.md").is_file() {
        fs::write(dir.join("log.md"), log_header()).map_err(|error| error.to_string())?;
    }
    let date = log_date(at);
    let mut text = fs::read_to_string(dir.join("log.md")).unwrap_or_else(|_| log_header());
    let heading = format!("## {date}\n");
    let entry = format!("* {line}\n");
    if let Some(index) = text.find(&heading) {
        let insert_at = index + heading.len();
        text.insert_str(insert_at, &entry);
    } else {
        text.push('\n');
        text.push_str(&heading);
        text.push_str(&entry);
    }
    fs::write(dir.join("log.md"), text).map_err(|error| error.to_string())
}

pub fn write_catalog(data: &Path) -> Result<(), String> {
    let bundles = list(data)?;
    let mut body = String::from("---\nokf_version: \"0.2\"\n---\n\n# Sessions\n\n");
    if bundles.is_empty() {
        body.push_str("No sessions yet.\n");
    }
    for bundle in bundles {
        body.push_str(&format!(
            "* [{}](bundles/{}/session.md) - {}\n",
            bundle.title, bundle.id, bundle.description
        ));
    }
    fs::create_dir_all(data).map_err(|error| error.to_string())?;
    fs::write(data.join("index.md"), body).map_err(|error| error.to_string())
}

pub fn list(data: &Path) -> Result<Vec<Summary>, String> {
    let root = data.join("bundles");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(&root)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    let mut bundles = Vec::new();
    for entry in entries {
        let id = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&id) {
            continue;
        }
        let Some(summary) = read_summary(&entry.path(), &id) else {
            continue;
        };
        bundles.push(summary);
    }
    Ok(bundles)
}

pub fn read_summary(dir: &Path, id: &str) -> Option<Summary> {
    let text = fs::read_to_string(dir.join("session.md")).ok()?;
    let (frontmatter, _) = split_frontmatter(&text);
    Some(Summary {
        id: id.to_string(),
        title: scalar(&frontmatter, "title").unwrap_or_else(|| id.to_string()),
        description: scalar(&frontmatter, "description").unwrap_or_default(),
        status: scalar(&frontmatter, "status").unwrap_or_else(|| "stable".into()),
        kind: scalar(&frontmatter, "capture").unwrap_or_else(|| "opened".into()),
        recorded_at: generated_at(&frontmatter).unwrap_or_default(),
        analysis: scalar(&frontmatter, "analysis").unwrap_or_default(),
    })
}

pub fn concepts(dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    walk_md(dir, dir, &mut found);
    found.sort();
    found
        .into_iter()
        .map(|relative| {
            let type_name = fs::read_to_string(dir.join(&relative))
                .ok()
                .and_then(|text| {
                    let (frontmatter, _) = split_frontmatter(&text);
                    scalar(&frontmatter, "type")
                })
                .unwrap_or_else(|| reserved_label(&relative).to_string());
            (relative, type_name)
        })
        .collect()
}

pub fn read_concept(dir: &Path, relative: &str) -> Result<Concept, String> {
    let path = safe_join(dir, relative)?;
    if !path.is_file() {
        return Err("no such note".into());
    }
    let raw = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let (frontmatter, body) = split_frontmatter(&raw);
    let stem = Path::new(relative)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(relative);
    Ok(Concept {
        relative: relative.to_string(),
        title: scalar(&frontmatter, "title").unwrap_or_else(|| stem.to_string()),
        type_name: scalar(&frontmatter, "type")
            .unwrap_or_else(|| reserved_label(relative).to_string()),
        description: scalar(&frontmatter, "description").unwrap_or_default(),
        frontmatter,
        body,
        raw,
    })
}

pub fn write_peaks(dir: &Path, peaks: &[f32]) -> Result<(), String> {
    let text = peaks
        .iter()
        .map(|peak| format!("{}", (peak.clamp(0.0, 1.0) * 1000.0).round() as u32))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(dir.join("references").join("peaks.txt"), text).map_err(|error| error.to_string())
}

pub fn read_peaks(dir: &Path) -> Vec<f32> {
    fs::read_to_string(dir.join("references").join("peaks.txt"))
        .ok()
        .map(|text| {
            text.split_whitespace()
                .filter_map(|value| value.parse::<f32>().ok())
                .map(|value| value / 1000.0)
                .collect()
        })
        .unwrap_or_default()
}

pub fn media_file(dir: &Path) -> Option<PathBuf> {
    let references = dir.join("references");
    let recording = references.join("recording.wav");
    if recording.is_file() {
        return Some(recording);
    }
    let entries = fs::read_dir(&references).ok()?;
    entries.filter_map(|entry| entry.ok()).find_map(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("original.") {
            Some(entry.path())
        } else {
            None
        }
    })
}

pub fn safe_join(dir: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("no such note".into());
    }
    Ok(dir.join(relative))
}

pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "speaker".into()
    } else {
        out
    }
}

fn write_speakers(dir: &Path, note: &Note) -> Result<(), String> {
    let mut speakers = Vec::new();
    for cue in &note.cues {
        for speaker in &cue.speakers {
            if !speakers.contains(speaker) {
                speakers.push(speaker.clone());
            }
        }
    }
    if speakers.is_empty() {
        let _ = fs::remove_dir_all(dir.join("speakers"));
        return Ok(());
    }
    fs::create_dir_all(dir.join("speakers")).map_err(|error| error.to_string())?;
    let mut index = String::from("# Speakers\n\n");
    for speaker in &speakers {
        let slug = slug(speaker);
        index.push_str(&format!(
            "* [{speaker}]({slug}.md) - A voice in this take.\n"
        ));
        let body = format!(
            "---\ntype: Speaker\ntitle: {title}\ndescription: A voice identified in this take.\ntags: [speaker]\nstatus: {status}\ngenerated: {{ by: process:socioscope-web, at: {at} }}\n---\n\nHeard in the [transcript](/transcript.md).\n",
            title = yaml(&speaker),
            status = note.status,
            at = note.created_at,
        );
        fs::write(dir.join("speakers").join(format!("{slug}.md")), body)
            .map_err(|error| error.to_string())?;
    }
    fs::write(dir.join("speakers").join("index.md"), index).map_err(|error| error.to_string())
}

fn index_md(note: &Note) -> String {
    let mut body = format!(
        "---\nokf_version: \"0.2\"\n---\n\n# Session\n\n* [{title}](session.md) - {description}\n\n# Recording\n\n* [Original recording](recording.md) - The take stored with this package.\n\n# Transcript\n\n* [Diarized transcript](transcript.md) - Speaker-labelled cues.\n",
        title = note.title,
        description = note.description,
    );
    if note.cues.iter().any(|cue| !cue.speakers.is_empty()) {
        body.push_str("\n# Speakers\n\n* [Speakers](speakers/index.md) - Voices in this take.\n");
    }
    body
}

fn session_md(note: &Note) -> String {
    let analysis = if note.analysis.is_empty() {
        String::new()
    } else {
        format!("analysis: {}\n", note.analysis)
    };
    format!(
        "---\ntype: Recording Session\ntitle: {title}\ndescription: {description}\nresource: /recording.md\ntags: [session, {kind}]\nstatus: {status}\ncapture: {kind}\n{analysis}generated: {{ by: process:socioscope-web, at: {at} }}\nsources:\n  - id: tape\n    resource: {source_url}\n    title: {source_title}\n    author: process:archive-org\n---\n\nThis package is one capture. The [original recording](/recording.md) is the take. The [diarized transcript](/transcript.md) is the reading of that take.\n\n{extra}\n",
        title = yaml(&note.title),
        description = yaml(&note.description),
        kind = note.kind,
        status = note.status,
        analysis = analysis,
        at = note.created_at,
        source_url = yaml(&note.source_url),
        source_title = yaml(&note.source_title),
        extra = if note.kind == "emulated" {
            "The microphone is the public Kennedy–Nixon tape, played at 1×. Cues from the prepared opening alignment appear as that clock advances. That is the stand-in for streaming analysis until an on-device model can keep up."
        } else if note.analysis == "queued" || note.analysis == "running" {
            "Socioscope is reading the take."
        } else if !note.analysis_error.is_empty() {
            "The read did not finish. The original file is still in this package."
        } else {
            "Opened from a file. The original bytes are in this package."
        }
    )
}

fn recording_md(note: &Note) -> String {
    let duration = if note.duration_ms == 0 {
        "unknown".to_string()
    } else {
        srt::clock(note.duration_ms)
    };
    format!(
        "---\ntype: Audio Recording\ntitle: Original recording\ndescription: The captured input, kept as the original take.\nresource: /{audio}\ntags: [audio, original]\nstatus: {status}\ngenerated: {{ by: process:socioscope-web, at: {at} }}\n---\n\n# Take\n\n| Field | Value |\n| --- | --- |\n| Duration | {duration} |\n| Bytes | {bytes} |\n| Source | {source} |\n\nThe file linked above is the original recording for this session.\n",
        audio = note.audio_rel,
        status = note.status,
        at = note.created_at,
        bytes = note.byte_size,
        source = note.source_title,
    )
}

fn transcript_md(note: &Note) -> String {
    let mut cues = String::from("# Cues\n\n");
    if !note.analysis_error.is_empty() {
        cues.push_str(&note.analysis_error);
        cues.push_str("\n\n");
    }
    if note.cues.is_empty() {
        cues.push_str("Listening. Cues show up as the take advances.\n");
    }
    for cue in &note.cues {
        let who = if cue.speakers.is_empty() {
            "Voice".to_string()
        } else {
            cue.speakers
                .iter()
                .map(|speaker| format!("[{speaker}](speakers/{}.md)", slug(speaker)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        cues.push_str(&format!(
            "* **{}–{}** {who} — {}\n",
            srt::clock(cue.start_ms),
            srt::clock(cue.end_ms),
            cue.text
        ));
    }
    format!(
        "---\ntype: Diarized Transcript\ntitle: Diarized transcript\ndescription: Speaker-labelled cues for this take.\nresource: /references/diarized_transcription.srt\ntags: [transcript, srt]\nstatus: {status}\ngenerated: {{ by: process:socioscope-stream, at: {at} }}\nsources:\n  - id: take\n    resource: /recording.md\n    title: Original recording\n---\n\n{cues}",
        status = note.status,
        at = note.created_at,
    )
}

fn log_header() -> String {
    "# Session Update Log\n".to_string()
}

fn walk_md(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("references") {
                continue;
            }
            walk_md(root, &path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            if let Ok(relative) = path.strip_prefix(root) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn reserved_label(relative: &str) -> &'static str {
    match Path::new(relative).file_name().and_then(|name| name.to_str()) {
        Some("index.md") => "Index",
        Some("log.md") => "Log",
        _ => "Note",
    }
}

pub fn tape_source(frontmatter: &str) -> (String, String) {
    let mut resource = String::new();
    let mut title = String::new();
    let mut sources = false;
    for line in frontmatter.lines() {
        if line.starts_with("sources:") {
            sources = true;
            continue;
        }
        if sources && !line.starts_with(' ') && !line.starts_with('-') && !line.is_empty() {
            break;
        }
        if !sources {
            continue;
        }
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("resource:") {
            resource = unquote(value.trim());
        } else if let Some(value) = trimmed.strip_prefix("title:") {
            title = unquote(value.trim());
        }
    }
    (resource, title)
}

pub fn split_frontmatter(text: &str) -> (String, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (String::new(), text.to_string());
    };
    let rest = rest.trim_start_matches('\n');
    let Some(end) = rest.find("\n---") else {
        return (String::new(), text.to_string());
    };
    let frontmatter = rest[..end].trim().to_string();
    let body = rest[end + 4..].trim_start_matches('\n').to_string();
    (frontmatter, body)
}

fn scalar(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in frontmatter.lines() {
        if let Some(value) = line.strip_prefix(&prefix) {
            let value = value.trim();
            if value.is_empty() || value.starts_with('{') || value.starts_with('[') {
                return None;
            }
            return Some(unquote(value));
        }
    }
    None
}

fn generated_at(frontmatter: &str) -> Option<String> {
    let token = "at: ";
    let start = frontmatter.find(token)? + token.len();
    let rest = &frontmatter[start..];
    let end = rest.find(|c: char| c.is_whitespace() || c == '}').unwrap_or(rest.len());
    Some(rest[..end].trim_matches('"').to_string())
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

pub fn yaml(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".into();
    }
    if value.chars().any(|c| {
        c.is_whitespace() || "\"':#{}[]&*!|>%@`,".contains(c) || c.is_control()
    }) {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

pub fn fresh_note(kind: &str, title: &str, description: &str, source_title: &str, source_url: &str, audio_rel: &str) -> Note {
    Note {
        title: title.to_string(),
        description: description.to_string(),
        status: "draft".into(),
        kind: kind.into(),
        source_title: source_title.into(),
        source_url: source_url.into(),
        created_at: timeutil::utc_now(),
        audio_rel: audio_rel.into(),
        duration_ms: 0,
        cues: Vec::new(),
        analysis: String::new(),
        analysis_error: String::new(),
        byte_size: 0,
    }
}
