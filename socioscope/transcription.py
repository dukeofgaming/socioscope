from datetime import timedelta
import json
import os
import subprocess

import pandas as pd
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

    print(f"Merging diarization and transcription for {file_path}...")
    merge_diarization_transcription(
        os.path.join(output_directory_path, "diarization.json"),
        os.path.join(output_directory_path, "transcription.json"),
        os.path.join(output_directory_path, "diarized_transcription.json")
    )

    print(f"Transcription complete. Output written to {output_directory_path}/diarized_transcription.json")

    print(f"Converting diarized transcription to SRT for {file_path}...")
    convert_diarized_json_to_srt(
        os.path.join(output_directory_path, "diarized_transcription.json"),
        os.path.join(output_directory_path, "diarized_transcription.srt")
    )
    print(f"Conversion complete. Output written to {output_directory_path}/diarized_transcription.srt")


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
        start_milliseconds  = convert_to_milliseconds(turn.start)
        end_milliseconds    = convert_to_milliseconds(turn.end)
        start_timestamp     = convert_to_timestamp(turn.start)
        end_timestamp       = convert_to_timestamp(turn.end)
        
        print(f"start={start_timestamp} stop={end_timestamp} speaker_{speaker}")
        
        diarization_segments.append({
            "timestamps": {
                "from"  : start_timestamp,
                "to"    : end_timestamp
            },
            "offsets": {
                "from"  : start_milliseconds,
                "to"    : end_milliseconds
            },
            "speaker"   : speaker
        })

    with open(os.path.join(output_directory_path,"diarization.json"),"w") as diarization_json_file:
        json.dump(diarization_segments, diarization_json_file, indent=4)

    print("Diarization: writing RTTM")
    # dump the diarization output to disk using RTTM format
    with open(os.path.join(output_directory_path, "diarization.rttm"), "w") as rttm:
        diarization.write_rttm(rttm)

def convert_to_milliseconds(time_in_seconds):
    return int(time_in_seconds * 1000)

# Function to convert seconds to the timestamp format "HH:MM:SS,mmm"
def convert_to_timestamp(time_in_seconds):
    td = timedelta(seconds=time_in_seconds)
    millis = int(td.total_seconds() * 1000) % 1000
    timestamp = str(td)
    # Handling days in timedelta conversion
    if td.days > 0:
        days, time = str(td).split(", ")
        hours, minutes, seconds = time.split(":")
        hours = int(hours) + (24 * int(days.split(" ")[0]))
        timestamp = f"{hours:02}:{minutes}:{seconds}"
    else:
        hours, minutes, seconds = str(td).split(":")
        timestamp = f"{int(hours):02}:{int(minutes):02}:{float(seconds):06.3f}".replace('.', ',')
    return timestamp

def merge_diarization_transcription(diarization_json, transcription_json, output_file):
    with open(diarization_json, 'r') as f:
        diarization_data = json.load(f)
    
    with open(transcription_json, 'r') as f:
        transcription_data = json.load(f)["transcription"]
    
    merged_segments = []

    for t_segment in transcription_data:
        t_start = t_segment['offsets']['from']
        t_end = t_segment['offsets']['to']
        t_text = t_segment['text']
        t_timestamps = t_segment['timestamps']
        
        # Find relevant diarization segments
        relevant_diarizations = [
            {
                "timestamps": {
                    "from": d["timestamps"]["from"],
                    "to": d["timestamps"]["to"]
                },
                "offsets": {
                    "from": d["offsets"]["from"],
                    "to": d["offsets"]["to"]
                },
                "speaker": d["speaker"]
            }
            for d in diarization_data if d["offsets"]["from"] < t_end and d["offsets"]["to"] > t_start
        ]
        
        merged_segments.append({
            "timestamps": t_timestamps,
            "offsets": {
                "from": t_start,
                "to": t_end
            },
            "text": t_text,
            "diarization": relevant_diarizations
        })
    
    # Write the merged data to a new JSON file
    with open(output_file, 'w') as f:
        json.dump(merged_segments, f, indent=4)


def convert_diarized_json_to_srt(input_file, output_file):
    with open(input_file, 'r') as f:
        data = json.load(f)
    
    srt_output = []
    
    for index, segment in enumerate(data):
        start_timestamp  = segment['timestamps']['from']
        end_timestamp    = segment['timestamps']['to']
        text = segment['text']
        
        # Collect unique speakers
        unique_speakers = {d["speaker"] for d in segment["diarization"]}
        speakers = ", ".join(sorted(unique_speakers))  # Sorting to ensure consistent order
        
        srt_output.append(f"{index + 1}")
        srt_output.append(f"{start_timestamp} --> {end_timestamp}")
        srt_output.append(f"[{speakers}] {text}")
        srt_output.append("")  # Blank line to separate SRT entries

    with open(output_file, 'w') as f:
        f.write("\n".join(srt_output))