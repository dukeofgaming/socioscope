import os
import subprocess

import torch
import yaml
from pyannote.audio import Pipeline

from socioscope import messages

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
    
    print(f"Diarizing {file_path}...")
    diarization(output_directory_path)


def run_whisper(file_path, output_directory_path):
    file_extension              = os.path.splitext(file_path)[1]
    output_file_path            = os.path.join(output_directory_path, "transcription.srt")

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

    print(f"Transcription complete. Output written to {output_directory_path}/transcription.srt")


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

    print(f"Conversion complete. Output written to {output_file_path}")

def diarization(output_directory_path):

    with open('config.yaml', 'r') as config_file:
        config = yaml.safe_load(config_file)

    huggingface_token = config['huggingface_token']

    print("Diarizing: preparing pipeline")
    pipeline = Pipeline.from_pretrained(
        "pyannote/speaker-diarization-3.1",
        use_auth_token=huggingface_token
    )

    # send pipeline to GPU (metal)
    print("Diarizing: sending pipeline to GPU")
    pipeline.to(torch.device("mps"))

    print("Diarizing: running pipeline")
    diarization = pipeline(
        os.path.join(output_directory_path, "converted.wav")
    )
    # print the result
    for turn, _, speaker in diarization.itertracks(yield_label=True):
        print(f"start={turn.start:.1f}s stop={turn.end:.1f}s speaker_{speaker}")

    print("Diarizing: writing RTTM")
    # dump the diarization output to disk using RTTM format
    with open(os.path.join(output_directory_path, "diarization.rttm"), "w") as rttm:
        diarization.write_rttm(rttm)