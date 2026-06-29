Feature: Conversation Management
  As a user
  I want to manage conversations
  So that I can have multiple chat sessions

  Background:
    Given sage API is available

  Scenario: Create a new conversation
    When I create a new conversation
    Then the response should contain a conversation_id

  Scenario: Send message to conversation
    Given I have a conversation
    When I send a chat message "What is AI?" to the conversation
    Then the response status should be 200
    And the response should contain assistant message

  Scenario: Retrieve conversation history
    Given I have a conversation with messages
    When I retrieve the conversation
    Then the response should contain all messages

  Scenario: List all conversations
    Given I have multiple conversations
    When I list all conversations
    Then the response should contain all conversation ids

  Scenario: Send message without conversation_id
    When I send a chat message "Hello" without conversation_id
    Then the response status should be 200

  Scenario: Send message to non-existent conversation
    When I send a chat message "Test" to conversation "invalid-id"
    Then the response status should be 200
