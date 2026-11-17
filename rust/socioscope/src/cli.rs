use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::transcription::AudioTranscriber;

const MAIN_HELP: &str = "\
usage: socioscope [-h] {transcribe} ...

socioscope CLI tool

positional arguments:
  {transcribe}
    transcribe  Transcribe an m4a audio file

options:
  -h, --help    show this help message and exit
";

const TRANSCRIBE_HELP: &str = "\
usage: socioscope transcribe [-h] FILE

positional arguments:
  FILE        The path to the m4a file to transcribe

options:
  -h, --help  show this help message and exit
";

pub fn run(
    args: &[String],
    transcriber: &mut dyn AudioTranscriber,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let rest = &args[1..];
    if rest.is_empty() || rest[0] == "-h" || rest[0] == "--help" {
        let _ = write!(stdout, "{MAIN_HELP}");
        return 0;
    }
    if rest[0] == "transcribe" {
        return transcribe(&rest[1..], transcriber, stdout, stderr);
    }
    if rest[0].starts_with('-') {
        let _ = write!(
            stderr,
            "usage: socioscope [-h] {{transcribe}} ...\nsocioscope: error: unrecognized arguments: {}\n",
            rest[0]
        );
        return 2;
    }
    let _ = write!(
        stderr,
        "usage: socioscope [-h] {{transcribe}} ...\nsocioscope: error: argument command: invalid choice: '{}' (choose from 'transcribe')\n",
        rest[0]
    );
    2
}

fn transcribe(
    args: &[String],
    transcriber: &mut dyn AudioTranscriber,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        let _ = write!(stdout, "{TRANSCRIBE_HELP}");
        return 0;
    }
    let mut positionals = Vec::new();
    for arg in args {
        if arg.starts_with('-') {
            let _ = write!(
                stderr,
                "usage: socioscope transcribe [-h] FILE\nsocioscope transcribe: error: unrecognized arguments: {arg}\n"
            );
            return 2;
        }
        positionals.push(arg);
    }
    match positionals.as_slice() {
        [] => {
            let _ = write!(
                stderr,
                "usage: socioscope transcribe [-h] FILE\nsocioscope transcribe: error: the following arguments are required: FILE\n"
            );
            2
        }
        [file] => transcribe_path(file, transcriber, stderr),
        [_, extras @ ..] => {
            let extras = extras
                .iter()
                .map(|arg| arg.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let _ = write!(
                stderr,
                "usage: socioscope transcribe [-h] FILE\nsocioscope transcribe: error: unrecognized arguments: {extras}\n"
            );
            2
        }
    }
}

fn transcribe_path(
    file: &str,
    transcriber: &mut dyn AudioTranscriber,
    stderr: &mut dyn Write,
) -> i32 {
    let path = Path::new(file);
    let files = match files_to_transcribe(path) {
        Ok(files) => files,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return 0;
        }
    };
    for file in files {
        if let Err(error) = transcriber.transcribe_audio(&file) {
            let _ = writeln!(stderr, "{error}");
        }
    }
    0
}

fn files_to_transcribe(path: &Path) -> Result<Vec<std::path::PathBuf>, Error> {
    if path.is_dir() {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|error| Error::new(error.to_string()))? {
            let entry = entry.map_err(|error| Error::new(error.to_string()))?;
            let file_path = entry.path();
            if file_path.is_file() {
                files.push(file_path);
            }
        }
        Ok(files)
    } else {
        Ok(vec![path.to_path_buf()])
    }
}
