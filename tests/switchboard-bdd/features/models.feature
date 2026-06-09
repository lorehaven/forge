Feature: Models API and UI
  As a user
  I want to browse and search for models
  So that I can find the best model for my needs

  Background:
    Given switchboard API is available
    And I am authenticated

  Scenario: List all models as JSON
    When POST request is sent to "/api/v1/models/list" with body:
      """
      {
        "source": "hf",
        "search": "",
        "sort": "name_asc",
        "quant": "ALL",
        "context": "ALL",
        "vllm_only": false
      }
      """
    Then response status should be 200
    And response should be a JSON array

  Scenario: Filter models by search term
    When POST request is sent to "/api/v1/models/list" with body:
      """
      {
        "source": "hf",
        "search": "llama",
        "sort": "name_asc",
        "quant": "ALL",
        "context": "ALL",
        "vllm_only": false
      }
      """
    Then response status should be 200
    And all models in the response should contain "llama" in their name

  Scenario: Get model estimates modal
    When GET request is sent to "/api/v1/models/estimates-modal?path=non-existent"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'estimates-modal'

  Scenario: Empty estimates modal
    When GET request is sent to "/api/v1/models/estimates-modal/empty"
    Then response status should be 200
    And response should contain 'estimates-modal'
