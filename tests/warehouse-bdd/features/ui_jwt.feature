Feature: UI JWT Authentication
  As a developer
  I want to ensure the UI authentication middleware handles various JWT scenarios
  So that the system remains secure

  Background:
    Given warehouse API is available

  Scenario: Request with missing JWT token
    When a GET request is sent to protected page "/ui/home" without token
    Then the response should be a redirect to "/ui/login"

  Scenario: Request with malformed JWT token
    When a GET request is sent to protected page "/ui/home" with malformed token
    Then the response should be a redirect to "/ui/login"

  Scenario: Request with JWT token signed with wrong secret
    When a GET request is sent to protected page "/ui/home" with token signed with wrong secret
    Then the response should be a redirect to "/ui/login"

  Scenario: Request with expired JWT token
    When a GET request is sent to protected page "/ui/home" with expired token
    Then the response should be a redirect to "/ui/login"

  Scenario: Request with JWT token for mismatched service
    When a GET request is sent to protected page "/ui/home" with token for service "wrong-service"
    Then the response should be a redirect to "/ui/login"

  Scenario: Request with JWT token with future iat
    When a GET request is sent to protected page "/ui/home" with token with future iat
    Then the response should be a redirect to "/ui/login"
