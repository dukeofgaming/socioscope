use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::Error;
use crate::json_io::read_json;

/// Speaker-labelled SRT. Speakers are sorted so the line is stable.
pub fn convert_diarized_json_to_srt(input_file: &Path, output_file: &Path) -> Result<(), Error> {
    let data = read_json(input_file)?;
    let segments = data
        .as_array()
        .ok_or_else(|| Error::new("diarized transcription is not a list"))?;
    let mut lines = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let start = timestamp(segment, "from")?;
        let end = timestamp(segment, "to")?;
        let text = segment.get("text").and_then(Value::as_str).unwrap_or("");
        let speakers = unique_speakers(segment);
        lines.push((index + 1).to_string());
        lines.push(format!("{start} --> {end}"));
        lines.push(format!("[{speakers}] {text}"));
        lines.push(String::new());
    }
    fs::write(output_file, lines.join("\n"))
        .map_err(|error| Error::new(format!("writing {}: {error}", output_file.display())))
}

pub fn write_transcription_srt(
    segments: &[(String, String, String)],
    output_file: &Path,
) -> Result<(), Error> {
    let mut lines = Vec::new();
    for (index, (start, end, text)) in segments.iter().enumerate() {
        lines.push((index + 1).to_string());
        lines.push(format!("{start} --> {end}"));
        lines.push(text.clone());
        lines.push(String::new());
    }
    fs::write(output_file, lines.join("\n"))
        .map_err(|error| Error::new(format!("writing {}: {error}", output_file.display())))
}

fn timestamp(segment: &Value, side: &str) -> Result<String, Error> {
    segment
        .get("timestamps")
        .and_then(|timestamps| timestamps.get(side))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::new(format!("missing timestamps.{side}")))
}

fn unique_speakers(segment: &Value) -> String {
    let Some(turns) = segment.get("diarization").and_then(Value::as_array) else {
        return String::new();
    };
    let speakers: BTreeSet<&str> = turns
        .iter()
        .filter_map(|turn| turn.get("speaker").and_then(Value::as_str))
        .collect();
    speakers.into_iter().collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::json_io::write_json;

    #[test]
    fn srt_sorts_speakers_and_keeps_whisper_spacing() {
        let dir = std::env::temp_dir().join("socioscope-srt-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("diarized_transcription.json");
        let output = dir.join("diarized_transcription.srt");
        write_json(
            &input,
            &json!([{
                "timestamps": {"from": "00:00:00,000", "to": "00:00:01,000"},
                "offsets": {"from": 0, "to": 1000},
                "text": " hello",
                "diarization": [
                    {"speaker": "SPEAKER_01"},
                    {"speaker": "SPEAKER_00"},
                    {"speaker": "SPEAKER_01"}
                ]
            }]),
        )
        .unwrap();

        convert_diarized_json_to_srt(&input, &output).unwrap();
        let srt = fs::read_to_string(&output).unwrap();
        assert_eq!(
            srt,
            "1\n00:00:00,000 --> 00:00:01,000\n[SPEAKER_00, SPEAKER_01]  hello\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
