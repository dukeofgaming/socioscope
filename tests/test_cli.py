import pytest
from unittest.mock import patch

from socioscope import cli

@patch('subprocess.run')
def test_should_return_hello_world_bang(mock_run):
    cli.main()
    mock_run.assert_called_once_with(['echo', 'Hello World!'], check=True)