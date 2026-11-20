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

`socioscope models` lists, downloads, and selects weights. The shelf is `rust/socioscope/models/catalog.yaml`. It is sized for an M3 Max with about 24 GB free. QVAC is not the downloader. The CLI runs each model with the engine that can load it:

- Whisper stays on whisper.cpp, and pyannote plus 3D-Speaker stays on sherpa-onnx. Both are linked in.
- Whistle runs on the Cactus runner, downloaded next to the weights on first use.
- Parakeet TDT 0.6B v3 and Nemotron 3 Diarization run on audio.cpp. The shelf stores the GGUF packages that engine loads, which are the audio.cpp conversions of those checkpoints. The first transcription downloads the audio.cpp build for this machine: Metal on a Mac, CPU on Linux. The Linux build of 0.9.5 wants glibc 2.43, so Ubuntu 24.04 cannot run that prebuilt.
- Voz and Who run through the Desert Ant Node packages. Voz uses the ONNX graphs, so the same command works on Linux and on a Mac. Who uses the native package: Core ML on a Mac, LiteRT on Linux. Node and npm are required for those two. Who's package is source-available.

`use` writes the engine name into the config beside the weight path, and the next transcribe follows it.

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
