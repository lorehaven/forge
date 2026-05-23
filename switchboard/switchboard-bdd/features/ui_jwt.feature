Feature: UI JWT Authentication
  As a developer
  I want to ensure the UI authentication middleware handles various JWT scenarios
  So that the system remains secure

  Background:
    Given switchboard API is available

  Scenario: Request with missing JWT token
    When GET request is sent to protected page "/ui/home" without token
    Then response should be a redirect to "/switchboard/ui/login"

  Scenario: Request with malformed JWT token
    When GET request is sent to protected page "/ui/home" with malformed token
    Then response should be a redirect to "/switchboard/ui/login"

  Scenario: Request with JWT token signed with wrong secret
    When GET request is sent to protected page "/ui/home" with token signed with wrong secret
    Then response should be a redirect to "/switchboard/ui/login"

  Scenario: Request with expired JWT token
    When GET request is sent to protected page "/ui/home" with expired token
    Then response should be a redirect to "/switchboard/ui/login"

  Scenario: Request with JWT token for mismatched service
    When GET request is sent to protected page "/ui/home" with token for service "wrong-service"
    Then response should be a redirect to "/switchboard/ui/login"
