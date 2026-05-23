Feature: UI Authentication
  As a user
  I want to log in to the UI
  So that I can access protected resources

  Background:
    Given switchboard API is available

  Scenario: Failed UI authentication
    When login attempt is made with username "admin" and password "wrong-password"
    Then response should be a redirect to "/switchboard/ui/login?err=1"

  Scenario: Successful UI authentication
    When login attempt is made with username "admin" and password "password"
    Then response should be a redirect to "/switchboard/ui/home"
    And session cookie should be set
