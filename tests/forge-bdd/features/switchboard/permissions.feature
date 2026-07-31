@switchboard
Feature: Switchboard per-action permissions
  As the estate's identity service
  I want launch, stop and delete-model gated independently
  So that a grant for one never implies the others

  # Switchboard's catalog entry declares no "write" action at all, only these
  # three - a coarse write grant could never satisfy any of them. Each route
  # checks `mod_impl::can` for its own action; these scenarios are what proves
  # the three stay independent rather than collapsing back into one.

  Background:
    Given switchboard API is available

  Scenario: A read-only token cannot launch an instance
    Given I hold a switchboard token scoped "user switchboard:read"
    When POST request is sent to "/api/v1/vllm/instances" with body:
      """
      {
        "model": "test-model",
        "host": "0.0.0.0",
        "port": 8001,
        "enable_prefix_caching": false
      }
      """
    Then response status should be 403

  Scenario: A launch grant is enough to launch, and nothing more
    Given I hold a switchboard token scoped "user switchboard:launch"
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
    When DELETE request is sent to "/api/v1/vllm/instances/{last_id}"
    Then response status should be 403

  Scenario: Stopping the instance a launch grant started needs its own stop grant
    Given I hold a switchboard token scoped "user switchboard:launch"
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
    Given I hold a switchboard token scoped "user switchboard:stop"
    When DELETE request is sent to "/api/v1/vllm/instances/{last_id}"
    Then response status should be 200

  Scenario: A read-only token cannot delete a model
    Given I hold a switchboard token scoped "user switchboard:read"
    When POST request is sent to "/api/v1/models/delete" with body:
      """
      {
        "path": "/not/a/real/model"
      }
      """
    Then response status should be 403

  Scenario: A delete-model grant reaches the route, a launch grant does not
    Given I hold a switchboard token scoped "user switchboard:launch"
    When POST request is sent to "/api/v1/models/delete" with body:
      """
      {
        "path": "/not/a/real/model"
      }
      """
    Then response status should be 403
    Given I hold a switchboard token scoped "user switchboard:delete-model"
    When POST request is sent to "/api/v1/models/delete" with body:
      """
      {
        "path": "/not/a/real/model"
      }
      """
    Then response status should be 403
    And response should contain "api_error_invalid_model_path"
