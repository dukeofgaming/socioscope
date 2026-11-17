## Requirements

- Rust
- ffmpeg
- A ggml whisper model and the sherpa diarization models in `config.tpl.yaml`

## Running

```bash
cp config.tpl.yaml config.yaml
cargo run --manifest-path rust/socioscope/Cargo.toml --release -- transcribe <file|directory>
```

## Specs

Cucumber specs are grouped by command under `rust/socioscope/specs`.

```bash
cargo test --manifest-path rust/socioscope/Cargo.toml
```
