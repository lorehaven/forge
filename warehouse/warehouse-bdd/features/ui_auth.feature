Feature: UI Authentication
  As a user
  I want to log in to the UI
  So that I can access protected resources

  Background:
    Given warehouse API is available

  Scenario: Failed UI authentication
    When a login attempt is made with username "admin" and password "wrong-password"
    Then the response should be a redirect to "/ui/login?err=1"

  Scenario: Successful UI authentication
    When a login attempt is made with username "admin" and password "password"
    Then the response should be a redirect to "/ui/home"
    And a session cookie should be set
