@sage
Feature: UI Authentication
  As a user
  I want signing in to be gatehouse's job
  So that one login works across the estate

  Background:
    Given sage API is available

  # This service has no login form of its own: gatehouse owns the credentials,
  # the session and the realm cookie. The redirect starts the authorization-code
  # + PKCE exchange (gatehouse/api/v1/authorize), not a bare hop to the login
  # form - gatehouse only shows a form if there is no gatehouse session yet.
  Scenario: The login route hands the browser to gatehouse
    When I open the login page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/api/v1/authorize"
    And the redirect location should contain "client_id=sage"

  Scenario: The return address is carried to gatehouse
    When I open the login page
    Then the redirect location should contain "redirect_uri="
    And the redirect location should contain "sage"

  Scenario: Logging out is realm-wide
    When I open the logout page
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/logout"

  Scenario: The auth status endpoint is honest about being anonymous
    When GET request is sent to "/ui/status"
    Then response status should be 200
    And response should contain "authenticated"
    And response should contain "false"

  # Not a 404: an anonymous visitor is sent to sign in before sage says anything
  # about the page behind it.
  Scenario: The home page needs a session
    When a GET request is sent to protected page "/ui/home" without token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"
