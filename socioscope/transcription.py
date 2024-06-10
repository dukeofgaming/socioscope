import json
import os
import subprocess

import torch
import yaml
from pyannote.audio import Pipeline

from socioscope import messages

supported_audio_formats = (".wav", ".m4a", ".mp3")

def get_output_directory_path(file_path):
    return os.path.join(
        os.path.dirname(file_path),
        os.path.basename(file_path).split('.')[0]
    )

def get_source_wav_path(file_path):
    file_extension = os.path.splitext(file_path)[1]

    if file_extension.endswith(".wav"):
        return file_path

    return os.path.join(
        get_output_directory_path(file_path),
        "converted.wav"
    )

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
    diarization(file_path)


def run_whisper(file_path, output_directory_path):

    # TODO: Load config from script from the script directory so that there
    #       is a separate config for tests 
    with open('config.yaml', 'r') as config_file:
        config = yaml.safe_load(config_file)

    file_extension              = os.path.splitext(file_path)[1]

    #TODO: Refactor to actually use below
    output_file_path            = os.path.join(output_directory_path, "transcription.srt")

    subprocess.run([
        'whisper-cpp',          # Path to the compiled whisper-cpp executable
        '--threads', '16',      # TODO: Parametrize, but default to max available
        '--model', config['transcription']['model'],   #'/Users/david/Projects/socioscope/tests/.models/ggml-base.en.bin',     # Model file path
        '--file', file_path if file_extension == '.wav' else f"{output_directory_path}/converted.wav",                                        # Input audio file path
        '--output-file', f"{output_directory_path}/transcription",      # Output file path
        # '--output-csv',                                                  # Output format
        '--output-srt',
        '--output-json',
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

def diarization(file_path):

    with open('config.yaml', 'r') as config_file:
        config = yaml.safe_load(config_file)

    output_directory_path = get_output_directory_path(file_path)

    huggingface_token = config['huggingface_token']

    print("Diarization: preparing pipeline")
    pipeline = Pipeline.from_pretrained(
        "pyannote/speaker-diarization-3.1",
        use_auth_token=huggingface_token
    )

    # send pipeline to GPU (metal)
    print("Diarization: sending pipeline to GPU")
    pipeline.to(torch.device("mps"))


    print("Diarization: running pipeline")
    diarization = pipeline(
        get_source_wav_path(file_path)
    )

    diarization_segments = []
    
    for turn, _, speaker in diarization.itertracks(yield_label=True):
        print(f"start={turn.start:.1f}s stop={turn.end:.1f}s speaker_{speaker}")
        diarization_segments.append({
            "start": turn.start,
            "end": turn.end,
            "speaker": speaker
        })

    with open(os.path.join(output_directory_path,"diarization.json"),"w") as diarization_json_file:
        json.dump(diarization_segments, diarization_json_file, indent=4)

    print("Diarization: writing RTTM")
    # dump the diarization output to disk using RTTM format
    with open(os.path.join(output_directory_path, "diarization.rttm"), "w") as rttm:
        diarization.write_rttm(rttm)