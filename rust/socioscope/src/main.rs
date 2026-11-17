use std::env;
use std::io::{self, Write};
use std::process::exit;

use socioscope::cli;
use socioscope::transcription::{AudioTranscriber, Engine};

fn main() {
    let code = run();
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    // Drop the whisper context before exit. process::exit skips Rust destructors,
    // and ggml Metal aborts if GPU buffers are still resident at process teardown.
    exit(code);
}

fn run() -> i32 {
    let args: Vec<String> = env::args().collect();
    let mut engine = Engine::new();
    cli::run(
        &args,
        &mut engine as &mut dyn AudioTranscriber,
        &mut io::stdout(),
        &mut io::stderr(),
    )
}
