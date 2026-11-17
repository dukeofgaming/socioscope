Feature: socioscope

  Scenario: Help is shown when no command is given
    When I run "socioscope"
    Then the exit code is 0
    And stdout contains "usage: socioscope [-h]"
    And stdout contains "socioscope CLI tool"
    And stdout contains "transcribe  Transcribe an m4a audio file"
    And stdout contains "show this help message and exit"
    And stderr is empty

  Scenario: Help is shown for -h
    When I run "socioscope -h"
    Then the exit code is 0
    And stdout contains "usage: socioscope [-h]"
    And stdout contains "show this help message and exit"
    And stderr is empty
