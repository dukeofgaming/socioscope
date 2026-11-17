Feature: transcribe

  Scenario: Help is shown for the transcribe command
    When I run "socioscope transcribe -h"
    Then the exit code is 0
    And stdout contains "usage: socioscope transcribe [-h] FILE"
    And stdout contains "The path to the m4a file to transcribe"
    And stderr is empty
