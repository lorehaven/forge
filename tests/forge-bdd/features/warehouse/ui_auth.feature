@warehouse
Feature: UI Authentication
  As a user
  I want signing in to be gatehouse's job
  So that one login works across the estate

  Background:
    Given warehouse API is available

  # This service has no login form of its own: gatehouse owns the credentials,
  # the session and the realm cookie.
  Scenario: The login route hands the browser to gatehouse
    When I open the login page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/login"

  Scenario: The return address is carried to gatehouse
    When I open the login page
    Then the redirect location should contain "redirect="
    And the redirect location should contain "warehouse"

  Scenario: Logging out is realm-wide
    When I open the logout page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/logout"

  Scenario: A protected page without a session goes to the login route
    When a GET request is sent to protected page "/ui/home" without token
    Then response status should be 302
