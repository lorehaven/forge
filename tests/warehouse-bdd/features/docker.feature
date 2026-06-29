Feature: Docker Registry API
  As a developer
  I want to interact with the Docker Registry API
  So that I can manage Docker images

  Background:
    Given warehouse API is available

  Scenario: Get authentication token
    When token for service "warehouse" and scope "repository:test-repo:pull" is requested
    Then response status should be 500

  Scenario: Push manifest without token
    When PUT request is sent to "/v2/test-unauth-push/manifests/latest" without token but valid manifest
    Then response status should be 401

  Scenario: Delete manifest with invalid token
    When DELETE request is sent to "/v2/test-delete-repo/manifests/sha256:0000000000000000000000000000000000000000000000000000000000000000" with token
    Then response status should be 401
