use std::path::Path;

use serde_json::{json, Value};

use crate::audio::convert_to_wav;
use crate::config::Config;
use crate::diarize::Diarizer;
use crate::error::Error;
use crate::json_io::write_json;
use crate::merge::merge_diarization_transcription;
use crate::messages::{invalid_audio_format, SUPPORTED_AUDIO_FORMATS};
use crate::paths::{get_output_directory_path, get_source_wav_path, splitext};
use crate::srt::{convert_diarized_json_to_srt, write_transcription_srt};
use crate::whisper::WhisperEngine;

pub trait AudioTranscriber {
    fn transcribe_audio(&mut self, file_path: &Path) -> Result<(), Error>;
}

/// Holds the whisper and diarization models so a directory does not reload them.
pub struct Engine {
    whisper: WhisperEngine,
    diarizer: Diarizer,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            whisper: WhisperEngine::new(),
            diarizer: Diarizer::new(),
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioTranscriber for Engine {
    fn transcribe_audio(&mut self, file_path: &Path) -> Result<(), Error> {
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let extension = splitext(file_name).1;
        let output_directory = get_output_directory_path(file_path);
        std::fs::create_dir_all(&output_directory).map_err(|error| {
            Error::new(format!("creating {}: {error}", output_directory.display()))
        })?;

        if !SUPPORTED_AUDIO_FORMATS.contains(&extension) {
            return Err(Error::new(invalid_audio_format()));
        }

        if extension != ".wav" {
            convert_to_wav(file_path, &output_directory)?;
        }

        println!("Transcribing {}...", file_path.display());
        let config = Config::load()?;
        let source_wav = get_source_wav_path(file_path);
        let segments = self
            .whisper
            .transcribe(&config.transcription.model, &source_wav)?;

        let transcription: Vec<Value> = segments
            .iter()
            .map(|segment| {
                json!({
                    "timestamps": {"from": segment.start, "to": segment.end},
                    "offsets": {"from": segment.start_ms, "to": segment.end_ms},
                    "text": segment.text,
                })
            })
            .collect();
        write_json(
            &output_directory.join("transcription.json"),
            &json!({ "transcription": transcription }),
        )?;
        write_transcription_srt(
            &segments
                .iter()
                .map(|segment| {
                    (
                        segment.start.clone(),
                        segment.end.clone(),
                        segment.text.clone(),
                    )
                })
                .collect::<Vec<_>>(),
            &output_directory.join("transcription.srt"),
        )?;
        println!(
            "Transcription complete. Output written to {}/transcription.srt",
            output_directory.display()
        );

        println!("Diarizing {}...", file_path.display());
        let diarization = config.diarization.as_ref().ok_or_else(|| {
            Error::new(
                "config.yaml is missing diarization.segmentation_model and diarization.embedding_model",
            )
        })?;
        self.diarizer
            .diarize(diarization, &source_wav, &output_directory)?;

        println!(
            "Merging diarization and transcription for {}...",
            file_path.display()
        );
        let diarized_json = output_directory.join("diarized_transcription.json");
        merge_diarization_transcription(
            &output_directory.join("diarization.json"),
            &output_directory.join("transcription.json"),
            &diarized_json,
        )?;
        println!(
            "Transcription complete. Output written to {}/diarized_transcription.json",
            output_directory.display()
        );

        println!(
            "Converting diarized transcription to SRT for {}...",
            file_path.display()
        );
        let diarized_srt = output_directory.join("diarized_transcription.srt");
        convert_diarized_json_to_srt(&diarized_json, &diarized_srt)?;
        println!(
            "Conversion complete. Output written to {}",
            diarized_srt.display()
        );
        Ok(())
    }
}
