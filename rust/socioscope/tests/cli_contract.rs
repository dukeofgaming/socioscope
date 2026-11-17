use std::fs;
use std::path::PathBuf;

use socioscope::cli::run;
use socioscope::error::Error;
use socioscope::messages::INVALID_AUDIO_TRANSCRIPTION_FORMAT;
use socioscope::transcription::AudioTranscriber;

struct RecordingTranscriber {
    calls: Vec<PathBuf>,
}

impl AudioTranscriber for RecordingTranscriber {
    fn transcribe_audio(&mut self, file_path: &std::path::Path) -> Result<(), Error> {
        self.calls.push(file_path.to_path_buf());
        Ok(())
    }
}

fn invoke(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut transcriber = RecordingTranscriber { calls: Vec::new() };
    let code = run(&args, &mut transcriber, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn test_that_cli_help_message_displays_correctly() {
    let (code, out, err) = invoke(&["socioscope", "-h"]);
    assert_eq!(code, 0);
    assert!(err.is_empty());
    assert!(out.contains("usage: socioscope [-h]"));
    assert!(out.contains("show this help message and exit"));
    assert!(out.contains("socioscope CLI tool"));
    assert!(out.contains("transcribe  Transcribe an m4a audio file"));
}

#[test]
fn test_error_transcribe_audio_format() {
    let dir = std::env::temp_dir().join("socioscope-cli-format");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("audio.txt");
    fs::write(&file, b"not audio").unwrap();

    let args = vec![
        "socioscope".to_string(),
        "transcribe".to_string(),
        file.display().to_string(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut engine = socioscope::transcription::Engine::new();
    let code = run(&args, &mut engine, &mut stdout, &mut stderr);
    let error = String::from_utf8(stderr).unwrap();

    assert_eq!(code, 0);
    let prefix = INVALID_AUDIO_TRANSCRIPTION_FORMAT.replace("{file_extensions}", "");
    assert!(error.contains(&prefix), "{error}");
    assert!(error.contains(".wav, .m4a, .mp3"), "{error}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_transcribe_directory() {
    let dir = std::env::temp_dir().join("socioscope-cli-directory");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in ["mock_file1.wav", "mock_file2.wav", "mock_file3.wav"] {
        fs::write(dir.join(name), b"RIFF").unwrap();
    }
    fs::create_dir_all(dir.join("somedirectory")).unwrap();

    let args = vec![
        "socioscope".to_string(),
        "transcribe".to_string(),
        dir.display().to_string(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut transcriber = RecordingTranscriber { calls: Vec::new() };
    let code = run(&args, &mut transcriber, &mut stdout, &mut stderr);

    assert_eq!(code, 0);
    assert_eq!(transcriber.calls.len(), 3, "{:?}", transcriber.calls);
    assert!(String::from_utf8(stderr).unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_argument_matches_argparse() {
    let (code, _out, err) = invoke(&["socioscope", "transcribe"]);
    assert_eq!(code, 2);
    assert_eq!(
        err,
        "usage: socioscope transcribe [-h] FILE\nsocioscope transcribe: error: the following arguments are required: FILE\n"
    );
}

#[test]
fn unknown_command_matches_argparse() {
    let (code, _out, err) = invoke(&["socioscope", "nope"]);
    assert_eq!(code, 2);
    assert_eq!(
        err,
        "usage: socioscope [-h] {transcribe} ...\nsocioscope: error: argument command: invalid choice: 'nope' (choose from 'transcribe')\n"
    );
}
