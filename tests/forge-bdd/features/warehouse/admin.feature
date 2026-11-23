@warehouse
Feature: Admin garbage collection permission enforcement
  As the estate's realm
  I want garbage collection to need a write grant
  So that an anonymous or read-only caller cannot delete registry storage

  Background:
    Given warehouse API is available

  Scenario: No token cannot run crates GC
    Given I hold no token
    When I run crates garbage collection
    Then response status should be 401

  Scenario: A read grant cannot run crates GC
    Given I hold a token scoped "user warehouse:read"
    When I run crates garbage collection
    Then response status should be 403

  Scenario: A read grant cannot run docker GC
    Given I hold a token scoped "user warehouse:read"
    When I run docker garbage collection
    Then response status should be 403

  # @serial: a crates collection removes what other scenarios of this process are in the middle of publishing and
  # yanking, so it runs alone.
  @serial
  Scenario: A write grant can run crates GC
    Given I hold a token scoped "user warehouse:write"
    When I run crates garbage collection
    Then response status should be 200
