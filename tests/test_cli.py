import pytest
import sys

from unittest.mock import patch

from socioscope import cli, messages

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
@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', 'audio.txt'])
def test_error_transcribe_audio_format(capfd):
    #Act
    cli.main()
    out, error = capfd.readouterr()

    #Assert
    assert messages.INVALID_AUDIO_TRANSCRIPTION_FORMAT.format(
        file_extensions=''
    ) in error
