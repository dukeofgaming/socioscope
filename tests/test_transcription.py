import sys
from unittest.mock import patch

from socioscope import cli

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', './data/jfk.wav'])
def test_functional_transcribe_wav_file(capfd):
    #Act
    cli.main()
    out, err = capfd.readouterr()

    #Assert
    assert "Transcribing audio.m4a..." in out