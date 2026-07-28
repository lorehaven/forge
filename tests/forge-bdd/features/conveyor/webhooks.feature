@conveyor
Feature: Conveyor's webhook endpoint
  As the one endpoint an unauthenticated stranger can reach
  I want it to refuse everything it can decide without a database
  So that reaching conveyor is not the same as being able to build with it

  # Conveyor reads a delivery, finds the repository it names, and only then
  # verifies the signature - because the secret is per repository and the body
  # is the only thing that says which repository this is.
  #
  # That means the signature checks need a database, and this suite runs on an
  # in-memory store. What is covered here is everything decided *before* the
  # lookup. The signature behaviour itself has twelve scenarios in
  # `docker/conveyor-service/tests/integration/webhook_tests.rs`, against a
  # real Postgres.

  Background:
    Given conveyor API is available

  Scenario: A ping is accepted and does nothing
    When a signed github ping is sent
    Then the response status should be 202

  # `git` reads a leading `-` as an option, and the ref comes from a body
  # somebody else wrote. Refused before anything looks the repository up.
  Scenario: A ref git would read as an option is refused
    When a signed github push is sent for "nobody/unregistered" with ref "--upload-pack=/tmp/x"
    Then the response status should be 400
    And response should contain "option"

  Scenario: A provider conveyor has no integration with is a 404
    When a delivery is sent to the "gitlab" webhook endpoint
    Then the response status should be 404

  # A provider has no realm token; its delivery is authenticated by its
  # signature instead. A 401 here would mean the endpoint had been put behind
  # the realm's middleware by mistake.
  Scenario: Webhooks are not behind the realm's auth
    When a signed github push is sent for "nobody/unregistered" with ref "--upload-pack=/tmp/x"
    Then the response status should not be 401
