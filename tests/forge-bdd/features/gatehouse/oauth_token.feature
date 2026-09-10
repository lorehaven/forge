@gatehouse
Feature: The OAuth token endpoint
  As a relying party or a machine client
  I want /api/v1/token to honour the grants it supports and refuse the rest
  So that a service identity is issued only against real client credentials

  # The authorization_code grant needs a browser round trip through /authorize
  # (oauth.feature covers the no-session redirect). What is exercised here is
  # every grant that needs no browser.

  Background:
    Given gatehouse API is available

  Scenario: A machine client exchanges its secret for a service token
    When I request a client-credentials token for "sage-switchboard" with secret "bdd-client-secret-sage-switchboard"
    Then response status should be 200
    And response should contain "access_token"

  Scenario: A wrong client secret is refused
    When I request a client-credentials token for "sage-switchboard" with secret "not-the-secret"
    Then response status should be 400

  Scenario: An unknown client is refused
    When I request a client-credentials token for "no-such-client" with secret "whatever"
    Then response status should be 400

  Scenario: A garbage refresh token is refused
    When I exchange the refresh token "not-a-real-refresh-token" at the token endpoint
    Then response status should be 401

  Scenario: An unsupported grant type is refused
    When I request a token with grant type "password"
    Then response status should be 400
