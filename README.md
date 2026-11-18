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

## Specs

Cucumber specs are grouped by command under `rust/socioscope/specs`.

```bash
cargo test --manifest-path rust/Cargo.toml -p socioscope --lib
cargo test --manifest-path rust/Cargo.toml -p socioscope --test specs
cargo test --manifest-path rust/Cargo.toml -p socioscope-web
```
