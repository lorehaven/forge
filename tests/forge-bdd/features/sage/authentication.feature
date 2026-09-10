@sage
Feature: Authentication and Authorization
  As a system
  I want to secure the chat API
  So that only authenticated users can access it

  Background:
    Given sage API is available

  Scenario: Request without authentication token
    When I send a chat message without authentication
    Then the response status should be 401

  Scenario: Request with invalid token
    When I send a chat message with invalid token
    Then the response status should be 401

  Scenario: Request with expired token
    When I send a chat message with expired token
    Then the response status should be 401

  Scenario: Request with valid token
    When I send a chat message with valid token
    Then the response status should be 200

  Scenario: Request with missing scope claim
    When I send a chat message with token missing scope
    Then the response status should be 401

  Scenario: Request with wrong service in token
    When I send a chat message with token for service "other-service"
    Then the response status should be 401

  # The whole `/api/v1/chat` scope sits behind `Auth` + `RequireWrite`
  # (sage-service/src/routers/mod.rs). `Auth` refuses a request with no usable
  # sage identity outright; `RequireWrite` only gates the mutating verbs, so the
  # GET reports here need a valid sage grant but not specifically `write`.
  Scenario: Reading capabilities needs a token
    When GET request is sent to "/api/v1/chat/capabilities"
    Then the response status should be 401

  Scenario: A token whose audience is not sage cannot read the cost report
    When GET "/api/v1/chat/costs" is sent with a token for another service
    Then the response status should be 401

  Scenario: A read grant is enough to read the cost report
    When GET "/api/v1/chat/costs" is sent with a sage token scoped "user sage:read"
    Then the response status should be 200

  Scenario: A read grant is enough to read the context-status report
    When GET "/api/v1/chat/context-status/web_assistant" is sent with a sage token scoped "user sage:read"
    Then the response status should be 200
    And response should contain "profile"
    And response should contain "max_tokens"
