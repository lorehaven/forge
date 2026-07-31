@switchboard
Feature: UI Authentication
  As a user
  I want signing in to be gatehouse's job
  So that one login works across the estate

  Background:
    Given switchboard API is available

  # This service has no login form of its own: gatehouse owns the credentials,
  # the session and the realm cookie.
  Scenario: The login route hands the browser to gatehouse
    When I open the login page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/login"

  Scenario: The return address is carried to gatehouse
    When I open the login page
    Then the redirect location should contain "redirect="
    And the redirect location should contain "switchboard"

  Scenario: Logging out is realm-wide
    When I open the logout page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/logout"

  # What a protected page does without a session is asserted in `ui_jwt.feature`
  # instead, which exercises every way a token can fail to authenticate it.
  # This file once carried warehouse's 302 scenario, copied verbatim; it passed
  # only because the step it borrowed reached warehouse's port instead of
  # switchboard's, and it never exercised this service at all.
