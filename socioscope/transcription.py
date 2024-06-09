import os
import subprocess

from socioscope import messages

from pyAudioAnalysis import audioSegmentation
import pandas


supported_audio_formats = (".wav", ".m4a", ".mp3")

def transcribe_audio(file_path):
    file_name                   = os.path.basename(file_path)
    file_extension              = os.path.splitext(file_path)[1]
    file_name_without_extension = os.path.splitext(file_name)[0]
    file_directory              = os.path.dirname(file_path)
    output_directory_path       = os.path.join(
        file_directory, 
        file_name_without_extension
    )

    os.makedirs(output_directory_path, exist_ok=True)

    if not file_extension in supported_audio_formats:
        raise ValueError(
            messages.INVALID_AUDIO_TRANSCRIPTION_FORMAT.format(
                file_extensions=', '.join(supported_audio_formats)
            )
        )
    
    if file_extension != ".wav":
        convert_to_wav(file_path, output_directory_path)
    
    print(f"Transcribing {file_path}...")

    run_whisper(file_path, output_directory_path)


def run_whisper(file_path, output_directory_path):
    file_extension              = os.path.splitext(file_path)[1]

    subprocess.run([
        'whisper-cpp',          # Path to the compiled whisper-cpp executable
        '--threads', '16',      # TODO: Parametrize, but default to max available
        # '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-large-v3.bin',     # Model file path
        '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-base.en.bin',     # Model file path
        # '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-medium.en.bin',     # Model file path
        '--file', file_path if file_extension == '.wav' else f"{output_directory_path}/converted.wav",                                        # Input audio file path
        '--output-file', f"{output_directory_path}/transcription",      # Output file path
        # '--output-csv'                                                  # Output format
        '--output-srt'
    ], check=True) 


def convert_to_wav(file_path, output_directory_path):
    output_file_path    = os.path.join(output_directory_path, f"converted.wav")

    print(f"Converting {file_path} to {output_file_path}...")
    
    subprocess.run([
        'ffmpeg', 
        '-i', file_path, 
        '-ar', '16000',     # Audio rate supported by whisper-cpp
        '-y',               
        output_file_path
    ], check=True)