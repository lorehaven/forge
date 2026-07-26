@warehouse
Feature: Crates Registry API
  As a Rust developer
  I want to interact with the Crates Registry API
  So that I can manage Rust crates

  Background:
    Given warehouse API is available

  Scenario: Search for crates
    When GET request is sent to "/api/v1/crates?q=test"
    Then response status should be 200

  Scenario: Get crate metadata
    When GET request is sent to "/api/v1/crates/non-existent-crate"
    Then response status should be 404

  Scenario: Download a crate
    When GET request is sent to "/api/v1/crates/non-existent-crate/1.0.0/download"
    Then response status should be 404

  Scenario: Create and delete a crate (Publish and Yank)
    Given warehouse API is available
    Given valid token is obtained
    When a crate "test-crate-random-unique-xyz-789" version "1.0.0" is published
    Then response status should be 200
    When GET request is sent to "/api/v1/crates?q=test-crate-random-unique-xyz-789"
    Then response status should be 200
    When DELETE request is sent to "/api/v1/crates/test-crate-random-unique-xyz-789/1.0.0/yank" with token for crates
    Then response status should be 200
    When PUT request is sent to "/api/v1/crates/test-crate-random-unique-xyz-789/1.0.0/unyank" with token for crates
    Then response status should be 200

  Scenario: Manage crate owners
    Given valid token is obtained
    And a crate "test-owners-crate" version "1.0.0" is published
    When GET request is sent to "/api/v1/crates/test-owners-crate/owners"
    Then response status should be 200
    And response should be a JSON object
    When PUT request is sent to "/api/v1/crates/test-owners-crate/owners" with token and body:
      """
      {
        "users": ["new-owner"]
      }
      """
    Then response status should be 200
    When GET request is sent to "/api/v1/crates/test-owners-crate/owners"
    Then response status should be 200
    And response should contain "new-owner"
    When DELETE request is sent to "/api/v1/crates/test-owners-crate/owners" with token and body:
      """
      {
        "users": ["new-owner"]
      }
      """
    Then response status should be 200
    When GET request is sent to "/api/v1/crates/test-owners-crate/owners"
    Then response status should be 200
    And response should not contain "new-owner"

  Scenario: Publish crate without token
    When a crate "test-no-token" version "1.0.0" is published without token
    Then response status should be 401

  Scenario: Publish crate with invalid metadata
    Given valid token is obtained
    When a crate is published with invalid metadata:
      """
      {"invalid": "metadata"}
      """
    Then response status should be 400

  Scenario: Yank non-existent crate version
    Given valid token is obtained
    When DELETE request is sent to "/api/v1/crates/non-existent-crate/1.0.0/yank" with token for crates
    Then response status should be 404

  Scenario: Get index config
    When GET request is sent to "/index/config.json"
    Then response status should be 200
    And response should be a JSON object
    And response should contain "dl"

  Scenario: Get crate index
    Given valid token is obtained
    And a crate "test-index-crate" version "1.1.0" is published
    When GET request is sent to "/index/te/st/test-index-crate"
    Then response status should be 200
    And response should contain "test-index-crate"
    And response should contain "1.1.0"
