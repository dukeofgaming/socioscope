use std::path::Path;

use serde_json::{json, Value};

use crate::error::Error;
use crate::json_io::{read_json, write_json};

/// Attach every diarization turn that overlaps a whisper segment.
///
/// Overlap matches the Python test: `d.from < t.to and d.to > t.from`.
pub fn merge_diarization_transcription(
    diarization_json: &Path,
    transcription_json: &Path,
    output_file: &Path,
) -> Result<(), Error> {
    let diarization_data = read_json(diarization_json)?;
    let transcription_root = read_json(transcription_json)?;
    let transcription_data = transcription_root
        .get("transcription")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("transcription.json has no transcription list"))?;
    let diarization_data = diarization_data
        .as_array()
        .ok_or_else(|| Error::new("diarization.json is not a list"))?;

    let mut merged_segments = Vec::new();
    for segment in transcription_data {
        let t_start = offset(segment, "from")?;
        let t_end = offset(segment, "to")?;
        let text = segment
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let timestamps = segment
            .get("timestamps")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let relevant_diarizations: Vec<Value> = diarization_data
            .iter()
            .filter(|turn| {
                let Ok(from) = offset(turn, "from") else {
                    return false;
                };
                let Ok(to) = offset(turn, "to") else {
                    return false;
                };
                from < t_end && to > t_start
            })
            .map(|turn| {
                json!({
                    "timestamps": turn.get("timestamps").cloned().unwrap_or_else(|| json!({})),
                    "offsets": turn.get("offsets").cloned().unwrap_or_else(|| json!({})),
                    "speaker": turn.get("speaker").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();

        merged_segments.push(json!({
            "timestamps": timestamps,
            "offsets": { "from": t_start, "to": t_end },
            "text": text,
            "diarization": relevant_diarizations,
        }));
    }

    write_json(output_file, &merged_segments)
}

fn offset(value: &Value, side: &str) -> Result<i64, Error> {
    value
        .get("offsets")
        .and_then(|offsets| offsets.get(side))
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::new(format!("missing offsets.{side}")))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn overlap_keeps_only_turns_that_intersect_the_segment() {
        let dir = std::env::temp_dir().join("socioscope-merge-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let diarization = dir.join("diarization.json");
        let transcription = dir.join("transcription.json");
        let merged = dir.join("diarized_transcription.json");

        write_json(
            &diarization,
            &json!([
                {
                    "timestamps": {"from": "00:00:00,000", "to": "00:00:00,400"},
                    "offsets": {"from": 0, "to": 400},
                    "speaker": "SPEAKER_01"
                },
                {
                    "timestamps": {"from": "00:00:00,500", "to": "00:00:01,500"},
                    "offsets": {"from": 500, "to": 1500},
                    "speaker": "SPEAKER_00"
                },
                {
                    "timestamps": {"from": "00:00:02,000", "to": "00:00:03,000"},
                    "offsets": {"from": 2000, "to": 3000},
                    "speaker": "SPEAKER_02"
                }
            ]),
        )
        .unwrap();
        write_json(
            &transcription,
            &json!({
                "transcription": [{
                    "timestamps": {"from": "00:00:00,000", "to": "00:00:01,000"},
                    "offsets": {"from": 0, "to": 1000},
                    "text": " hello"
                }]
            }),
        )
        .unwrap();

        merge_diarization_transcription(&diarization, &transcription, &merged).unwrap();
        let merged: Value = read_json(&merged).unwrap();
        let turns = merged[0]["diarization"].as_array().unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0]["speaker"], "SPEAKER_01");
        assert_eq!(turns[1]["speaker"], "SPEAKER_00");
        assert_eq!(merged[0]["text"], " hello");
        let _ = fs::remove_dir_all(&dir);
    }
}
