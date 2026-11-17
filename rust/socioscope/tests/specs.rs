use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cucumber::{given, then, when, World};
use socioscope::cli::run;
use socioscope::error::Error;
use socioscope::transcription::{AudioTranscriber, Engine};

static TEMP_DIRS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, World)]
struct Cli {
    stdout: String,
    stderr: String,
    code: i32,
    dir: Option<PathBuf>,
    calls: Vec<PathBuf>,
}

struct RecordingTranscriber {
    calls: Vec<PathBuf>,
}

impl AudioTranscriber for RecordingTranscriber {
    fn transcribe_audio(&mut self, file_path: &Path) -> Result<(), Error> {
        self.calls.push(file_path.to_path_buf());
        Ok(())
    }
}

fn workspace() -> PathBuf {
    let id = TEMP_DIRS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("socioscope-specs-{id}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn invoke(cli: &mut Cli, args: &[&str], real: bool) {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = if real {
        let mut engine = Engine::new();
        run(&args, &mut engine, &mut stdout, &mut stderr)
    } else {
        let mut transcriber = RecordingTranscriber { calls: Vec::new() };
        let code = run(&args, &mut transcriber, &mut stdout, &mut stderr);
        cli.calls = transcriber.calls;
        code
    };
    cli.code = code;
    cli.stdout = String::from_utf8(stdout).unwrap();
    cli.stderr = String::from_utf8(stderr).unwrap();
}

#[given(expr = "a file named {string}")]
async fn a_file(cli: &mut Cli, name: String) {
    let dir = workspace();
    fs::write(dir.join(&name), b"not audio").unwrap();
    cli.dir = Some(dir);
}

#[given(expr = "a directory containing {string}, {string}, and {string}")]
async fn a_directory(cli: &mut Cli, first: String, second: String, third: String) {
    let dir = workspace();
    for name in [first, second, third] {
        fs::write(dir.join(name), b"RIFF").unwrap();
    }
    cli.dir = Some(dir);
}

#[given(expr = "a subdirectory named {string}")]
async fn a_subdirectory(cli: &mut Cli, name: String) {
    let dir = cli.dir.as_ref().expect("directory");
    fs::create_dir_all(dir.join(name)).unwrap();
}

#[when(expr = "I run {string}")]
async fn i_run(cli: &mut Cli, command: String) {
    let mut parts = command.split_whitespace();
    let program = parts.next().unwrap_or("socioscope");
    let mut args = vec![program];
    args.extend(parts);
    invoke(cli, &args, false);
}

#[when("I transcribe that file")]
async fn transcribe_file(cli: &mut Cli) {
    let dir = cli.dir.as_ref().expect("file directory");
    let file = fs::read_dir(dir)
        .unwrap()
        .find_map(|entry| entry.ok().map(|entry| entry.path()))
        .expect("file");
    let path = file.display().to_string();
    invoke(cli, &["socioscope", "transcribe", &path], true);
}

#[when("I transcribe that directory")]
async fn transcribe_directory(cli: &mut Cli) {
    let path = cli.dir.as_ref().expect("directory").display().to_string();
    invoke(cli, &["socioscope", "transcribe", &path], false);
}

#[then(expr = "the exit code is {int}")]
async fn exit_code(cli: &mut Cli, code: i32) {
    assert_eq!(
        cli.code, code,
        "stdout:\n{}\nstderr:\n{}",
        cli.stdout, cli.stderr
    );
}

#[then(expr = "stdout contains {string}")]
async fn stdout_contains(cli: &mut Cli, text: String) {
    assert!(cli.stdout.contains(&text), "stdout:\n{}", cli.stdout);
}

#[then("stdout is empty")]
async fn stdout_empty(cli: &mut Cli) {
    assert!(cli.stdout.is_empty(), "stdout:\n{}", cli.stdout);
}

#[then(expr = "stderr contains {string}")]
async fn stderr_contains(cli: &mut Cli, text: String) {
    assert!(cli.stderr.contains(&text), "stderr:\n{}", cli.stderr);
}

#[then("stderr is empty")]
async fn stderr_empty(cli: &mut Cli) {
    assert!(cli.stderr.is_empty(), "stderr:\n{}", cli.stderr);
}

#[then("stderr is the argparse missing FILE error")]
async fn missing_file(cli: &mut Cli) {
    assert_eq!(
        cli.stderr,
        "usage: socioscope transcribe [-h] FILE\nsocioscope transcribe: error: the following arguments are required: FILE\n"
    );
}

#[then(expr = "stderr is the argparse unknown command error for {string}")]
async fn unknown_command(cli: &mut Cli, command: String) {
    assert_eq!(
        cli.stderr,
        format!(
            "usage: socioscope [-h] {{transcribe}} ...\nsocioscope: error: argument command: invalid choice: '{command}' (choose from 'transcribe')\n"
        )
    );
}

#[then(expr = "{int} files are transcribed")]
async fn files_transcribed(cli: &mut Cli, count: usize) {
    assert_eq!(cli.calls.len(), count, "{:?}", cli.calls);
}

#[tokio::main]
async fn main() {
    Cli::run("specs").await;
}
