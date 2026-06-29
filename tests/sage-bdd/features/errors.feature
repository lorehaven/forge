Feature: Error Handling and Validation
  As a user
  I want clear error messages
  So that I can fix my requests

  Background:
    Given sage API is available

  Scenario: Empty message accepted
    Given I have a conversation
    When I send a chat message "" with valid token
    Then the response status should be 200

  Scenario: Missing required field
    Given I have a conversation
    When I send chat request without message field
    Then the response status should be 400

  Scenario: Message too long accepted
    Given I have a conversation
    When I send a very long chat message (10000+ characters)
    Then the response status should be 200

  Scenario: Invalid JSON format
    When I send malformed JSON to chat endpoint
    Then the response status should be 400

  Scenario: vLLM service gracefully handled
    Given vLLM service is unavailable
    When I send a chat message
    Then the response status should be 200

  Scenario: Concurrent requests
    Given I have a conversation
    When I send 5 concurrent chat messages
    Then all requests should complete successfully
