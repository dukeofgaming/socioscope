import subprocess
from socioscope import messages

supported_audio_formats = (".mp3", ".wav", ".flac", ".aac", ".m4a", ".ogg")

def transcribe_audio(file_path):

    if not file_path.lower().endswith(supported_audio_formats):
        raise ValueError(
            messages.INVALID_AUDIO_TRANSCRIPTION_FORMAT.format(
                file_extensions=', '.join(supported_audio_formats)
            )
        )
    
    print(f"Transcribing {file_path}...")

    command = [
        'whisper-cpp',          # Path to the compiled whisper-cpp executable
        '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-base.en.bin',     # Model file path
        '--file', file_path,     # Input audio file path
        '--output-txt'     # Output file path (if needed)
    ]

    subprocess.run(command, check=True)