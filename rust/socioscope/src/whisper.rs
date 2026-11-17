use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::load_mono_f32;
use crate::error::Error;
use crate::time::{convert_to_milliseconds, convert_to_timestamp};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
const THREADS: i32 = 16;

pub struct TranscriptSegment {
    pub start: String,
    pub end: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

pub struct WhisperEngine {
    context: Option<WhisperContext>,
    model_path: Option<String>,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self {
            context: None,
            model_path: None,
        }
    }

    pub fn transcribe(
        &mut self,
        model_path: &str,
        wav_path: &Path,
    ) -> Result<Vec<TranscriptSegment>, Error> {
        if self.model_path.as_deref() != Some(model_path) {
            let params = WhisperContextParameters::default();
            let context = WhisperContext::new_with_params(model_path, params)
                .map_err(|error| Error::new(error.to_string()))?;
            self.context = Some(context);
            self.model_path = Some(model_path.to_string());
        }
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| Error::new("whisper model is not loaded"))?;
        let mut state = context
            .create_state()
            .map_err(|error| Error::new(error.to_string()))?;
        let samples = load_mono_f32(wav_path, WHISPER_SAMPLE_RATE)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(THREADS);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state
            .full(params, &samples)
            .map_err(|error| Error::new(error.to_string()))?;

        let mut segments = Vec::new();
        let count = state.full_n_segments();
        for index in 0..count {
            let Some(segment) = state.get_segment(index) else {
                continue;
            };
            let start_cs = segment.start_timestamp();
            let end_cs = segment.end_timestamp();
            let start_seconds = start_cs as f64 / 100.0;
            let end_seconds = end_cs as f64 / 100.0;
            let text = segment
                .to_str()
                .map_err(|error| Error::new(error.to_string()))?
                .to_string();
            segments.push(TranscriptSegment {
                start: convert_to_timestamp(start_seconds),
                end: convert_to_timestamp(end_seconds),
                start_ms: convert_to_milliseconds(start_seconds),
                end_ms: convert_to_milliseconds(end_seconds),
                text,
            });
        }
        Ok(segments)
    }
}

impl Default for WhisperEngine {
    fn default() -> Self {
        Self::new()
    }
}
