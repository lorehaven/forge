@sage
Feature: UI JWT Authentication
  As a developer
  I want tampered or invalid tokens to be rejected
  So that only a genuine realm session can view the interface

  Background:
    Given sage API is available

  # The steps are shared with warehouse and switchboard (src/steps/warehouse/
  # ui_jwt.rs); they hit whichever service the background selected.
  Scenario: Request to home page without token
    When a GET request is sent to protected page "/ui/home" without token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with malformed token
    When a GET request is sent to protected page "/ui/home" with malformed token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token signed with wrong secret
    When a GET request is sent to protected page "/ui/home" with token signed with wrong secret
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with expired token
    When a GET request is sent to protected page "/ui/home" with expired token
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token for wrong service
    When a GET request is sent to protected page "/ui/home" with token for service "wrong-service"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Request to home page with token with future iat
    When a GET request is sent to protected page "/ui/home" with token with future iat
    Then response should be a redirect
    And the redirect location should contain "/ui/login"
