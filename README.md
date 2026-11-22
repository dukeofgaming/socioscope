## Requirements

- Python 3.10.14



## Caption studio

The desktop app is a GPUI caption studio: a monitor for the active cue, a timeline of the SRT, and a cue sheet with reading-speed marks.

```bash
cd desktop
cargo run --release
```

It opens the sample library (Roosevelt's first inaugural, the "ask not" excerpt, and a three-speaker sample). Import any `.srt` from the sidebar. Space plays, and the left and right keys move between cues.

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

