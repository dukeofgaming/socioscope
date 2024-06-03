import subprocess
from socioscope import messages

supported_audio_formats = (".wav")

def transcribe_audio(file_path):

    if not file_path.lower().endswith(supported_audio_formats):
        raise ValueError(
            messages.INVALID_AUDIO_TRANSCRIPTION_FORMAT.format(
                file_extensions=', '.join(supported_audio_formats)
            )
        )
    
    print(f"Transcribing {file_path}...")

    run_whisper(file_path)

def run_whisper(file_path):
    command = [
        'whisper-cpp',          # Path to the compiled whisper-cpp executable
        '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-large-v3.bin',     # Model file path
        '--file', file_path,     # Input audio file path
        '--output-csv'          # Output file path (if needed)
    ]

    subprocess.run(command, check=True)