import pytest
from unittest.mock import patch

from socioscope import cli

def test_that_cli_help_message_displays_correctly(capfd):
    with patch('sys.argv', ['cli.__package__', '-h']), pytest.raises(SystemExit):
        cli.main()
    out, err = capfd.readouterr()
    assert f"usage: {cli.__package__} [-h]" in out
    assert "show this help message and exit" in out

# @patch('sys.argv', ['cli', 'transcribe', 'audio.m4a'])
# def test_transcribe_command(mock_run):
#     cli.main()
#     mock_run.assert_called_once_with(['echo', 'Transcribing audio.m4a...'], check=True)