use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, Player};

/// Rodio reports the position after the speed change. A 2× player at 5s has
/// consumed 10s of the file. Seeking uses the inverse so the file stays put.
pub fn file_ms(heard: Duration, speed: f32) -> u64 {
    let speed = usable_speed(speed);
    (heard.as_secs_f64() * f64::from(speed) * 1000.0).round() as u64
}

pub fn heard_for(file_ms: u64, speed: f32) -> Duration {
    let speed = usable_speed(speed);
    Duration::from_secs_f64(file_ms as f64 / 1000.0 / f64::from(speed))
}

fn usable_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed >= 0.25 { speed } else { 1.0 }
}

struct Desired {
    path: PathBuf,
    file_ms: u64,
    speed: f32,
    playing: bool,
    token: u64,
}

enum Command {
    Apply(Desired),
    Stop,
}

struct Shared {
    position_ms: AtomicU64,
    ended: AtomicBool,
    failed: AtomicBool,
    ready: AtomicBool,
    error: Mutex<Option<String>>,
}

impl Shared {
    fn fail(&self, message: String) {
        self.failed.store(true, Ordering::Relaxed);
        self.ready.store(false, Ordering::Relaxed);
        if let Ok(mut slot) = self.error.lock() {
            *slot = Some(message);
        }
    }
}

struct Engine {
    sink: Option<rodio::MixerDeviceSink>,
    player: Option<Player>,
    path: Option<PathBuf>,
    speed: f32,
    token: u64,
    playing: bool,
    device_dead: bool,
}

impl Engine {
    fn reconcile(&mut self, desired: Option<&Desired>, shared: &Shared) {
        let Some(desired) = desired else {
            self.player = None;
            self.path = None;
            self.playing = false;
            self.device_dead = false;
            return;
        };
        if desired.token != self.token {
            self.device_dead = false;
        }
        if self.device_dead {
            return;
        }
        if self.sink.is_none() {
            match DeviceSinkBuilder::open_default_sink() {
                Ok(sink) => self.sink = Some(sink),
                Err(err) => {
                    self.device_dead = true;
                    self.token = desired.token;
                    shared.fail(format!("No audio output. {err}"));
                    return;
                }
            }
        }
        let same_file = self.path.as_deref() == Some(desired.path.as_path());
        if same_file && self.player.as_ref().is_some_and(|player| player.empty()) && self.token == desired.token {
            shared.ended.store(true, Ordering::Relaxed);
            self.playing = false;
            return;
        }
        let reload = !same_file || self.player.as_ref().is_none_or(|player| player.empty());
        if reload {
            let mixer = self.sink.as_ref().unwrap().mixer();
            let player = Player::connect_new(mixer);
            player.pause();
            player.set_speed(desired.speed);
            let file = match File::open(&desired.path) {
                Ok(file) => file,
                Err(err) => {
                    shared.fail(format!("Could not open the audio. {err}"));
                    self.player = None;
                    self.path = None;
                    return;
                }
            };
            match Decoder::try_from(file) {
                Ok(decoder) => player.append(decoder),
                Err(err) => {
                    shared.fail(format!("Could not decode the audio. {err}"));
                    self.player = None;
                    self.path = None;
                    return;
                }
            }
            self.player = Some(player);
            self.path = Some(desired.path.clone());
            self.token = u64::MAX;
        }
        let player = self.player.as_ref().unwrap();
        if self.token != desired.token || (self.speed - desired.speed).abs() > 0.001 {
            player.set_speed(desired.speed);
            if let Err(err) = player.try_seek(heard_for(desired.file_ms, desired.speed)) {
                shared.fail(format!("Could not seek the audio. {err}"));
            }
            self.token = desired.token;
            self.speed = desired.speed;
            shared.position_ms.store(desired.file_ms, Ordering::Relaxed);
        }
        if desired.playing {
            player.play();
            shared.failed.store(false, Ordering::Relaxed);
            shared.ready.store(true, Ordering::Relaxed);
            shared.ended.store(false, Ordering::Relaxed);
            if let Ok(mut slot) = shared.error.lock() {
                *slot = None;
            }
        } else {
            player.pause();
        }
        self.playing = desired.playing;
    }

    fn publish(&self, shared: &Shared) {
        let Some(player) = &self.player else {
            return;
        };
        if !self.playing || player.is_paused() {
            return;
        }
        if player.empty() {
            shared.ended.store(true, Ordering::Relaxed);
            return;
        }
        shared.position_ms.store(file_ms(player.get_pos(), self.speed), Ordering::Relaxed);
    }
}

fn serve(rx: Receiver<Command>, shared: Arc<Shared>) {
    #[allow(unused_assignments)]
    let mut desired: Option<Desired> = None;
    let mut engine = Engine {
        sink: None,
        player: None,
        path: None,
        speed: 1.0,
        token: 0,
        playing: false,
        device_dead: false,
    };
    loop {
        let incoming = match rx.recv_timeout(Duration::from_millis(40)) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(mut command) = incoming {
            loop {
                match rx.try_recv() {
                    Ok(next) => command = next,
                    Err(_) => break,
                }
            }
            match command {
                Command::Apply(next) => desired = Some(next),
                Command::Stop => desired = None,
            }
            engine.reconcile(desired.as_ref(), &shared);
        }
        engine.publish(&shared);
    }
}

pub struct Output {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    token: AtomicU64,
    _thread: JoinHandle<()>,
}

impl Output {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            position_ms: AtomicU64::new(0),
            ended: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("socioscope-audio".into())
            .spawn(move || serve(rx, thread_shared))
            .expect("audio thread");
        Self {
            tx,
            shared,
            token: AtomicU64::new(0),
            _thread: thread,
        }
    }

    pub fn cue(&self, path: &Path, at_ms: u64, speed: f32, playing: bool, seek: bool) {
        if playing {
            self.shared.ended.store(false, Ordering::Relaxed);
            self.shared.failed.store(false, Ordering::Relaxed);
        }
        self.shared.position_ms.store(at_ms, Ordering::Relaxed);
        let token = if seek {
            self.token.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            self.token.load(Ordering::Relaxed)
        };
        let _ = self.tx.send(Command::Apply(Desired {
            path: path.to_path_buf(),
            file_ms: at_ms,
            speed,
            playing,
            token,
        }));
    }

    pub fn stop(&self) {
        self.shared.ready.store(false, Ordering::Relaxed);
        self.shared.ended.store(false, Ordering::Relaxed);
        let _ = self.tx.send(Command::Stop);
    }

    pub fn position_ms(&self) -> u64 {
        self.shared.position_ms.load(Ordering::Relaxed)
    }

    pub fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::Relaxed)
    }

    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }

    pub fn ready(&self) -> bool {
        self.shared.ready.load(Ordering::Relaxed)
    }

    pub fn take_error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|mut slot| slot.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::source::Source;

    #[test]
    fn speed_converts_back_to_the_file() {
        let heard = heard_for(10_000, 2.0);
        assert_eq!(file_ms(heard, 2.0), 10_000);
        assert_eq!(file_ms(Duration::from_secs(5), 2.0), 10_000);
        assert_eq!(file_ms(Duration::from_secs(5), 1.0), 5_000);
    }

    #[test]
    fn sample_audio_decodes_and_seeks() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/data");
        let wav = File::open(root.join("jfk.wav")).unwrap();
        let mut decoder = Decoder::try_from(wav).unwrap();
        let duration = decoder.total_duration().expect("wav duration");
        assert!((duration.as_secs_f64() - 11.0).abs() < 0.05);
        decoder.try_seek(Duration::from_secs(2)).unwrap();
        assert!(decoder.next().is_some());

        let mp3 = File::open(root.join("fdr.mp3")).unwrap();
        let decoder = Decoder::try_from(mp3).unwrap();
        let duration = decoder.total_duration().expect("mp3 duration");
        let seconds = duration.as_secs_f64();
        assert!(
            (1000.0..1500.0).contains(&seconds),
            "fdr.mp3 duration was {seconds}"
        );
    }
}
