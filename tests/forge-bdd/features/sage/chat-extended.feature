@sage
Feature: Extended Chat Functionality
  As a user
  I want comprehensive chat features
  So that I can have rich conversations

  Background:
    Given sage API is available

  Scenario: Multi-turn conversation
    Given I have a conversation
    When I send a chat message "What is machine learning?"
    And I send a chat message "Explain neural networks"
    And I send a chat message "How do transformers work?"
    Then all three messages should be in conversation history
    And each message should have a response

  Scenario: Message with special characters
    Given I have a conversation
    When I send a chat message "Hello! @#$%^&*() 你好 🚀"
    Then the response status should be 200

  Scenario: Long message
    Given I have a conversation
    When I send a long chat message with 5000 characters
    Then the response status should be 200

  Scenario: Rapid fire messages
    Given I have a conversation
    When I send 5 rapid chat messages
    Then all requests should complete successfully

  Scenario: Message response contains required fields
    Given I have a conversation
    When I send a chat message "Test"
    Then the response should contain:
      | message_id |
      | content |
      | role |
      | timestamp |

  Scenario: Conversation metadata
    Given I have a conversation
    When I send a chat message
    Then the conversation should have:
      | created_at |
      | updated_at |
      | message_count |

  Scenario: Token usage tracking
    Given I have a conversation
    When I send a chat message
    Then the response should include token usage:
      | prompt_tokens |
      | completion_tokens |
      | total_tokens |
