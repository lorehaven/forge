Feature: Model Selection and Management
  As a user
  I want to select and manage AI models
  So that I can choose the best model for my task

  Background:
    Given sage API is available

  Scenario: List available models
    When I request available models
    Then the response should contain model list

  Scenario: Send message with specific model
    Given I have a conversation
    When I send a message "Test" with model "test-model"
    Then the response status should be 200

  Scenario: Send message with invalid model
    Given I have a conversation
    When I send a message "Test" with model "non-existent-model"
    Then the response status should be 200

  Scenario: Switch models mid-conversation
    Given I have a conversation with model "test-model"
    When I send a message "First" with model "test-model"
    And I send a message "Second" with model "test-model"
    Then both messages should be in conversation history

  Scenario: Model not available triggers launch
    Given default model is not running
    When I send a chat message
    Then sage should request switchboard to launch the model
