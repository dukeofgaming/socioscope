import pytest
from unittest.mock import patch
from socioscope.cli import main

def test_main_no_arguments_exclamation():
    with patch('subprocess.run') as mock_run:
        main()
        # This test is expected to fail because it checks for the exact string "Hello World!" with an exclamation mark
        mock_run.assert_called_once_with(['echo', 'Hello World!'], check=True)