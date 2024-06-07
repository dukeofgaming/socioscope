import os
import sys
from unittest.mock import patch

from socioscope import cli

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/jfk.wav'])
def test_functional_transcribe_wav_file():
    #Arrange
    audio_file_path           = sys.argv[2]
    transcription_file_path = f"./tests/data/jfk/transcription.csv"

    if os.path.exists(transcription_file_path):
        os.remove(transcription_file_path)
    
    assert os.path.exists(audio_file_path)
    
    #Act
    cli.main()

    #Assert
    assert os.path.exists(transcription_file_path)


@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/fdr.mp3'])
def test_functional_transcribe_m4a_file():
    #Arrange
    wav_file_path           = "./tests/data/fdr/converted.wav"
    transcription_csv_path  = "./tests/data/fdr/transcription.csv"

    if os.path.exists(wav_file_path):
        os.remove(wav_file_path)

    if os.path.exists(transcription_csv_path):
        os.remove(transcription_csv_path)

    #Act
    cli.main()

    #Assert
    assert os.path.exists(wav_file_path)
    assert os.path.exists(transcription_csv_path)