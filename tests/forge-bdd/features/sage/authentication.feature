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
