@gatehouse
Feature: The authorization-code + PKCE redirect flow
  As a relying party
  I want gatehouse to hand an unauthenticated browser to its own login page
  So that logging in can send it straight back to finish the request it came for

  Background:
    Given gatehouse API is available

  Scenario: No session sends the browser to login with a redirect back to /authorize itself
    When I request authorization for client "conveyor" without a session
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/login"
    # Regression: this used to carry a bare "/api/v1/authorize", missing the
    # /gatehouse base path - a browser that logged in landed on a 404 instead
    # of back at /authorize.
    And the redirect location should contain "redirect=%2Fgatehouse%2Fapi%2Fv1%2Fauthorize"
