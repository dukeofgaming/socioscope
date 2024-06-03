import os
import subprocess
from socioscope import messages

supported_audio_formats = (".wav", ".m4a")

def transcribe_audio(file_path):

    #Get file extension from path
    file_extension = os.path.splitext(file_path)[1]

    if not file_extension in supported_audio_formats:
        raise ValueError(
            messages.INVALID_AUDIO_TRANSCRIPTION_FORMAT.format(
                file_extensions=', '.join(supported_audio_formats)
            )
        )
    
    if file_extension != ".wav":
        convert_to_wav(file_path)
        file_path = os.path.splitext(file_path)[0] + ".wav" # Update file path to the converted file
    
    print(f"Transcribing {file_path}...")

    run_whisper(file_path)


def run_whisper(file_path):
    subprocess.run([
        'whisper-cpp',          # Path to the compiled whisper-cpp executable
        # '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-large-v3.bin',     # Model file path
        '--model', '/Users/david/Projects/socioscope/tests/.models/ggml-base.en.bin',     # Model file path
        '--file', file_path,     # Input audio file path
        '--output-csv'          # Output file path (if needed)
    ], check=True)


def convert_to_wav(file_path):
    base                = os.path.basename(file_path)   #TODO: See if this is necessary
    directory           = os.path.dirname(file_path)

    name_without_ext    = os.path.splitext(base)[0]
    output_file_path    = os.path.join(directory, f"{name_without_ext}.wav")

    print(f"Converting {file_path} to {output_file_path}...")
    
    subprocess.run([
        'ffmpeg', 
        '-i', file_path, 
        '-ar', '16000',     # Audio rate supported by whisper-cpp
        '-y',               #TODO: Optimize # Overwrite output file if it exists
        output_file_path
    ], check=True)