use std::path::{Path, PathBuf};
use std::process::Command;

use hound::{SampleFormat, WavReader};

use crate::error::Error;
use crate::paths::get_output_directory_path;

/// ffmpeg to 16 kHz wav. Flags match `convert_to_wav`.
pub fn convert_to_wav(file_path: &Path, output_directory_path: &Path) -> Result<PathBuf, Error> {
    let output_file_path = output_directory_path.join("converted.wav");
    println!(
        "Converting {} to {}...",
        file_path.display(),
        output_file_path.display()
    );

    let args = [
        "-i",
        file_path
            .to_str()
            .ok_or_else(|| Error::new("audio path is not utf-8"))?,
        "-ar",
        "16000",
        "-y",
        output_file_path
            .to_str()
            .ok_or_else(|| Error::new("output path is not utf-8"))?,
    ];
    let status = match Command::new("ffmpeg").args(args).status() {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::new("[Errno 2] No such file or directory: 'ffmpeg'"));
        }
        Err(error) => return Err(Error::new(error.to_string())),
    };
    if !status.success() {
        let code = status.code().unwrap_or(1);
        let rendered = format!(
            "['ffmpeg', '-i', '{}', '-ar', '16000', '-y', '{}']",
            file_path.display(),
            output_file_path.display()
        );
        return Err(Error::new(format!(
            "Command {rendered} returned non-zero exit status {code}."
        )));
    }

    println!(
        "Conversion complete. Output written to {}",
        output_file_path.display()
    );
    Ok(output_file_path)
}

pub fn output_directory_for(file_path: &Path) -> PathBuf {
    get_output_directory_path(file_path)
}

/// Mono f32 samples at `target_rate`. Whisper and the diarizer both want this.
pub fn load_mono_f32(path: &Path, target_rate: u32) -> Result<Vec<f32>, Error> {
    let reader = WavReader::open(path)
        .map_err(|error| Error::new(format!("{}: {error}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err(Error::new(format!(
            "{} has no audio channels",
            path.display()
        )));
    }
    let source_rate = spec.sample_rate;
    let interleaved = read_interleaved(reader, spec.sample_format, spec.bits_per_sample)?;
    let mono = downmix(&interleaved, channels);
    Ok(resample(&mono, source_rate, target_rate))
}

fn read_interleaved(
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    format: SampleFormat,
    bits: u16,
) -> Result<Vec<f32>, Error> {
    match (format, bits) {
        (SampleFormat::Float, _) => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Error::new(error.to_string())),
        (SampleFormat::Int, 16) => reader
            .into_samples::<i16>()
            .map(|sample| sample.map(|value| value as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Error::new(error.to_string())),
        (SampleFormat::Int, 32) => reader
            .into_samples::<i32>()
            .map(|sample| sample.map(|value| value as f32 / 2_147_483_648.0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Error::new(error.to_string())),
        _ => Err(Error::new(format!(
            "unsupported wav format {format:?} {bits}-bit"
        ))),
    }
}

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 || to == 0 {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((input.len() as f64) / ratio).round() as usize;
    let mut output = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let fraction = (position - left as f64) as f32;
        let right = (left + 1).min(input.len() - 1);
        let left = left.min(input.len() - 1);
        output.push(input[left] + (input[right] - input[left]) * fraction);
    }
    output
}
