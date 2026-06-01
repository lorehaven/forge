Feature: GPU API
  As a user
  I want to see GPU status
  So that I know the system's capabilities

  Background:
    Given switchboard API is available

  Scenario: Get GPU status JSON
    When GET request is sent to "/api/v1/gpu/status"
    Then response status should be 200
    And response should contain "name"
    And response should contain "total_gb"
    And response should contain "free_gb"

  Scenario: Get GPU status SSE
    When GET request is sent to "/api/v1/gpu/status/sse"
    Then response status should be 200
    And response content type should be "text/event-stream"
