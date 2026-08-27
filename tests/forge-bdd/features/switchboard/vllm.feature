@switchboard
Feature: vLLM Management API
  As a user
  I want to manage vLLM instances
  So that I can run models

  Background:
    Given switchboard API is available
    And I am authenticated

  Scenario: List vLLM instances
    When GET request is sent to "/api/v1/vllm/list"
    Then response status should be 200
    And response should be a JSON array

  Scenario: Get vLLM instances grid
    When GET request is sent to "/api/v1/vllm/grid"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'vllm-instances-grid'

  Scenario: Get launch instance modal
    When GET request is sent to "/api/v1/vllm/launch-modal?model=some-model"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'launch-instance-modal'

  Scenario: Get empty launch instance modal
    When GET request is sent to "/api/v1/vllm/launch-modal/empty"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'launch-instance-modal'

  Scenario: Get stop instance modal
    When GET request is sent to "/api/v1/vllm/stop-modal?id=some-instance-id"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'stop-instance-modal'

  Scenario: Get empty stop instance modal
    When GET request is sent to "/api/v1/vllm/stop-modal/empty"
    Then response status should be 200
    And response content type should be "text/html"
    And response should contain 'stop-instance-modal'

  Scenario: Get vLLM status SSE
    When GET request is sent to "/api/v1/vllm/sse"
    Then response status should be 200
    And response content type should be "text/event-stream"

  Scenario: Launch and stop a vLLM instance
    When POST request is sent to "/api/v1/vllm/instances" with body:
      """
      {
        "model": "test-model",
        "host": "0.0.0.0",
        "port": 8001,
        "enable_prefix_caching": false
      }
      """
    Then response status should be 202
    And response should be a JSON object
    And response should contain "test-model"
    And response should contain "mock-"
    When DELETE request is sent to "/api/v1/vllm/instances/{last_id}"
    Then response status should be 200

  Scenario: Launch a vLLM instance on CPU
    When POST request is sent to "/api/v1/vllm/instances" with body:
      """
      {
        "model": "cpu-model",
        "host": "0.0.0.0",
        "port": 8002,
        "enable_prefix_caching": false,
        "device": "cpu"
      }
      """
    Then response status should be 202
    And response should be a JSON object
    And response should contain "cpu-model"
    And response should contain "cpu"
    When DELETE request is sent to "/api/v1/vllm/instances/{last_id}"
    Then response status should be 200

  Scenario: Stop non-existent instance
    When DELETE request is sent to "/api/v1/vllm/instances/non-existent-id"
    Then response status should be 404

  Scenario: Launch instance with invalid data
    When POST request is sent to "/api/v1/vllm/instances" with body:
      """
      {
        "model": "",
        "host": "0.0.0.0",
        "port": 8000,
        "enable_prefix_caching": false
      }
      """
    Then response status should be 400
