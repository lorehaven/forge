@warehouse
Feature: Sparse Index Protocol
  As a Cargo client
  I want to fetch crate metadata from the sparse index
  So that I can resolve dependencies

  Background:
    Given warehouse API is available

  Scenario: Fetch index config
    When GET request is sent to "/index/config.json"
    Then response status should be 200
    And response should contain "dl"
    And response should contain "api"

  Scenario: Fetch crate index entry
    Given valid token is obtained
    When a crate "test-crate-index-123" version "0.1.0" is published
    Then response status should be 200
    When GET request is sent to "/index/te/st/test-crate-index-123"
    Then response status should be 200
    And response content type should be "text/plain; charset=utf-8"
    And response should contain '"name":"test-crate-index-123"'
    And response should contain '"vers":"0.1.0"'

  Scenario: Fetch non-existent crate index entry
    When GET request is sent to "/index/no/n-/non-existent-crate"
    Then response status should be 404
