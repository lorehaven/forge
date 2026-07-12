Feature: UI Authentication
  As a user
  I want to log in to the UI
  So that I can access protected resources

  Background:
    Given warehouse API is available

  Scenario: Failed UI authentication
    When a login attempt is made with username "admin" and password "wrong-password"
    Then response status should be 302

  Scenario: Successful UI authentication
    When a login attempt is made with username "admin" and password "password"
    Then response status should be 302
    And a session cookie should be set
