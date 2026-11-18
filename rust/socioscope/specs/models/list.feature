Feature: model shelf

  Scenario: the shelf lists models the app can download
    When I run "socioscope models list"
    Then the exit code is 0
    And stdout contains "Nemotron"
    And stdout contains "Whistle"
    And stdout contains "large-v3-turbo"
