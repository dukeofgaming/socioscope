use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::transcription::AudioTranscriber;

const MAIN_HELP: &str = "\
usage: socioscope [-h] {transcribe,models} ...

socioscope CLI tool

positional arguments:
  {transcribe,models}
    transcribe  Transcribe an m4a audio file
    models      List, download, and select transcription and diarization models

options:
  -h, --help    show this help message and exit
";

const MODELS_HELP: &str = "\
usage: socioscope models [-h] {list,download,use} ...

positional arguments:
  {list,download,use}
    list      Show the model shelf
    download  Download a model by id
    use       Select a downloaded model this build can run

options:
  -h, --help  show this help message and exit
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
    if rest[0] == "models" {
        return models(&rest[1..], stdout, stderr);
    }
    if rest[0].starts_with('-') {
        let _ = write!(
            stderr,
            "usage: socioscope [-h] {{transcribe,models}} ...\nsocioscope: error: unrecognized arguments: {}\n",
            rest[0]
        );
        return 2;
    }
    let _ = write!(
        stderr,
        "usage: socioscope [-h] {{transcribe,models}} ...\nsocioscope: error: argument command: invalid choice: '{}' (choose from 'transcribe', 'models')\n",
        rest[0]
    );
    2
}

fn models(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if args.is_empty() || (args.len() == 1 && (args[0] == "-h" || args[0] == "--help")) {
        let _ = write!(stdout, "{MODELS_HELP}");
        return 0;
    }
    match args[0].as_str() {
        "list" => models_list(&args[1..], stdout, stderr),
        "download" => models_id(&args[1..], stdout, stderr, true),
        "use" => models_id(&args[1..], stdout, stderr, false),
        "-h" | "--help" => {
            let _ = write!(stdout, "{MODELS_HELP}");
            0
        }
        other if other.starts_with('-') => {
            let _ = write!(
                stderr,
                "usage: socioscope models [-h] {{list,download,use}} ...\nsocioscope models: error: unrecognized arguments: {other}\n"
            );
            2
        }
        other => {
            let _ = write!(
                stderr,
                "usage: socioscope models [-h] {{list,download,use}} ...\nsocioscope models: error: argument command: invalid choice: '{other}' (choose from 'list', 'download', 'use')\n"
            );
            2
        }
    }
}

fn models_list(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if arg == "-h" || arg == "--help" {
            let _ = write!(stdout, "usage: socioscope models list [-h] [--json]\n");
            return 0;
        } else {
            let _ = write!(
                stderr,
                "usage: socioscope models list [-h] [--json]\nsocioscope models list: error: unrecognized arguments: {arg}\n"
            );
            return 2;
        }
    }
    match crate::models::list(&crate::models::models_dir_from_env()) {
        Ok(shelf) => {
            if json {
                match serde_json::to_string(&shelf) {
                    Ok(text) => {
                        let _ = writeln!(stdout, "{text}");
                    }
                    Err(error) => {
                        let _ = writeln!(stderr, "{error}");
                        return 1;
                    }
                }
            } else {
                let _ = write!(stdout, "{}", crate::models::render_text(&shelf));
            }
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            1
        }
    }
}

fn models_id(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write, download: bool) -> i32 {
    let verb = if download { "download" } else { "use" };
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        let _ = write!(stdout, "usage: socioscope models {verb} [-h] ID\n");
        return 0;
    }
    let mut id = None;
    for arg in args {
        if arg.starts_with('-') {
            let _ = write!(
                stderr,
                "usage: socioscope models {verb} [-h] ID\nsocioscope models {verb}: error: unrecognized arguments: {arg}\n"
            );
            return 2;
        }
        if id.is_some() {
            let _ = write!(
                stderr,
                "usage: socioscope models {verb} [-h] ID\nsocioscope models {verb}: error: unrecognized arguments: {arg}\n"
            );
            return 2;
        }
        id = Some(arg.as_str());
    }
    let Some(id) = id else {
        let _ = write!(
            stderr,
            "usage: socioscope models {verb} [-h] ID\nsocioscope models {verb}: error: the following arguments are required: ID\n"
        );
        return 2;
    };
    let dir = crate::models::models_dir_from_env();
    let result = if download {
        crate::models::download(id, &dir)
    } else {
        crate::models::activate(id, &dir, &crate::models::config_path_from_env())
    };
    match result {
        Ok(message) => {
            let _ = writeln!(stdout, "{message}");
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            1
        }
    }
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
