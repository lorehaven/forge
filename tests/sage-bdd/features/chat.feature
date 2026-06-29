Feature: Sage Chat API
  Scenario: Send a chat message
    Given sage API is available
    When I send a chat message "Hello"
    Then I should receive a response
