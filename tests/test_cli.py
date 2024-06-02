import pytest
import sys

from unittest.mock import patch


from socioscope import cli

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, '-h'])
def test_that_cli_help_message_displays_correctly(capfd):

    #Act
    with pytest.raises(SystemExit):
        cli.main()
    
    out, err = capfd.readouterr()

    #Assert
    assert f"usage: {cli.__package__} [-h]" in out
    assert "show this help message and exit" in out

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', 'audio.m4a'])
def test_transcribe_m4a_file(capfd):
    cli.main()
    out, err = capfd.readouterr()
    assert "Transcribing audio.m4a..." in out

#Arrange
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', 'audio.txt'])
def test_error_transcribe_audio_format(capfd):
    #Act
    cli.main()
    out, err = capfd.readouterr()

    #Assert
    assert "The file must be an audio file." in err
    