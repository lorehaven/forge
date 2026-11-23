@switchboard
Feature: GPU API
  As a user
  I want to see GPU status
  So that I know the system's capabilities

  Background:
    Given switchboard API is available
    And I am authenticated

  Scenario: Get GPU status JSON
    When GET request is sent to "/api/v1/gpu/status"
    Then response status should be 200
    And response should contain "name"
    And response should contain "total_gb"
    And response should contain "free_gb"

  # The page is a client-rendered application that polls this; there is no server-sent stream or HTML fragment.
  Scenario: GPU status is JSON for the client to poll
    When GET request is sent to "/api/v1/gpu/status"
    Then response status should be 200
    And response content type should be "application/json"
    And response should contain "used_gb"
