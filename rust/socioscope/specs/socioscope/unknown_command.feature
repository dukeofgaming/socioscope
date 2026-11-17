Feature: socioscope

  Scenario: An unknown command is rejected
    When I run "socioscope nope"
    Then the exit code is 2
    And stderr is the argparse unknown command error for "nope"
    And stdout is empty
