@switchboard
Feature: UI JWT Authentication
  As a developer
  I want tampered or invalid tokens to be rejected
  So that only a genuine realm session can view the interface

  Background:
    Given switchboard API is available

  Scenario: Request to home page without token
    When GET request is sent to protected page "/ui/home" without token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with malformed token
    When GET request is sent to protected page "/ui/home" with malformed token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token signed with wrong secret
    When GET request is sent to protected page "/ui/home" with token signed with wrong secret
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with expired token
    When GET request is sent to protected page "/ui/home" with expired token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token for wrong service
    When GET request is sent to protected page "/ui/home" with token for service "wrong-service"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token with future iat
    When GET request is sent to protected page "/ui/home" with token with future iat
    Then response should be a redirect
    And the redirect location should contain "/ui/login"
