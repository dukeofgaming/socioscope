use std::env;
use std::io::{self, Write};
use std::process::exit;

use socioscope::cli;
use socioscope::transcription::{AudioTranscriber, Engine};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut engine = Engine::new();
    let code = cli::run(
        &args,
        &mut engine as &mut dyn AudioTranscriber,
        &mut io::stdout(),
        &mut io::stderr(),
    );
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    exit(code);
}
