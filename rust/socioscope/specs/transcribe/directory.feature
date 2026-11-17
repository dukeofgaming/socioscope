Feature: transcribe

  Scenario: A directory transcribes files and skips subdirectories
    Given a directory containing "mock_file1.wav", "mock_file2.wav", and "mock_file3.wav"
    And a subdirectory named "somedirectory"
    When I transcribe that directory
    Then the exit code is 0
    And 3 files are transcribed
    And stderr is empty
