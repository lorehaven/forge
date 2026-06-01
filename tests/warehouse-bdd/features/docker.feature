Feature: Docker Registry API
  As a developer
  I want to interact with the Docker Registry API
  So that I can manage Docker images

  Background:
    Given warehouse API is available

  Scenario: Check registry version
    Given valid token for scope "repository:*:pull" is obtained
    When GET request is sent to "/v2/" with token
    Then response status should be 200

  Scenario: Get authentication token
    When token for service "warehouse" and scope "repository:test-repo:pull" is requested
    Then response status should be 200
    And response should contain a JWT token

  Scenario: List tags for a repository
    Given valid token for scope "repository:non-existent-repo:pull" is obtained
    When GET request is sent to "/v2/non-existent-repo/tags/list" with token
    Then response status should be 404

  Scenario: Create and delete a repository (Push and Delete)
    Given valid token for scope "repository:test-repo-final:push,pull" is obtained
    When PUT request is sent to "/v2/test-repo-final/manifests/latest" with token and valid manifest
    Then response status should be 201
    And response should contain header "Docker-Content-Digest"
    When GET request is sent to "/v2/test-repo-final/tags/list" with token
    Then response status should be 200
    And response should contain tag "latest"
    When DELETE request is sent to repository "test-repo-final" with digest from header "Docker-Content-Digest" and token
    Then response status should be 202

  Scenario: Get non-existent manifest
    Given valid token for scope "repository:non-existent:pull" is obtained
    When GET request is sent to "/v2/non-existent/manifests/latest" with token
    Then response status should be 404

  Scenario: Get non-existent blob
    Given valid token for scope "repository:non-existent:pull" is obtained
    When GET request is sent to "/v2/non-existent/blobs/sha256:1234567890123456789012345678901234567890123456789012345678901234" with token
    Then response status should be 404

  Scenario: Push manifest without token
    When PUT request is sent to "/v2/test-unauth-push/manifests/latest" without token but valid manifest
    Then response status should be 401

  Scenario: Delete non-existent manifest
    Given valid token for scope "repository:test-delete-repo:push,pull" is obtained
    When DELETE request is sent to "/v2/test-delete-repo/manifests/sha256:0000000000000000000000000000000000000000000000000000000000000000" with token
    Then response status should be 404
