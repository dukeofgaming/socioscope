## Requirements

- Python 3.10.14



## Caption studio

The desktop app is a GPUI caption studio: a monitor for the active cue, a timeline of the SRT, and a cue sheet with reading-speed marks.

```bash
cd desktop
cargo run --release
```

It opens the sample library (Roosevelt's first inaugural, the "ask not" excerpt, and a three-speaker sample). Import any `.srt` from the sidebar. Space plays, and the left and right keys move between cues. The window reflows for a tablet, a phone, and a watch-sized frame.

Each session is an [Open Knowledge Format](https://github.com/open-knowledge-format/open-knowledge-format) 0.3 bundle. The Package tab lists the concepts, assets, and log in that bundle.

Models live in `config.yaml` (copy `config.tpl.yaml`). The Config page edits the transcription model path, the diarization model id, and the Hugging Face token. Transcribe locally reads the same file.

Two plugins sit in the right pane, and each one can be turned off from Config. Fallacies asks Laya about the open session. DSM-5 asks Laya for evidence themes and keeps a profile for each speaker label across the library. A local Bonsai server can correlate a profile. The panes say what they are: draft evidence, not a diagnosis. Point `engines.laya.path` at a Laya GGUF and, if you want correlation, `engines.bonsai.path` plus `engines.bonsai.endpoint`.

## Command line

1. Activate the virtual environment:

```bash
python3 -m venv venv
source venv/bin/activate
```

```bash
python -m socioscope transcribe <file|directory>
```

When a recording in the studio has an audio file, Transcribe locally runs this same command. If it writes `diarized_transcription.srt`, the studio loads that file.

