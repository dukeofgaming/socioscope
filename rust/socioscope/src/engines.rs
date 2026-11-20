//! Extra transcription and diarization engines.
//!
//! Whisper and pyannote stay in-process. Cactus, audio.cpp, and Desert Ant
//! are the runners for the other weights on the shelf. Each one is fetched
//! into the models directory the first time a transcription needs it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::error::Error;
use crate::time::{convert_to_milliseconds, convert_to_timestamp};
use crate::whisper::TranscriptSegment;

const SAMPLE_RATE: f64 = 16_000.0;
const CACTUS_VERSION: &str = "4.1.3";
const AUDIOCPP_VERSION: &str = "v0.9.5";

pub fn transcribe_cactus(model: &Path, wav: &Path) -> Result<Vec<TranscriptSegment>, Error> {
    let bin = cactus_bin()?;
    let output = Command::new(&bin)
        .args([
            "--model",
            &model.display().to_string(),
            "--audio",
            &wav.display().to_string(),
            "--audio-word-timestamps",
        ])
        .output()
        .map_err(|error| Error::new(format!("starting {}: {error}", bin.display())))?;
    if !output.status.success() {
        return Err(Error::new(command_error("cactus", &output)));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(text.trim())
        .map_err(|error| Error::new(format!("cactus did not return JSON: {error}")))?;
    Ok(segments_from_words(&value, false))
}

pub fn transcribe_audiocpp(family: &str, model: &Path, wav: &Path) -> Result<Vec<TranscriptSegment>, Error> {
    let bin = audiocpp_bin()?;
    let words = std::env::temp_dir().join(format!("socioscope-words-{}.json", std::process::id()));
    let output = audio_command(&bin)?
        .args([
            "--task",
            "asr",
            "--family",
            family,
            "--model",
            &model.display().to_string(),
            "--backend",
            backend(),
            "--audio",
            &wav.display().to_string(),
            "--words-out",
            &words.display().to_string(),
        ])
        .output()
        .map_err(|error| Error::new(format!("starting {}: {error}", bin.display())))?;
    if !output.status.success() {
        return Err(Error::new(command_error("audiocpp", &output)));
    }
    let text = fs::read_to_string(&words).map_err(|error| {
        Error::new(format!(
            "audio.cpp wrote no word times ({}). {}",
            error,
            command_error("audiocpp", &output)
        ))
    })?;
    let _ = fs::remove_file(&words);
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| Error::new(format!("audio.cpp word times are not JSON: {error}")))?;
    Ok(segments_from_words(&value, true))
}

pub fn diarize_audiocpp(family: &str, model: &Path, wav: &Path) -> Result<Vec<(f64, f64, String)>, Error> {
    let bin = audiocpp_bin()?;
    let turns = std::env::temp_dir().join(format!("socioscope-turns-{}.json", std::process::id()));
    let output = audio_command(&bin)?
        .args([
            "--task",
            "diar",
            "--family",
            family,
            "--model",
            &model.display().to_string(),
            "--backend",
            backend(),
            "--audio",
            &wav.display().to_string(),
            "--turns-out",
            &turns.display().to_string(),
        ])
        .output()
        .map_err(|error| Error::new(format!("starting {}: {error}", bin.display())))?;
    if !output.status.success() {
        return Err(Error::new(command_error("audiocpp", &output)));
    }
    let text = fs::read_to_string(&turns).map_err(|error| {
        Error::new(format!(
            "audio.cpp wrote no speaker turns ({}). {}",
            error,
            command_error("audiocpp", &output)
        ))
    })?;
    let _ = fs::remove_file(&turns);
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| Error::new(format!("audio.cpp speaker turns are not JSON: {error}")))?;
    turns_from_json(&value, true)
}

pub fn transcribe_desertant(model_dir: &Path, wav: &Path) -> Result<Vec<TranscriptSegment>, Error> {
    let value = desertant("transcribe", model_dir, wav)?;
    Ok(segments_from_words(&value, false))
}

pub fn diarize_desertant(model_dir: &Path, wav: &Path) -> Result<Vec<(f64, f64, String)>, Error> {
    let value = desertant("diarize", model_dir, wav)?;
    turns_from_json(&value, false)
}

fn desertant(command: &str, model_dir: &Path, wav: &Path) -> Result<Value, Error> {
    if let Ok(program) = std::env::var("SOCIOSCOPE_DESERTANT") {
        let out = std::env::temp_dir().join(format!(
            "socioscope-desertant-{}-{}.json",
            command,
            std::process::id()
        ));
        let output = Command::new(&program)
            .args([
                command,
                &model_dir.display().to_string(),
                &wav.display().to_string(),
                &out.display().to_string(),
            ])
            .output()
            .map_err(|error| Error::new(format!("starting {program}: {error}")))?;
        if !output.status.success() {
            return Err(Error::new(command_error("desertant", &output)));
        }
        let text = fs::read_to_string(&out).map_err(|error| Error::new(error.to_string()))?;
        let _ = fs::remove_file(&out);
        return serde_json::from_str(&text).map_err(|error| Error::new(error.to_string()));
    }
    let dir = ensure_desertant()?;
    let out = std::env::temp_dir().join(format!(
        "socioscope-desertant-{}-{}.json",
        command,
        std::process::id()
    ));
    let node = std::env::var("SOCIOSCOPE_NODE").unwrap_or_else(|_| "node".into());
    let output = Command::new(&node)
        .arg(dir.join("run.mjs"))
        .args([
            command,
            &model_dir.display().to_string(),
            &wav.display().to_string(),
            &out.display().to_string(),
        ])
        .current_dir(&dir)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::new("node is not installed, so Desert Ant Voz and Who cannot run")
            } else {
                Error::new(format!("starting node: {error}"))
            }
        })?;
    if !output.status.success() {
        return Err(Error::new(command_error("desertant", &output)));
    }
    let text = fs::read_to_string(&out).map_err(|error| Error::new(error.to_string()))?;
    let _ = fs::remove_file(&out);
    serde_json::from_str(&text).map_err(|error| Error::new(error.to_string()))
}

fn ensure_desertant() -> Result<PathBuf, Error> {
    let dir = crate::models::models_dir_from_env().join("engines").join("desertant");
    fs::create_dir_all(&dir).map_err(|error| Error::new(error.to_string()))?;
    fs::write(dir.join("package.json"), include_str!("../engines/desertant/package.json"))
        .map_err(|error| Error::new(error.to_string()))?;
    fs::write(dir.join("run.mjs"), include_str!("../engines/desertant/run.mjs"))
        .map_err(|error| Error::new(error.to_string()))?;
    if !dir.join("node_modules").join("@desert-ant-labs").join("voz").is_dir() {
        let status = Command::new("npm")
            .args(["install", "--omit=dev", "--no-fund", "--no-audit"])
            .current_dir(&dir)
            .status()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Error::new("npm is not installed, so Desert Ant Voz and Who cannot be installed")
                } else {
                    Error::new(error.to_string())
                }
            })?;
        if !status.success() {
            return Err(Error::new("npm could not install the Desert Ant engines"));
        }
    }
    Ok(dir)
}

fn cactus_bin() -> Result<PathBuf, Error> {
    if let Ok(path) = std::env::var("SOCIOSCOPE_CACTUS") {
        return Ok(PathBuf::from(path));
    }
    let name = if cfg!(windows) { "cactus.exe" } else { "cactus" };
    let dest = crate::models::models_dir_from_env()
        .join("engines")
        .join("cactus")
        .join(name);
    if dest.is_file() {
        return Ok(dest);
    }
    let (folder, bytes) = cactus_asset()?;
    let url = format!(
        "https://huggingface.co/Cactus-Compute/cactus/resolve/main/releases/{CACTUS_VERSION}/{folder}/cactus"
    );
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::new(error.to_string()))?;
    }
    download(&url, &dest, bytes)?;
    make_executable(&dest)?;
    Ok(dest)
}

fn cactus_asset() -> Result<(&'static str, u64), Error> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(("linux-x86_64", 1_602_640)),
        ("macos", "aarch64") => Ok(("macos-arm64", 1_139_768)),
        (os, arch) => Err(Error::new(format!(
            "Cactus publishes no runner for {os} {arch}. Whistle runs on Linux x86_64 and Apple silicon."
        ))),
    }
}

fn audiocpp_bin() -> Result<PathBuf, Error> {
    if let Ok(path) = std::env::var("SOCIOSCOPE_AUDIOCPP") {
        return Ok(PathBuf::from(path));
    }
    let root = crate::models::models_dir_from_env().join("engines").join("audiocpp");
    if let Some(found) = find_named(&root, "audiocpp_cli") {
        return Ok(found);
    }
    let asset = audiocpp_asset()?;
    fs::create_dir_all(&root).map_err(|error| Error::new(error.to_string()))?;
    let archive = root.join(asset);
    let url = format!("https://github.com/0xShug0/audio.cpp/releases/download/{AUDIOCPP_VERSION}/{asset}");
    download(&url, &archive, 0)?;
    let status = Command::new("tar")
        .args(["-xzf", &archive.display().to_string(), "-C", &root.display().to_string()])
        .status()
        .map_err(|error| Error::new(format!("tar: {error}")))?;
    if !status.success() {
        return Err(Error::new("could not unpack audio.cpp"));
    }
    find_named(&root, "audiocpp_cli").ok_or_else(|| Error::new("the audio.cpp archive has no audiocpp_cli"))
}

fn audiocpp_asset() -> Result<&'static str, Error> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("audio-v0.9.5-bin-ubuntu-x64-cpu-portable.tar.gz"),
        ("macos", "aarch64") => Ok("audio-v0.9.5-bin-macos-arm64-metal.tar.gz"),
        ("macos", "x86_64") => Ok("audio-v0.9.5-bin-macos-x64-metal.tar.gz"),
        (os, arch) => Err(Error::new(format!(
            "audio.cpp publishes no build for {os} {arch}. Parakeet and Nemotron 3 need Linux x86_64 or macOS."
        ))),
    }
}

fn audio_command(bin: &Path) -> Result<Command, Error> {
    let mut command = Command::new(bin);
    if let Some(dir) = bin.parent() {
        let joined = std::env::join_paths(
            std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(
                &std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default(),
            )),
        )
        .map_err(|error| Error::new(error.to_string()))?;
        command.env("LD_LIBRARY_PATH", joined);
    }
    Ok(command)
}

fn backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    }
}

fn download(url: &str, dest: &Path, bytes: u64) -> Result<(), Error> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::new(error.to_string()))?;
    }
    let partial = PathBuf::from(format!("{}.partial", dest.display()));
    let status = Command::new("curl")
        .args([
            "-L",
            "--fail",
            "-sS",
            "--retry",
            "3",
            "--retry-delay",
            "1",
            "-o",
            &partial.display().to_string(),
            url,
        ])
        .status()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::new("curl is not installed")
            } else {
                Error::new(error.to_string())
            }
        })?;
    if !status.success() {
        let _ = fs::remove_file(&partial);
        return Err(Error::new(format!("could not download {url}")));
    }
    if bytes > 0 {
        let size = fs::metadata(&partial).map(|meta| meta.len()).unwrap_or(0);
        if size != bytes {
            let _ = fs::remove_file(&partial);
            return Err(Error::new(format!("{url} is {size} bytes, expected {bytes}")));
        }
    }
    fs::rename(&partial, dest).map_err(|error| Error::new(error.to_string()))
}

fn make_executable(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).map_err(|error| Error::new(error.to_string()))?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| Error::new(error.to_string()))?;
    }
    Ok(())
}

fn find_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|file| file.to_str()) == Some(name) && path.is_file() {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_named(&path, name) {
                return Some(found);
            }
        }
    }
    None
}

fn command_error(name: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if !stderr.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    if detail.is_empty() {
        format!("{name} failed")
    } else {
        format!("{name} failed: {detail}")
    }
}

fn segments_from_words(value: &Value, samples: bool) -> Vec<TranscriptSegment> {
    let words = word_list(value);
    let mut pieces = Vec::new();
    for word in words {
        let Some(text) = word
            .get("word")
            .or_else(|| word.get("text"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let (start, end) = span(word, samples);
        if end < start {
            continue;
        }
        pieces.push((start, end, text.trim().to_string()));
    }
    if pieces.is_empty() {
        let text = value.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if text.is_empty() {
            return Vec::new();
        }
        return vec![segment(0.0, 0.0, text)];
    }
    let mut grouped: Vec<(f64, f64, String)> = Vec::new();
    for (start, end, text) in pieces {
        let gap = grouped.last().map(|(_, previous, _)| start - previous).unwrap_or(0.0);
        let long = grouped.last().map(|(_, _, line)| line.split_whitespace().count() >= 16).unwrap_or(false);
        if grouped.is_empty() || gap > 0.8 || long {
            grouped.push((start, end, text));
        } else if let Some(current) = grouped.last_mut() {
            current.1 = end;
            if !current.2.is_empty() && !text.is_empty() {
                current.2.push(' ');
            }
            current.2.push_str(&text);
        }
    }
    grouped
        .into_iter()
        .map(|(start, end, text)| segment(start, end, text))
        .collect()
}

fn turns_from_json(value: &Value, samples: bool) -> Result<Vec<(f64, f64, String)>, Error> {
    let list = value
        .as_array()
        .or_else(|| value.get("turns").and_then(Value::as_array))
        .or_else(|| value.get("speaker_turns").and_then(Value::as_array))
        .ok_or_else(|| Error::new("diarization JSON has no turns"))?;
    let mut turns = Vec::new();
    for turn in list {
        let (start, end) = span(turn, samples);
        let raw = turn
            .get("speaker_id")
            .or_else(|| turn.get("speaker"))
            .map(|speaker| match speaker {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                _ => "0".into(),
            })
            .unwrap_or_else(|| "0".into());
        turns.push((start, end, speaker_label(&raw)));
    }
    Ok(turns)
}

fn word_list(value: &Value) -> &[Value] {
    value
        .get("words")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn span(value: &Value, samples: bool) -> (f64, f64) {
    if samples {
        let start = value.get("start_sample").and_then(Value::as_f64).unwrap_or(0.0) / SAMPLE_RATE;
        let end = value.get("end_sample").and_then(Value::as_f64).unwrap_or(start * SAMPLE_RATE) / SAMPLE_RATE;
        return (start, end);
    }
    if value.get("start").is_some() || value.get("end").is_some() {
        let start = value.get("start").and_then(Value::as_f64).unwrap_or(0.0);
        let end = value.get("end").and_then(Value::as_f64).unwrap_or(start);
        return (start, end);
    }
    let start = value.get("start_sample").and_then(Value::as_f64).unwrap_or(0.0) / SAMPLE_RATE;
    let end = value.get("end_sample").and_then(Value::as_f64).unwrap_or(0.0) / SAMPLE_RATE;
    (start, end)
}

fn segment(start: f64, end: f64, text: String) -> TranscriptSegment {
    TranscriptSegment {
        start: convert_to_timestamp(start),
        end: convert_to_timestamp(end),
        start_ms: convert_to_milliseconds(start),
        end_ms: convert_to_milliseconds(end),
        text,
    }
}

fn speaker_label(raw: &str) -> String {
    if raw.starts_with("SPEAKER_") {
        return raw.to_string();
    }
    let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
    let number: u32 = digits.parse().unwrap_or(0);
    format!("SPEAKER_{number:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cactus_words_group_on_a_pause() {
        let value = serde_json::json!({
            "text": "ask not",
            "words": [
                {"word": "ask", "start": 0.1, "end": 0.4},
                {"word": "not", "start": 0.4, "end": 0.8},
                {"word": "what", "start": 2.0, "end": 2.4}
            ]
        });
        let segments = segments_from_words(&value, false);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "ask not");
        assert_eq!(segments[1].text, "what");
        assert_eq!(segments[0].start_ms, 100);
    }

    #[test]
    fn audiocpp_samples_become_seconds() {
        let value = serde_json::json!([
            {"word": "hello", "start_sample": 16000, "end_sample": 32000, "confidence": 1.0}
        ]);
        let segments = segments_from_words(&value, true);
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].start_ms, 1000);
        assert_eq!(segments[0].end_ms, 2000);
        let turns = turns_from_json(
            &serde_json::json!([{"speaker_id": "1", "start_sample": 0, "end_sample": 16000}]),
            true,
        )
        .unwrap();
        assert_eq!(turns, vec![(0.0, 1.0, "SPEAKER_01".into())]);
    }

    #[cfg(unix)]
    #[test]
    fn a_stand_in_runner_is_what_the_engine_calls() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("socioscope-engines-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cactus = dir.join("cactus");
        fs::write(
            &cactus,
            "#!/bin/sh\nprintf '%s\\n' '{\"words\":[{\"word\":\"ask\",\"start\":0.1,\"end\":0.4}]}'\n",
        )
        .unwrap();
        let audio = dir.join("audiocpp");
        fs::write(
            &audio,
            r#"#!/bin/sh
task=asr
out=
while [ $# -gt 0 ]; do
  case "$1" in
    --task) task=$2; shift 2 ;;
    --words-out|--turns-out) out=$2; shift 2 ;;
    *) shift ;;
  esac
done
if [ "$task" = diar ]; then
  printf '%s\n' '[{"speaker_id":1,"start_sample":0,"end_sample":16000}]' > "$out"
else
  printf '%s\n' '[{"word":"hello","start_sample":16000,"end_sample":32000}]' > "$out"
fi
"#,
        )
        .unwrap();
        let desert = dir.join("desertant");
        fs::write(
            &desert,
            r#"#!/bin/sh
if [ "$1" = diarize ]; then
  printf '%s\n' '{"turns":[{"speaker":"0","start":0.0,"end":1.5}]}' > "$4"
else
  printf '%s\n' '{"words":[{"text":"hello","start":0.2,"end":0.6}]}' > "$4"
fi
"#,
        )
        .unwrap();
        for path in [&cactus, &audio, &desert] {
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
        std::env::set_var("SOCIOSCOPE_CACTUS", &cactus);
        std::env::set_var("SOCIOSCOPE_AUDIOCPP", &audio);
        std::env::set_var("SOCIOSCOPE_DESERTANT", &desert);
        let wav = dir.join("clip.wav");
        fs::write(&wav, b"RIFF").unwrap();
        let model = dir.join("model");
        fs::write(&model, b"weights").unwrap();

        let segments = transcribe_cactus(&model, &wav).unwrap();
        assert_eq!(segments[0].text, "ask");
        let segments = transcribe_audiocpp("parakeet_tdt", &model, &wav).unwrap();
        assert_eq!(segments[0].text, "hello");
        assert_eq!(segments[0].start_ms, 1000);
        let turns = diarize_audiocpp("nemotron_3_diar", &model, &wav).unwrap();
        assert_eq!(turns, vec![(0.0, 1.0, "SPEAKER_01".into())]);
        let segments = transcribe_desertant(&dir, &wav).unwrap();
        assert_eq!(segments[0].text, "hello");
        let turns = diarize_desertant(&dir, &wav).unwrap();
        assert_eq!(turns, vec![(0.0, 1.5, "SPEAKER_00".into())]);

        std::env::remove_var("SOCIOSCOPE_CACTUS");
        std::env::remove_var("SOCIOSCOPE_AUDIOCPP");
        std::env::remove_var("SOCIOSCOPE_DESERTANT");
    }
}
