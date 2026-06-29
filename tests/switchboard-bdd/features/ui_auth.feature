Feature: UI Authentication
  As a user
  I want to log in to the UI
  So that I can access protected resources

  Background:
    Given switchboard API is available

  Scenario: Failed UI authentication
    When login attempt is made with username "admin" and password "wrong-password"
    Then response status should be 302
    And location header contains "/switchboard/ui"

  Scenario: Successful UI authentication
    When login attempt is made with username "admin" and password "password"
    Then response status should be 302
    And location header contains "/switchboard/ui"
