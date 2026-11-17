use std::path::Path;

use serde_json::{json, Value};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractorConfig,
};

use crate::audio::load_mono_f32;
use crate::config::DiarizationConfig;
use crate::error::Error;
use crate::json_io::write_json;
use crate::time::{convert_to_milliseconds, convert_to_timestamp};

pub struct Diarizer {
    engine: Option<OfflineSpeakerDiarization>,
    key: Option<(String, String, i32, u32)>,
}

impl Diarizer {
    pub fn new() -> Self {
        Self {
            engine: None,
            key: None,
        }
    }

    pub fn diarize(
        &mut self,
        config: &DiarizationConfig,
        wav_path: &Path,
        output_directory: &Path,
    ) -> Result<(), Error> {
        let threshold_bits = config.threshold.to_bits();
        let key = (
            config.segmentation_model.clone(),
            config.embedding_model.clone(),
            config.num_clusters,
            threshold_bits,
        );
        if self.key.as_ref() != Some(&key) {
            println!("Diarization: preparing pipeline");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            self.engine = Some(load(config)?);
            self.key = Some(key);
        }
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| Error::new("diarization pipeline is not loaded"))?;

        println!("Diarization: running pipeline");
        let sample_rate = engine.sample_rate();
        if sample_rate <= 0 {
            return Err(Error::new("diarization model reported a sample rate of 0"));
        }
        let samples = load_mono_f32(wav_path, sample_rate as u32)?;
        let result = engine
            .process(&samples)
            .ok_or_else(|| Error::new("diarization produced no result"))?;

        let mut segments = Vec::new();
        for turn in result.sort_by_start_time() {
            let start = f64::from(turn.start);
            let end = f64::from(turn.end);
            let start_timestamp = convert_to_timestamp(start);
            let end_timestamp = convert_to_timestamp(end);
            let speaker = format!("SPEAKER_{:02}", turn.speaker);
            println!("start={start_timestamp} stop={end_timestamp} speaker_{speaker}");
            segments.push(json!({
                "timestamps": { "from": start_timestamp, "to": end_timestamp },
                "offsets": {
                    "from": convert_to_milliseconds(start),
                    "to": convert_to_milliseconds(end),
                },
                "speaker": speaker,
            }));
        }

        write_json(
            &output_directory.join("diarization.json"),
            &Value::Array(segments.clone()),
        )?;
        println!("Diarization: writing RTTM");
        write_rttm(
            &output_directory.join("diarization.rttm"),
            wav_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("<NA>"),
            &segments,
        )
    }
}

impl Default for Diarizer {
    fn default() -> Self {
        Self::new()
    }
}

fn load(config: &DiarizationConfig) -> Result<OfflineSpeakerDiarization, Error> {
    let diarization = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(config.segmentation_model.clone()),
                ..Default::default()
            },
            num_threads: 4,
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(config.embedding_model.clone()),
            num_threads: 4,
            ..Default::default()
        },
        clustering: FastClusteringConfig {
            num_clusters: config.num_clusters,
            threshold: config.threshold,
            ..Default::default()
        },
        ..Default::default()
    };
    OfflineSpeakerDiarization::create(&diarization)
        .ok_or_else(|| Error::new("failed to initialize offline speaker diarization"))
}

fn write_rttm(path: &Path, uri: &str, segments: &[Value]) -> Result<(), Error> {
    let mut body = String::new();
    for segment in segments {
        let start_ms = segment["offsets"]["from"].as_i64().unwrap_or(0);
        let end_ms = segment["offsets"]["to"].as_i64().unwrap_or(start_ms);
        let start = start_ms as f64 / 1000.0;
        let duration = (end_ms - start_ms) as f64 / 1000.0;
        let speaker = segment["speaker"].as_str().unwrap_or("SPEAKER_00");
        body.push_str(&format!(
            "SPEAKER {uri} 1 {start:.3} {duration:.3} <NA> <NA> {speaker} <NA> <NA>\n"
        ));
    }
    std::fs::write(path, body)
        .map_err(|error| Error::new(format!("writing {}: {error}", path.display())))
}
