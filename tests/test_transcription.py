import os
import shutil
import sys

from unittest.mock import patch

from socioscope import cli, transcription

def remove_output_dir_if_exists(file_path):
    output_directory_path = transcription.get_output_directory_path(file_path)

    if os.path.exists(output_directory_path):
        shutil.rmtree(output_directory_path)

def assert_source_wav_exists(file_path):
    assert os.path.exists(
        transcription.get_source_wav_path(file_path)
    )

def assert_transcription_files_exist(file_path):
    transcription_srt_file_path = os.path.join(
        transcription.get_output_directory_path(file_path),
        "transcription.srt"
    )

    transcription_json_file_path = os.path.join(
        transcription.get_output_directory_path(file_path),
        "transcription.json"
    )

    assert os.path.exists(transcription_srt_file_path)
    assert os.path.exists(transcription_json_file_path)

def assert_diarization_files_exist(file_path):
    output_directory_path   = transcription.get_output_directory_path(file_path)
    
    diarization_rttm_file_path   = os.path.join(
        output_directory_path,
        "diarization.rttm"
    )

    diarization_json_file_path   = os.path.join(
        output_directory_path,
        "diarization.json"
    )

    assert os.path.exists(diarization_rttm_file_path)
    assert os.path.exists(diarization_json_file_path)

def assert_output_directory_contents_exist(file_path):
    assert_source_wav_exists(file_path)
    assert_transcription_files_exist(file_path)
    assert_diarization_files_exist(file_path)

# Functional test to do transcription on a wav file
# TODO: Decoulpe from CLI implementation or go BDD?
@patch.object(sys, 'argv', new = [cli.__package__, 
    'transcribe', 
    './tests/data/jfk.wav'
])
def test_functional_transcribe_wav_file():
    #Arrange
    original_audio_file_path    = sys.argv[2]

    remove_output_dir_if_exists(original_audio_file_path)
    assert_source_wav_exists(original_audio_file_path)
    
    #Act
    cli.main()

    #Assert
    assert_output_directory_contents_exist(original_audio_file_path)

# Functional test to do transcription on an mp3 file, with intermediate wav file
@patch.object(sys, 'argv', new = [ cli.__package__, 
    'transcribe', 
    './tests/data/fdr.mp3'
])
def test_functional_transcribe_mp3_file():
    #Arrange
    original_audio_file_path    = sys.argv[2]
    remove_output_dir_if_exists(original_audio_file_path)
    assert original_audio_file_path.endswith(".mp3")

    #Act
    cli.main()

    #Assert
    assert_source_wav_exists(original_audio_file_path)
    assert_output_directory_contents_exist(original_audio_file_path)

@patch.object(sys, 'argv', new = [cli.__package__,
    'transcribe', 
    './tests/data/jfk-rmn.mp3'
])
def test_functional_diarization():
    #Arrange & act
    original_audio_file_path    = sys.argv[2]
    remove_output_dir_if_exists(original_audio_file_path)

    cli.main()

    output_directory_path   = transcription.get_output_directory_path(original_audio_file_path)
    diarization_file_path   = os.path.join(output_directory_path, "diarization.rttm")

    assert_source_wav_exists(original_audio_file_path)

    #Assert
    assert_diarization_files_exist(original_audio_file_path)

    # assert that the diarization file is not empty
    with open(diarization_file_path) as diarization_file:
        contents = diarization_file.read().strip()
        speakers = [f"SPEAKER_{i:02d}" for i in range(4)]  # Generate speaker names

        assert contents != ""
        for speaker in speakers:
            assert speaker in contents