//! Local app: record or open audio, keep the original, browse the OKF package.

mod audio_io;
mod okf;
mod record;
mod srt;
mod timeutil;
mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Form, Multipart, Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use socioscope::messages::{invalid_audio_format, SUPPORTED_AUDIO_FORMATS};
use socioscope::paths::{get_output_directory_path, splitext};
use socioscope::transcription::{AudioTranscriber, Engine};

use crate::okf::Note;
use crate::record::{Live, Phase, Tape};
use crate::srt::Cue;

pub trait Analyze: Send {
    fn analyze(&mut self, audio: &Path) -> Result<String, String>;
}

struct EngineAnalyze {
    engine: Engine,
}

impl Analyze for EngineAnalyze {
    fn analyze(&mut self, audio: &Path) -> Result<String, String> {
        self.engine
            .transcribe_audio(audio)
            .map_err(|error| error.to_string())?;
        let srt = get_output_directory_path(audio).join("diarized_transcription.srt");
        std::fs::read_to_string(&srt).map_err(|error| format!("no transcript at {}: {error}", srt.display()))
    }
}

pub struct FixedAnalyze {
    pub srt: String,
}

impl Analyze for FixedAnalyze {
    fn analyze(&mut self, audio: &Path) -> Result<String, String> {
        if !audio.is_file() {
            return Err(format!("missing {}", audio.display()));
        }
        Ok(self.srt.clone())
    }
}

pub struct AppConfig {
    pub data_dir: PathBuf,
    pub pace: f64,
    pub opening_wav: Vec<u8>,
    pub opening_srt: String,
    pub full_tape: Option<PathBuf>,
    pub analyzer: Box<dyn Analyze + Send>,
    pub socioscope_bin: PathBuf,
    pub models_dir: PathBuf,
    pub config_path: PathBuf,
}

struct Inner {
    data_dir: PathBuf,
    pace: f64,
    opening_wav: Vec<u8>,
    opening_cues: Vec<Cue>,
    full_tape: Option<PathBuf>,
    lives: Mutex<HashMap<String, Arc<Mutex<Live>>>>,
    stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
    jobs: Sender<String>,
    socioscope_bin: PathBuf,
    models_dir: PathBuf,
    config_path: PathBuf,
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Inner>,
}

const ARCHIVE: &str = "https://archive.org/details/1960_kennedy-nixon_1";
const OPENING_LABEL: &str = "Kennedy–Nixon opening, 26 September 1960";
const DEBATE_LABEL: &str = "Kennedy–Nixon first debate, 26 September 1960";

pub fn router() -> Router {
    let full = ["tests/data/jfk-rmn.mp3", "jfk-rmn.mp3"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file());
    let data = std::env::var("SOCIOSCOPE_DATA").unwrap_or_else(|_| "socioscope-data".into());
    let pace = std::env::var("SOCIOSCOPE_PACE")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|pace: &f64| *pace > 0.0)
        .unwrap_or(1.0);
    router_from(AppConfig {
        data_dir: PathBuf::from(data),
        pace,
        opening_wav: include_bytes!("../fixtures/kennedy-nixon.wav").to_vec(),
        opening_srt: include_str!("../fixtures/kennedy-nixon.srt").to_string(),
        full_tape: full,
        analyzer: Box::new(EngineAnalyze {
            engine: Engine::new(),
        }),
        socioscope_bin: socioscope_bin(),
        models_dir: std::env::var("SOCIOSCOPE_MODELS")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("socioscope-models")),
        config_path: std::env::var("SOCIOSCOPE_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("config.yaml")),
    })
}

fn socioscope_bin() -> PathBuf {
    if let Ok(path) = std::env::var("SOCIOSCOPE_BIN") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("socioscope");
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("socioscope")
}

pub fn router_from(config: AppConfig) -> Router {
    let (tx, rx) = mpsc::channel();
    let data_dir = config.data_dir.clone();
    thread::spawn(move || worker(rx, data_dir, config.analyzer));
    let state = AppState {
        inner: Arc::new(Inner {
            data_dir: config.data_dir,
            pace: config.pace,
            opening_cues: srt::parse(&config.opening_srt),
            opening_wav: config.opening_wav,
            full_tape: config.full_tape,
            lives: Mutex::new(HashMap::new()),
            stops: Mutex::new(HashMap::new()),
            jobs: tx,
            socioscope_bin: config.socioscope_bin,
            models_dir: config.models_dir,
            config_path: config.config_path,
        }),
    };
    Router::new()
        .route("/", get(home))
        .route("/record", get(record_page))
        .route("/models", get(models_page))
        .route("/models/{id}/download", post(download_model))
        .route("/models/{id}/use", post(use_model))
        .route("/package", get(package_page))
        .route("/library", get(library))
        .route("/recordings", post(start_recording))
        .route("/recordings/{id}", get(show_recording))
        .route("/recordings/{id}/stop", post(stop_recording))
        .route("/imports/file", post(import_file))
        .route("/imports/path", post(import_path))
        .route("/bundles/{id}", get(show_bundle))
        .route("/bundles/{id}/package", get(show_package))
        .route("/bundles/{id}/analysis", get(show_analysis))
        .route("/bundles/{id}/media", get(media))
        .route("/bundles/{id}/concepts/{*path}", get(concept))
        .route("/app.css", get(css))
        .route("/app.js", get(js))
        .route("/htmx.min.js", get(htmx))
        .route("/icon.svg", get(icon))
        .route("/manifest.webmanifest", get(manifest))
        .route("/fonts/{name}", get(font))
        .layer(DefaultBodyLimit::max(80 * 1024 * 1024))
        .with_state(state)
}

fn worker(rx: mpsc::Receiver<String>, data: PathBuf, mut analyzer: Box<dyn Analyze + Send>) {
    while let Ok(id) = rx.recv() {
        let Ok(dir) = okf::bundle_dir(&data, &id) else {
            continue;
        };
        let Some(audio) = okf::media_file(&dir) else {
            let _ = finish_analysis(&dir, None, "The package has no audio file.");
            continue;
        };
        if let Some(mut note) = note_from(&dir) {
            note.analysis = "running".into();
            let _ = okf::write_notes(&dir, &note);
        }
        match analyzer.analyze(&audio) {
            Ok(srt) => {
                let _ = finish_analysis(&dir, Some(srt), "");
            }
            Err(error) => {
                let _ = finish_analysis(&dir, None, &error);
            }
        }
        let _ = okf::write_catalog(&data);
    }
}

fn finish_analysis(dir: &Path, srt: Option<String>, error: &str) -> Result<(), String> {
    let Some(mut note) = note_from(dir) else {
        return Err("missing session".into());
    };
    if let Some(srt) = srt {
        note.cues = srt::parse(&srt);
        note.analysis = "done".into();
        note.analysis_error.clear();
        note.status = "stable".into();
        okf::append_log(
            dir,
            &note.created_at,
            "**Update**: Wrote the diarized transcript from the original file.",
        )?;
    } else {
        note.analysis = "error".into();
        note.analysis_error = error.to_string();
        note.status = "draft".into();
        okf::append_log(dir, &note.created_at, "**Update**: The read failed. The original file is unchanged.")?;
    }
    okf::write_notes(dir, &note)
}

fn note_from(dir: &Path) -> Option<Note> {
    let id = dir.file_name()?.to_str()?;
    let summary = okf::read_summary(dir, id)?;
    let title = summary.title.clone();
    let cues = std::fs::read_to_string(dir.join("references").join("diarized_transcription.srt"))
        .map(|text| srt::parse(&text))
        .unwrap_or_default();
    let session = std::fs::read_to_string(dir.join("session.md")).unwrap_or_default();
    let (frontmatter, _) = okf::split_frontmatter(&session);
    let (source_url, source_title) = okf::tape_source(&frontmatter);
    let audio_rel = okf::media_file(dir)
        .and_then(|path| path.strip_prefix(dir).ok().map(|path| path.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|| "references/recording.wav".into());
    let byte_size = okf::media_file(dir)
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .unwrap_or(0);
    let source_title = if source_title.is_empty() {
        title.clone()
    } else {
        source_title
    };
    Some(Note {
        title,
        description: summary.description,
        status: summary.status,
        kind: summary.kind,
        source_title,
        source_url,
        created_at: if summary.recorded_at.is_empty() {
            timeutil::utc_now()
        } else {
            summary.recorded_at
        },
        audio_rel,
        duration_ms: cues.iter().map(|cue| cue.end_ms).max().unwrap_or(0),
        cues,
        analysis: summary.analysis,
        analysis_error: String::new(),
        byte_size,
    })
}

async fn home(State(state): State<AppState>, headers: HeaderMap) -> Response {
    page(&state, &headers, Screen::Home, None)
}

async fn record_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    page(&state, &headers, Screen::Record, None)
}

async fn package_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    page(&state, &headers, Screen::Catalog, None)
}

#[derive(Deserialize)]
struct Search {
    q: Option<String>,
}

async fn library(State(state): State<AppState>, headers: HeaderMap, Query(search): Query<Search>) -> Response {
    let query = search.q.unwrap_or_default();
    let items = filtered(&state, &query);
    if hx(&headers) {
        return Html(view::library_items(&items)).into_response();
    }
    let _ = query;
    page(&state, &headers, Screen::Home, None)
}

#[derive(Deserialize)]
struct SourceForm {
    source: String,
}

async fn start_recording(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SourceForm>,
) -> Response {
    if let Some(existing) = active(&state) {
        let live = existing.lock().expect("live");
        return respond(&state, &headers, Screen::Record, None, view::recording_stage(&live));
    }
    let (tape, cues, title, description, label) = if form.source == "debate" {
        let Some(path) = state.inner.full_tape.clone() else {
            return respond(
                &state,
                &headers,
                Screen::Record,
                None,
                view::alert("The full debate tape is not on this machine."),
            );
        };
        (
            Tape::File(path),
            state.inner.opening_cues.clone(),
            "Kennedy–Nixon, first debate".to_string(),
            "Emulated capture of the 26 September 1960 debate. The opening alignment follows the tape; the rest of the take is stored for streaming analysis.".to_string(),
            DEBATE_LABEL.to_string(),
        )
    } else if form.source == "opening" {
        (
            Tape::Wav(state.inner.opening_wav.clone()),
            state.inner.opening_cues.clone(),
            "Kennedy–Nixon opening".to_string(),
            "Emulated capture of Howard K. Smith introducing Senator John F. Kennedy, 26 September 1960.".to_string(),
            OPENING_LABEL.to_string(),
        )
    } else {
        return respond(&state, &headers, Screen::Record, None, view::alert("Choose a tape."));
    };

    let id = format!("ses-{}", timeutil::stamp_now());
    let live = Arc::new(Mutex::new(Live {
        id: id.clone(),
        title: title.clone(),
        source_label: label.clone(),
        phase: Phase::Preparing,
        error: String::new(),
        elapsed_ms: 0,
        duration_ms: 0,
        peaks: Vec::new(),
        cues: cues.clone(),
    }));
    let stop = Arc::new(AtomicBool::new(false));
    state.inner.lives.lock().expect("lives").insert(id.clone(), Arc::clone(&live));
    state.inner.stops.lock().expect("stops").insert(id.clone(), Arc::clone(&stop));
    record::begin(record::Start {
        data_dir: state.inner.data_dir.clone(),
        id,
        title,
        description,
        source_label: label,
        source_url: ARCHIVE.into(),
        tape,
        cues,
        pace: state.inner.pace,
        live: Arc::clone(&live),
        stop,
    });
    let guard = live.lock().expect("live");
    if guard.phase == Phase::Done {
        let id = guard.id.clone();
        drop(guard);
        return redirect(&headers, &format!("/bundles/{id}"));
    }
    respond(&state, &headers, Screen::Record, None, view::recording_stage(&guard))
}

async fn show_recording(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    let Some(live) = state.inner.lives.lock().expect("lives").get(&id).cloned() else {
        return (StatusCode::NOT_FOUND, "No such recording").into_response();
    };
    let live = live.lock().expect("live");
    if live.phase == Phase::Done {
        return redirect(&headers, &format!("/bundles/{id}"));
    }
    if hx(&headers) {
        return Html(view::live_poll(&live)).into_response();
    }
    respond(&state, &headers, Screen::Record, None, view::recording_stage(&live))
}

async fn stop_recording(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    if let Some(stop) = state.inner.stops.lock().expect("stops").get(&id) {
        stop.store(true, Ordering::Relaxed);
    }
    show_recording(State(state), headers, UrlPath(id)).await
}

async fn import_file(State(state): State<AppState>, headers: HeaderMap, mut multipart: Multipart) -> Response {
    let mut name = String::new();
    let mut bytes = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => {
                return respond(&state, &headers, Screen::Home, None, view::alert(&error.to_string()));
            }
        };
        if field.name() != Some("audio") {
            continue;
        }
        name = field
            .file_name()
            .unwrap_or("audio")
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("audio")
            .to_string();
        bytes = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(error) => {
                return respond(&state, &headers, Screen::Home, None, view::alert(&error.to_string()));
            }
        };
    }
    if name.is_empty() || bytes.is_empty() {
        return respond(&state, &headers, Screen::Home, None, view::alert("Choose an audio file."));
    }
    match store_original(&state, &name, &bytes) {
        Ok(id) => show_bundle(State(state), headers, UrlPath(id)).await,
        Err(error) => respond(&state, &headers, Screen::Home, None, view::alert(&error)),
    }
}

#[derive(Deserialize)]
struct PathForm {
    path: String,
}

async fn import_path(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<PathForm>) -> Response {
    let path = form.path.trim();
    if path.is_empty() {
        return respond(&state, &headers, Screen::Home, None, view::alert("Enter a path to an audio file."));
    }
    let file = PathBuf::from(path);
    if file.is_dir() {
        return respond(&state, &headers, Screen::Home, None, view::alert("Open one audio file."));
    }
    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(error) => {
            return respond(
                &state,
                &headers,
                Screen::Home,
                None,
                view::alert(&format!("could not read {}: {error}", file.display())),
            );
        }
    };
    let name = file.file_name().and_then(|name| name.to_str()).unwrap_or("audio");
    match store_original(&state, name, &bytes) {
        Ok(id) => show_bundle(State(state), headers, UrlPath(id)).await,
        Err(error) => respond(&state, &headers, Screen::Home, None, view::alert(&error)),
    }
}

fn store_original(state: &AppState, name: &str, bytes: &[u8]) -> Result<String, String> {
    let extension = splitext(name).1.to_ascii_lowercase();
    if !SUPPORTED_AUDIO_FORMATS.contains(&extension.as_str()) {
        return Err(invalid_audio_format());
    }
    let id = format!("ses-{}", timeutil::stamp_now());
    let dir = okf::bundle_dir(&state.inner.data_dir, &id)?;
    let rel = format!("references/original{extension}");
    std::fs::create_dir_all(dir.join("references")).map_err(|error| error.to_string())?;
    std::fs::write(dir.join(&rel), bytes).map_err(|error| error.to_string())?;
    let stem = splitext(name).0;
    let mut note = okf::fresh_note(
        "opened",
        stem,
        &format!("Opened from {name}."),
        name,
        &format!("/{rel}"),
        &rel,
    );
    note.byte_size = bytes.len() as u64;
    note.analysis = "queued".into();
    okf::write_notes(&dir, &note)?;
    okf::append_log(&dir, &note.created_at, "**Creation**: Opened a file. The original bytes are in this package.")?;
    okf::write_catalog(&state.inner.data_dir)?;
    state.inner.jobs.send(id.clone()).map_err(|error| error.to_string())?;
    Ok(id)
}

async fn show_bundle(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    let Some(summary) = okf::read_summary(&dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    let cues = std::fs::read_to_string(dir.join("references").join("diarized_transcription.srt"))
        .map(|text| srt::parse(&text))
        .unwrap_or_default();
    let peaks = okf::read_peaks(&dir);
    let notice = analysis_error(&dir);
    respond(
        &state,
        &headers,
        Screen::Bundle(&id),
        Some(&id),
        view::session_stage(&summary, &cues, &peaks, &notice),
    )
}

async fn show_package(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    if okf::read_summary(&dir, &id).is_none() {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    }
    let (package_id, title, files) = package_files(&state, Some(&id));
    let stage = view::package_panel(package_id.as_deref(), &title, &files);
    respond(&state, &headers, Screen::Files, Some(&id), stage)
}

async fn show_analysis(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    let Some(summary) = okf::read_summary(&dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    if summary.analysis == "queued" || summary.analysis == "running" {
        return Html(format!(
            r#"<div id="analysis" hx-get="/bundles/{id}/analysis" hx-trigger="every 1s" hx-swap="outerHTML" class="rounded-xl border border-line bg-panel px-3 py-2 text-sm">Reading the take…</div>"#
        ))
        .into_response();
    }
    if summary.analysis == "error" {
        return Html(format!(
            r#"<p id="analysis" class="text-sm text-copper" role="alert">{}</p>"#,
            view::escape(&analysis_error(&dir))
        ))
        .into_response();
    }
    let cues = std::fs::read_to_string(dir.join("references").join("diarized_transcription.srt"))
        .map(|text| srt::parse(&text))
        .unwrap_or_default();
    Html(view::analysis_done(&cues)).into_response()
}

async fn concept(
    State(state): State<AppState>,
    headers: HeaderMap,
    UrlPath((id, path)): UrlPath<(String, String)>,
) -> Response {
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    let concept = match okf::read_concept(&dir, &path) {
        Ok(concept) => concept,
        Err(_) => return (StatusCode::NOT_FOUND, "No such note").into_response(),
    };
    respond(
        &state,
        &headers,
        Screen::Bundle(&id),
        Some(&id),
        view::concept_stage(&id, &concept),
    )
}

async fn media(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, &id) else {
        return (StatusCode::NOT_FOUND, "No such package").into_response();
    };
    let Some(path) = okf::media_file(&dir) else {
        return (StatusCode::NOT_FOUND, "No audio in this package").into_response();
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    };
    let mime = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    };
    let total = bytes.len();
    if let Some(range) = headers.get(header::RANGE).and_then(|value| value.to_str().ok()) {
        if let Some((start, end)) = parse_range(range, total) {
            let slice = bytes[start..=end].to_vec();
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                )
                .body(Body::from(slice))
                .unwrap();
        }
    }
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::from(bytes))
        .unwrap()
}

fn parse_range(header: &str, total: usize) -> Option<(usize, usize)> {
    let rest = header.trim().strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start: usize = if start.is_empty() { 0 } else { start.parse().ok()? };
    let end: usize = if end.is_empty() { total - 1 } else { end.parse().ok()? };
    if start >= total || end < start {
        return None;
    }
    Some((start, end.min(total - 1)))
}

async fn css() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/app.css"),
    )
        .into_response()
}

async fn js() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../static/app.js"),
    )
        .into_response()
}

async fn htmx() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_bytes!("../static/htmx.min.js").as_slice(),
    )
        .into_response()
}

async fn icon() -> Response {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#1c1915"/><circle cx="32" cy="32" r="14" fill="none" stroke="#e07a68" stroke-width="4"/><circle cx="32" cy="32" r="4" fill="#e07a68"/></svg>"##,
    )
        .into_response()
}

async fn manifest() -> Response {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        r##"{"name":"socioscope","short_name":"socioscope","display":"standalone","start_url":"/","background_color":"#f4efe6","theme_color":"#f4efe6","icons":[{"src":"/icon.svg","sizes":"any","type":"image/svg+xml","purpose":"any"}]}"##,
    )
        .into_response()
}

async fn font(UrlPath(name): UrlPath<String>) -> Response {
    let bytes: &[u8] = match name.as_str() {
        "outfit-400.woff2" => include_bytes!("../static/fonts/outfit-400.woff2"),
        "outfit-500.woff2" => include_bytes!("../static/fonts/outfit-500.woff2"),
        "outfit-600.woff2" => include_bytes!("../static/fonts/outfit-600.woff2"),
        "fraunces-500.woff2" => include_bytes!("../static/fonts/fraunces-500.woff2"),
        "fraunces-600.woff2" => include_bytes!("../static/fonts/fraunces-600.woff2"),
        _ => return (StatusCode::NOT_FOUND, "No such font").into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, "font/woff2"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

#[derive(Clone, Copy)]
enum Screen<'a> {
    Home,
    Record,
    Models,
    Catalog,
    Files,
    Bundle(&'a str),
}

fn page(state: &AppState, headers: &HeaderMap, screen: Screen, bundle: Option<&str>) -> Response {
    let stage = match screen {
        Screen::Home => view::home_stage(state.inner.full_tape.is_some()),
        Screen::Record => view::record_stage(state.inner.full_tape.is_some()),
        Screen::Catalog | Screen::Files | Screen::Bundle(_) | Screen::Models => String::new(),
    };
    respond(state, headers, screen, bundle, stage)
}

fn respond(state: &AppState, headers: &HeaderMap, screen: Screen, bundle: Option<&str>, stage: String) -> Response {
    let items = filtered(state, "");
    let bundle = selected_bundle(state, bundle);
    let (package_id, package_title, files) = package_files(state, bundle.as_deref());
    let package = view::package_panel(package_id.as_deref(), &package_title, &files);
    let target = headers
        .get("hx-target")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if hx(headers) && (target == "stage" || target == "#stage") {
        return Html(view::stage_swap(&stage, &view::library_items(&items), &package)).into_response();
    }
    let (tab, library_mobile, stage_mobile, package_mobile) = match screen {
        Screen::Home => ("library", true, true, false),
        Screen::Record => ("record", false, true, false),
        Screen::Models => ("models", false, true, false),
        Screen::Catalog => ("package", false, false, true),
        Screen::Files => ("package", false, true, false),
        Screen::Bundle(id) => {
            let _ = id;
            ("library", false, true, false)
        }
    };
    let package_href = bundle
        .map(|id| format!("/bundles/{id}/package"))
        .unwrap_or_else(|| "/package".into());
    let title = match screen {
        Screen::Record => "Record · socioscope",
        Screen::Models => "Models · socioscope",
        Screen::Catalog | Screen::Files => "Package · socioscope",
        Screen::Bundle(_) => "Session · socioscope",
        Screen::Home => "socioscope",
    };
    Html(view::document(&view::Shell {
        document_title: title,
        tab,
        package_href: &package_href,
        library: view::library(&items, ""),
        stage,
        package,
        library_mobile,
        stage_mobile,
        package_mobile,
    }))
    .into_response()
}

fn selected_bundle(state: &AppState, bundle: Option<&str>) -> Option<String> {
    if let Some(id) = bundle.filter(|id| !id.is_empty()) {
        return Some(id.to_string());
    }
    okf::list(&state.inner.data_dir)
        .ok()
        .and_then(|items| items.into_iter().next())
        .map(|item| item.id)
}

fn package_files(state: &AppState, bundle: Option<&str>) -> (Option<String>, String, Vec<(String, String)>) {
    let Some(id) = bundle else {
        return (None, String::new(), Vec::new());
    };
    let Ok(dir) = okf::bundle_dir(&state.inner.data_dir, id) else {
        return (None, String::new(), Vec::new());
    };
    let title = okf::read_summary(&dir, id)
        .map(|summary| summary.title)
        .unwrap_or_else(|| id.to_string());
    (Some(id.to_string()), title, okf::concepts(&dir))
}

fn filtered(state: &AppState, query: &str) -> Vec<okf::Summary> {
    let items = okf::list(&state.inner.data_dir).unwrap_or_default();
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| {
            item.title.to_ascii_lowercase().contains(&needle)
                || item.description.to_ascii_lowercase().contains(&needle)
        })
        .collect()
}

fn active(state: &AppState) -> Option<Arc<Mutex<Live>>> {
    let lives = state.inner.lives.lock().expect("lives");
    lives.values().find_map(|live| {
        let phase = live.lock().expect("live").phase;
        if matches!(phase, Phase::Preparing | Phase::Recording) {
            Some(Arc::clone(live))
        } else {
            None
        }
    })
}

fn analysis_error(dir: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(dir.join("transcript.md")) else {
        return "The read failed.".into();
    };
    let (_, body) = okf::split_frontmatter(&text);
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('*') && !line.starts_with('|'))
        .unwrap_or("The read failed.")
        .to_string()
}

async fn models_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (stage, _) = models_stage(&state, "");
    respond(&state, &headers, Screen::Models, None, stage)
}

async fn download_model(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    if !model_id_ok(&id) {
        return (StatusCode::BAD_REQUEST, "No such model").into_response();
    }
    let bin = state.inner.socioscope_bin.clone();
    let models_dir = state.inner.models_dir.clone();
    let config_path = state.inner.config_path.clone();
    let spawned = Command::new(&bin)
        .args(["models", "download", &id])
        .env("SOCIOSCOPE_MODELS", &models_dir)
        .env("SOCIOSCOPE_CONFIG", &config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let error = match spawned {
        Ok(child) => {
            thread::spawn(move || {
                let _ = child.wait_with_output();
            });
            tokio::time::sleep(Duration::from_millis(150)).await;
            String::new()
        }
        Err(error) => format!("Could not start {bin}: {error}", bin = bin.display()),
    };
    let (stage, _) = models_stage(&state, &error);
    respond(&state, &headers, Screen::Models, None, stage)
}

async fn use_model(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    if !model_id_ok(&id) {
        return (StatusCode::BAD_REQUEST, "No such model").into_response();
    }
    let bin = state.inner.socioscope_bin.clone();
    let models_dir = state.inner.models_dir.clone();
    let config_path = state.inner.config_path.clone();
    let switched = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["models", "use", &id])
            .env("SOCIOSCOPE_MODELS", models_dir)
            .env("SOCIOSCOPE_CONFIG", config_path)
            .output()
    })
    .await;
    let error = match switched {
        Ok(Ok(output)) if output.status.success() => String::new(),
        Ok(Ok(output)) => {
            let text = String::from_utf8_lossy(&output.stderr);
            let text = text.trim();
            if text.is_empty() {
                "The model was not switched.".into()
            } else {
                text.to_string()
            }
        }
        Ok(Err(error)) => error.to_string(),
        Err(error) => error.to_string(),
    };
    let (stage, _) = models_stage(&state, &error);
    respond(&state, &headers, Screen::Models, None, stage)
}

fn models_stage(state: &AppState, error: &str) -> (String, bool) {
    match shelf(state) {
        Ok(shelf) => (view::models_stage(&shelf.note, &shelf.models, error), true),
        Err(cli_error) => {
            let message = if error.is_empty() { cli_error } else { format!("{error} {cli_error}") };
            (view::models_stage("The model shelf comes from the socioscope CLI.", &[], &message), false)
        }
    }
}

fn shelf(state: &AppState) -> Result<socioscope::models::Shelf, String> {
    let output = Command::new(&state.inner.socioscope_bin)
        .args(["models", "list", "--json"])
        .env("SOCIOSCOPE_MODELS", &state.inner.models_dir)
        .env("SOCIOSCOPE_CONFIG", &state.inner.config_path)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("Could not start {}.", state.inner.socioscope_bin.display())
            } else {
                error.to_string()
            }
        })?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        let text = text.trim();
        return Err(if text.is_empty() {
            "The model shelf failed.".into()
        } else {
            text.to_string()
        });
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

fn model_id_ok(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn hx(headers: &HeaderMap) -> bool {
    headers.get("hx-request").is_some()
}

fn redirect(headers: &HeaderMap, location: &str) -> Response {
    if hx(headers) {
        let mut response = StatusCode::NO_CONTENT.into_response();
        response
            .headers_mut()
            .insert("hx-redirect", location.parse().unwrap());
        response
    } else {
        Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, location)
            .body(Body::empty())
            .unwrap()
    }
}
