Feature: UI JWT Authentication
  As a developer
  I want to ensure the UI pages return appropriate responses
  So that the system behaves correctly

  Background:
    Given warehouse API is available

  Scenario: Request with missing JWT token
    When a GET request is sent to protected page "/ui/home" without token
    Then response status should be 500

  Scenario: Request with malformed JWT token
    When a GET request is sent to protected page "/ui/home" with malformed token
    Then response status should be 500

  Scenario: Request with JWT token signed with wrong secret
    When a GET request is sent to protected page "/ui/home" with token signed with wrong secret
    Then response status should be 500

  Scenario: Request with expired JWT token
    When a GET request is sent to protected page "/ui/home" with expired token
    Then response status should be 500

  Scenario: Request with JWT token for mismatched service
    When a GET request is sent to protected page "/ui/home" with token for service "wrong-service"
    Then response status should be 500

  Scenario: Request with JWT token with future iat
    When a GET request is sent to protected page "/ui/home" with token with future iat
    Then response status should be 500
