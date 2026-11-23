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

  # `/instances` is the REST-shaped sibling of `/list`; both answer with the
  # same array, and a client that guessed the plural should not get a 404.
  Scenario: List vLLM instances under the REST path
    When GET request is sent to "/api/v1/vllm/instances"
    Then response status should be 200
    And response should be a JSON array

  # What the client asks to decide which controls to draw.
  Scenario: A reader is told it cannot launch, stop or delete
    When GET request is sent to "/api/v1/vllm/capabilities"
    Then response status should be 200
    And response content type should be "application/json"
    And response should contain "\"vllm_management\":true"

  Scenario: List running models
    When GET request is sent to "/api/v1/models/running"
    Then response status should be 200
    And response should be a JSON array

  # @serial: the launch-then-stop scenarios (here and in permissions.feature)
  # share the switchboard process's mock instance registry and all bind the
  # same port, so running them concurrently makes one scenario's launch evict
  # another's before its stop-by-id lands.
  @serial
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

  @serial
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
