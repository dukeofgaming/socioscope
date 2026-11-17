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

`socioscope-web` is a separate package. It serves a static page with axum and a vendored copy of HTMX 2.0.4, and calls the same transcribe command on one background thread so the models stay loaded.

```bash
cargo run --manifest-path rust/socioscope-web/Cargo.toml --release
```

The page listens on `127.0.0.1:3210`. Set `SOCIOSCOPE_WEB_ADDR` to change it.

## Specs

Cucumber specs are grouped by command under `rust/socioscope/specs`.

```bash
cargo test --manifest-path rust/Cargo.toml -p socioscope --lib
cargo test --manifest-path rust/Cargo.toml -p socioscope --test specs
cargo test --manifest-path rust/Cargo.toml -p socioscope-web
```
