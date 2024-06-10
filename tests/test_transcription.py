import os
import sys
from unittest.mock import patch

from socioscope import cli

@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/jfk.wav'])
def test_functional_transcribe_wav_file():
    #Arrange
    audio_file_path           = sys.argv[2]
    transcription_file_path = f"./tests/data/jfk/transcription.srt"

    if os.path.exists(transcription_file_path):
        os.remove(transcription_file_path)
    
    assert os.path.exists(audio_file_path)
    
    #Act
    cli.main()

    #Assert
    assert os.path.exists(transcription_file_path)


@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/fdr.mp3'])
def test_functional_transcribe_mp3_file():
    #Arrange
    wav_file_path           = "./tests/data/fdr/converted.wav"
    transcription_csv_path  = "./tests/data/fdr/transcription.srt"

    if os.path.exists(wav_file_path):
        os.remove(wav_file_path)

    if os.path.exists(transcription_csv_path):
        os.remove(transcription_csv_path)

    #Act
    cli.main()

    #Assert
    assert os.path.exists(wav_file_path)
    assert os.path.exists(transcription_csv_path)

@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/jfk-rmn.mp3'])
def test_functional_diarization(capfd):
    #Arrange & act
    cli.main()

    output_directory_path   = "./tests/data/jfk-rmn"
    wav_file_path           = os.path.join(output_directory_path, "converted.wav")
    diarization_file_path   = os.path.join(output_directory_path, "diarization.rttm")

    assert os.path.exists(wav_file_path)

    #Assert
    assert os.path.exists(diarization_file_path)

    # assert that the diarization file is not empty
    with open(diarization_file_path) as diarization_file:
        assert diarization_file.read().strip() != ""