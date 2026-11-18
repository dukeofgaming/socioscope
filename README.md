## Requirements

- Rust
- ffmpeg
- A ggml whisper model and the sherpa diarization models in `config.tpl.yaml`

## Running

```bash
cp config.tpl.yaml config.yaml
cargo run --manifest-path rust/socioscope/Cargo.toml --release -- transcribe <file|directory>
```

## Web UI

`socioscope-web` is a separate package. It serves a responsive app with axum, a vendored copy of HTMX 2.0.4, and Tailwind.

```bash
cargo run --manifest-path rust/socioscope-web/Cargo.toml --release
```

The page listens on `127.0.0.1:3210`. Set `SOCIOSCOPE_WEB_ADDR` to change it, and `SOCIOSCOPE_DATA` to change where packages are written (the default is `./socioscope-data`).

Record uses an emulated microphone: the opening of the first Kennedy–Nixon debate (26 September 1960, [archive.org](https://archive.org/details/1960_kennedy-nixon_1)), played at 1×. If `tests/data/jfk-rmn.mp3` is present, the record screen also offers the full 16 minute tape. The samples that arrive are stored as the original recording. Cues from the prepared opening alignment appear as the clock advances.

Open stores the original file in the package and runs the same transcribe path as the CLI.

Each session is an [OKF](https://github.com/GoogleCloudPlatform/open-knowledge-format) v0.2 bundle: `index.md`, `log.md`, a session note, the recording note, the diarized transcript, speaker notes, and `references/` for the audio and SRT. The app lists those packages and opens each note.

## Models

`socioscope models` lists, downloads, and selects weights. The shelf is `rust/socioscope/models/catalog.yaml`. It is sized for an M3 Max with about 24 GB free. QVAC is not the downloader: it can run Whisper, Parakeet, and Nemotron, and it cannot run Desert Ant Voz, Desert Ant Who, or Cactus Whistle.

This build can switch to the Whisper ggml files and the pyannote plus 3D-Speaker pair. The other entries download and stay on disk. `use` refuses them so the next transcribe still points at a model the engine can load.

```bash
socioscope models list
socioscope models download whisper-tiny-en
socioscope models use whisper-tiny-en
```

Downloads go to `SOCIOSCOPE_MODELS` or `./socioscope-models`. `use` rewrites `SOCIOSCOPE_CONFIG` or `./config.yaml` and keeps the Hugging Face token. The web shelf calls this CLI. It looks for `SOCIOSCOPE_BIN`, then a `socioscope` binary next to the server, then `PATH`.

## Specs

Cucumber specs are grouped by command under `rust/socioscope/specs`.

```bash
cargo test --manifest-path rust/Cargo.toml -p socioscope --lib
cargo test --manifest-path rust/Cargo.toml -p socioscope --test specs
cargo test --manifest-path rust/Cargo.toml -p socioscope-web
```
