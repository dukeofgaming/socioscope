use std::collections::HashMap;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use axum::extract::{Form, Path as UrlPath, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use socioscope::cli;
use socioscope::paths::get_output_directory_path;
use socioscope::transcription::{AudioTranscriber, Engine};

const INDEX: &str = include_str!("../static/index.html");
const HTMX: &[u8] = include_bytes!("../static/htmx.min.js");

pub struct Outcome {
    pub code: i32,
    pub stderr: String,
    pub output: String,
}

trait Transcribe {
    fn transcribe(&mut self, path: &str) -> Outcome;
}

struct EngineTranscriber {
    engine: Engine,
}

impl Transcribe for EngineTranscriber {
    fn transcribe(&mut self, path: &str) -> Outcome {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let args = vec![
            "socioscope".to_string(),
            "transcribe".to_string(),
            path.to_string(),
        ];
        let code = cli::run(
            &args,
            &mut self.engine as &mut dyn AudioTranscriber,
            &mut stdout,
            &mut stderr,
        );
        Outcome {
            code,
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            output: read_result(Path::new(path)),
        }
    }
}

struct ClosureTranscriber<F>(F);

impl<F> Transcribe for ClosureTranscriber<F>
where
    F: FnMut(&str) -> Outcome,
{
    fn transcribe(&mut self, path: &str) -> Outcome {
        (self.0)(path)
    }
}

struct Job {
    path: String,
    phase: Phase,
    outcome: Option<Outcome>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Running,
    Finished,
}

struct Jobs {
    next_id: u64,
    by_id: HashMap<u64, Job>,
}

#[derive(Clone)]
struct AppState {
    jobs: Arc<Mutex<Jobs>>,
    tx: mpsc::Sender<u64>,
}

/// Serves the UI with one transcriber thread, so whisper and the diarizer load once.
pub fn router() -> Router {
    serve(|| EngineTranscriber {
        engine: Engine::new(),
    })
}

/// Same UI, with the transcription step supplied by the caller. Used by tests.
pub fn router_with<F>(transcribe: F) -> Router
where
    F: FnMut(&str) -> Outcome + Send + 'static,
{
    serve(|| ClosureTranscriber(transcribe))
}

fn serve<T, F>(build: F) -> Router
where
    F: FnOnce() -> T + Send + 'static,
    T: Transcribe,
{
    let jobs = Arc::new(Mutex::new(Jobs {
        next_id: 1,
        by_id: HashMap::new(),
    }));
    let (tx, rx) = mpsc::channel();
    let worker_jobs = Arc::clone(&jobs);
    thread::spawn(move || {
        let mut transcriber = build();
        while let Ok(id) = rx.recv() {
            let path = {
                let mut jobs = worker_jobs.lock().expect("jobs");
                let job = jobs.by_id.get_mut(&id).expect("job");
                job.phase = Phase::Running;
                job.path.clone()
            };
            let outcome = transcriber.transcribe(&path);
            let mut jobs = worker_jobs.lock().expect("jobs");
            let job = jobs.by_id.get_mut(&id).expect("job");
            job.phase = Phase::Finished;
            job.outcome = Some(outcome);
        }
    });

    Router::new()
        .route("/", get(index))
        .route("/htmx.min.js", get(htmx))
        .route("/jobs", post(create_job))
        .route("/jobs/{id}", get(show_job))
        .with_state(AppState { jobs, tx })
}

async fn index() -> Html<&'static str> {
    Html(INDEX)
}

async fn htmx() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        HTMX,
    )
        .into_response()
}

#[derive(Deserialize)]
struct NewJob {
    file: String,
}

async fn create_job(State(state): State<AppState>, Form(form): Form<NewJob>) -> Html<String> {
    let path = form.file.trim();
    if path.is_empty() {
        return Html(r#"<p class="error">Enter a file or directory.</p>"#.to_string());
    }

    let id = {
        let mut jobs = state.jobs.lock().expect("jobs");
        let id = jobs.next_id;
        jobs.next_id += 1;
        jobs.by_id.insert(
            id,
            Job {
                path: path.to_string(),
                phase: Phase::Queued,
                outcome: None,
            },
        );
        id
    };
    let _ = state.tx.send(id);
    let jobs = state.jobs.lock().expect("jobs");
    Html(render_job(id, jobs.by_id.get(&id).expect("job")))
}

async fn show_job(State(state): State<AppState>, UrlPath(id): UrlPath<u64>) -> Response {
    let jobs = state.jobs.lock().expect("jobs");
    match jobs.by_id.get(&id) {
        Some(job) => Html(render_job(id, job)).into_response(),
        None => (StatusCode::NOT_FOUND, "No such job").into_response(),
    }
}

fn render_job(id: u64, job: &Job) -> String {
    let path = escape(&job.path);
    match job.phase {
        Phase::Queued => polling(id, &format!("Queued <code>{path}</code>")),
        Phase::Running => polling(id, &format!("Transcribing <code>{path}</code>")),
        Phase::Finished => {
            let outcome = job.outcome.as_ref().expect("finished job");
            let heading = if outcome.code == 0 {
                "Finished"
            } else {
                "Failed"
            };
            let mut body = format!("<p>{heading} <code>{path}</code></p>");
            if !outcome.stderr.is_empty() {
                body.push_str("<pre class=\"error\">");
                body.push_str(&escape(&outcome.stderr));
                body.push_str("</pre>");
            }
            if !outcome.output.is_empty() {
                body.push_str("<pre>");
                body.push_str(&escape(&outcome.output));
                body.push_str("</pre>");
            }
            format!(r#"<article id="job-{id}">{body}</article>"#)
        }
    }
}

fn polling(id: u64, body: &str) -> String {
    format!(
        r#"<article id="job-{id}" hx-get="/jobs/{id}" hx-trigger="every 1s" hx-swap="outerHTML"><p>{body}</p></article>"#
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn read_result(path: &Path) -> String {
    if path.is_dir() {
        let mut files = match std::fs::read_dir(path) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .collect::<Vec<_>>(),
            Err(_) => return String::new(),
        };
        files.retain(|file| file.is_file());
        files.sort();
        let mut output = String::new();
        for file in files {
            let srt = srt_for(&file);
            if srt.is_empty() {
                continue;
            }
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&file.display().to_string());
            output.push('\n');
            output.push_str(&srt);
        }
        return output;
    }
    srt_for(path)
}

fn srt_for(path: &Path) -> String {
    let srt = get_output_directory_path(path).join("diarized_transcription.srt");
    std::fs::read_to_string(srt).unwrap_or_default()
}
