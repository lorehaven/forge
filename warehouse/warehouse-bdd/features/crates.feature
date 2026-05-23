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
