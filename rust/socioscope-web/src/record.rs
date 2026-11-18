//! Plays a public tape as the microphone and stores the samples that have "arrived".

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::audio_io::{self, SAMPLE_RATE};
use crate::okf;
use crate::srt::{self, Cue};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Recording,
    Failed,
    Done,
}

pub struct Live {
    pub id: String,
    pub title: String,
    pub source_label: String,
    pub phase: Phase,
    pub error: String,
    pub elapsed_ms: u64,
    pub duration_ms: u64,
    pub peaks: Vec<f32>,
    pub cues: Vec<Cue>,
}

pub enum Tape {
    Wav(Vec<u8>),
    File(PathBuf),
}

pub struct Start {
    pub data_dir: PathBuf,
    pub id: String,
    pub title: String,
    pub description: String,
    pub source_label: String,
    pub source_url: String,
    pub tape: Tape,
    pub cues: Vec<Cue>,
    pub pace: f64,
    pub live: Arc<Mutex<Live>>,
    pub stop: Arc<AtomicBool>,
}

pub fn begin(start: Start) {
    thread::spawn(move || run(start));
}

fn run(start: Start) {
    let decoded = match &start.tape {
        Tape::Wav(bytes) => audio_io::decode_wav_bytes(bytes),
        Tape::File(path) => audio_io::decode_file(path),
    };
    let samples = match decoded {
        Ok(samples) => samples,
        Err(error) => {
            fail(&start.live, error);
            return;
        }
    };
    if start.stop.load(Ordering::Relaxed) {
        fail(&start.live, "Stopped before the tape started.".to_string());
        return;
    }
    if samples.is_empty() {
        fail(&start.live, "The tape has no samples.".to_string());
        return;
    }

    let duration_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;
    {
        let mut live = start.live.lock().expect("live");
        live.phase = Phase::Recording;
        live.duration_ms = duration_ms;
        live.cues = start.cues.clone();
    }

    let dir = match okf::bundle_dir(&start.data_dir, &start.id) {
        Ok(dir) => dir,
        Err(error) => {
            fail(&start.live, error);
            return;
        }
    };
    let mut note = okf::fresh_note(
        "emulated",
        &start.title,
        &start.description,
        &start.source_label,
        &start.source_url,
        "references/recording.wav",
    );
    note.duration_ms = duration_ms;
    if let Err(error) = okf::write_notes(&dir, &note) {
        fail(&start.live, error);
        return;
    }
    if let Err(error) = okf::append_log(
        &dir,
        &note.created_at,
        "**Creation**: Started an emulated capture. The microphone is the public debate tape.",
    ) {
        fail(&start.live, error);
        return;
    }
    let _ = okf::write_catalog(&start.data_dir);

    let pace = if start.pace <= 0.0 { 1.0 } else { start.pace };
    let started = Instant::now();
    let mut written = 0usize;
    let mut last_note = Instant::now();
    loop {
        if start.stop.load(Ordering::Relaxed) {
            break;
        }
        let target = ((started.elapsed().as_secs_f64() * pace) * SAMPLE_RATE as f64) as usize;
        let target = target.min(samples.len());
        if target > written {
            written = target;
            let elapsed_ms = written as u64 * 1000 / SAMPLE_RATE as u64;
            let peaks = audio_io::peaks(&samples[..written]);
            let mut live = start.live.lock().expect("live");
            live.elapsed_ms = elapsed_ms;
            live.peaks = peaks;
        }
        if last_note.elapsed() >= Duration::from_millis(700) {
            note.cues = srt::visible(&start.cues, elapsed_of(written));
            note.duration_ms = duration_ms;
            note.byte_size = (written * 2) as u64;
            let _ = okf::write_notes(&dir, &note);
            last_note = Instant::now();
        }
        if written == samples.len() {
            break;
        }
        thread::sleep(Duration::from_millis(40));
    }

    let captured = &samples[..written];
    let elapsed_ms = elapsed_of(written);
    note.cues = srt::visible(&start.cues, elapsed_ms);
    note.duration_ms = elapsed_ms;
    note.status = "stable".into();
    note.byte_size = (captured.len() * 2) as u64;
    if let Err(error) = audio_io::write_wav(&dir.join("references").join("recording.wav"), captured) {
        fail(&start.live, error);
        return;
    }
    if let Err(error) = okf::write_notes(&dir, &note) {
        fail(&start.live, error);
        return;
    }
    let _ = okf::write_peaks(&dir, &audio_io::peaks(captured));
    let _ = okf::append_log(
        &dir,
        &note.created_at,
        "**Update**: Stored the original take and the cues that had arrived.",
    );
    let _ = okf::write_catalog(&start.data_dir);
    let mut live = start.live.lock().expect("live");
    live.elapsed_ms = elapsed_ms;
    live.duration_ms = elapsed_ms;
    live.peaks = audio_io::peaks(captured);
    live.phase = Phase::Done;
}

fn elapsed_of(samples: usize) -> u64 {
    samples as u64 * 1000 / SAMPLE_RATE as u64
}

fn fail(live: &Mutex<Live>, error: String) {
    if let Ok(mut live) = live.lock() {
        live.phase = Phase::Failed;
        live.error = error;
    }
}
