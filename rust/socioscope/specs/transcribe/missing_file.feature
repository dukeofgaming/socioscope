Feature: transcribe

  Scenario: A file argument is required
    When I run "socioscope transcribe"
    Then the exit code is 2
    And stderr is the argparse missing FILE error
    And stdout is empty
