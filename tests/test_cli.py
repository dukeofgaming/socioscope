import pytest
import sys

from unittest.mock import patch


from socioscope import cli

@patch.object(sys, 'argv', new=[cli.__package__, '-h'])
def test_that_cli_help_message_displays_correctly(capfd):
    
    with pytest.raises(SystemExit):
        cli.main()
    
    out, err = capfd.readouterr()
    assert f"usage: {cli.__package__} [-h]" in out
    assert "show this help message and exit" in out

@patch.object(sys, 'argv', new=['cli', 'transcribe', 'audio.m4a'])
def test_transcribe_m4a_file(capfd):
    cli.main()
    out, err = capfd.readouterr()
    assert "Transcribing audio.m4a..." in out

@patch.object(sys, 'argv', new=['cli', 'transcribe', 'audio.mp3'])
def test_transcribe_fail_on_non_m4a_file(capfd):
    with pytest.raises(SystemExit):
        cli.main()
    out, err = capfd.readouterr()
    assert "Error: The file must be an m4a audio file." in out

