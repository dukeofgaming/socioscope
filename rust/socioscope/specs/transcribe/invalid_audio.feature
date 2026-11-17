Feature: transcribe

  Scenario: A non-audio file is rejected and the process continues
    Given a file named "audio.txt"
    When I transcribe that file
    Then the exit code is 0
    And stderr contains "The file must be an audio file: .wav, .m4a, .mp3"
    And stdout is empty
