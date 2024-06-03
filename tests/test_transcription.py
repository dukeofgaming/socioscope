import os
import sys
from unittest.mock import patch

from socioscope import cli

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/jfk.wav'])
def test_functional_transcribe_wav_file(capfd):
    #Arrange
    wav_file_path           = './tests/data/jfk.wav'
    transcription_file_path = f"{wav_file_path}.csv"

    if os.path.exists(transcription_file_path):
        os.remove(transcription_file_path)
    
    assert os.path.exists(wav_file_path)
    
    #Act
    cli.main()

    #Assert
    assert os.path.exists(transcription_file_path)


@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/recordings/N Ninth St 3.m4a'])
def test_functional_transcribe_m4a_file(capfd):
    #Arrange
    m4a_file_path           = './tests/data/recordings/N Ninth St 3.m4a'
    wav_file_path           = f"{os.path.splitext(m4a_file_path)[0]}.wav"
    transcription_csv_path  = f"{wav_file_path}.csv"

    if os.path.exists(wav_file_path):
        os.remove(wav_file_path)

    if os.path.exists(transcription_csv_path):
        os.remove(transcription_csv_path)

    #Act
    cli.main()
    assert os.path.exists(wav_file_path)

    #Assert
    assert os.path.exists(transcription_csv_path)