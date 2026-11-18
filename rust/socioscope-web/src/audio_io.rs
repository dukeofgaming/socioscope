use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Command;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

pub const SAMPLE_RATE: u32 = 16_000;

pub fn decode_wav_bytes(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let reader = WavReader::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    decode_reader(reader)
}

pub fn decode_file(path: &Path) -> Result<Vec<f32>, String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
    {
        let reader = WavReader::open(path).map_err(|error| error.to_string())?;
        return decode_reader(reader);
    }
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            path.to_str().ok_or("audio path is not utf-8")?,
            "-f",
            "f32le",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-",
        ])
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "ffmpeg is not installed".to_string()
            } else {
                error.to_string()
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("could not read {}: {stderr}", path.display()));
    }
    let mut samples = Vec::with_capacity(output.stdout.len() / 4);
    let mut chunks = output.stdout.chunks_exact(4);
    for chunk in chunks.by_ref() {
        samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(samples)
}

pub fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|error| error.to_string())?;
    for sample in samples {
        let clipped = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(clipped)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())
}

pub fn peaks(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let window = (SAMPLE_RATE / 10) as usize;
    samples
        .chunks(window)
        .map(|chunk| {
            let energy = chunk.iter().map(|sample| sample * sample).sum::<f32>() / chunk.len() as f32;
            energy.sqrt().clamp(0.0, 1.0)
        })
        .collect()
}

fn decode_reader<R: Read>(reader: WavReader<R>) -> Result<Vec<f32>, String> {
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err("audio has no channels".to_string());
    }
    let interleaved = match spec.sample_format {
        SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .into_samples::<i32>()
                .map(|sample| sample.map(|sample| sample as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        }
        SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?,
    };
    let mut mono = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum / channels as f32);
    }
    Ok(resample(&mono, spec.sample_rate, SAMPLE_RATE))
}

fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == 0 || from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (samples.len() as f64 / ratio).round() as usize;
    (0..out_len)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let mix = (position - left as f64) as f32;
            samples[left] * (1.0 - mix) + samples[right] * mix
        })
        .collect()
}
