import os
import pytest
import sys

from unittest.mock import patch

from socioscope import cli, messages, transcription

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

@patch.object(sys, 'argv', new=[cli.__package__, 'transcribe', 'mock_directory'])
@patch.object(transcription, 'transcribe_audio')
@patch('os.listdir')
@patch('os.path.isdir')
@patch('os.path.isfile')
def test_transcribe_directory(
        mock_isfile,
        mock_isdir, 
        mock_listdir, 
        mock_transcribe_audio
    ):
    
    #Arrange
    mock_isdir.return_value = True
    mock_listdir.return_value = [
        'mock_file1.wav', 
        'mock_file2.wav', 
        'mock_file3.wav',
        'somedirectory'
    ]

    mock_isfile.side_effect = lambda file: (
        file.endswith(
            transcription.supported_audio_formats
        )
    )
    
    #Act
    cli.main()

    #Assert
    assert mock_transcribe_audio.call_count == 3