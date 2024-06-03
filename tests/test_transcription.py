import os
import sys
from unittest.mock import patch

from socioscope import cli

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './tests/data/jfk.wav'])
def test_functional_transcribe_wav_file(capfd):
    #Arrange
    wav_file_path = './tests/data/jfk.wav'
    txt_file_path = f"{wav_file_path}.txt"
    if os.path.exists(txt_file_path):
        os.remove(txt_file_path)
    
    assert os.path.exists(wav_file_path)
    
    #Act
    cli.main()

    #Assert
    assert os.path.exists(txt_file_path)