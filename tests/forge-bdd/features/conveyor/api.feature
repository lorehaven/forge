@conveyor
Feature: Conveyor's API
  As an operator
  I want the API to need a realm token and to be honest about what it cannot do
  So that nothing builds without an identity and nothing fails silently

  Background:
    Given conveyor API is available

  # Every one of these was unreachable once, because a scope registered before
  # them swallowed the prefix and 404ed inside itself.
  Scenario Outline: Every API route needs a token
    When GET request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | path                |
      | /api/v1/repos       |
      | /api/v1/repos/abc   |
      | /api/v1/runs        |
      | /api/v1/runs/abc    |
      | /api/v1/secrets     |

  # 401 rather than 404: the auth middleware wraps the scope, so it answers
  # before a path is matched inside it. That is the right order - an
  # unauthenticated caller should not be able to map the API by probing it.
  Scenario: An unknown path under the API is refused rather than described
    When GET request is sent to "/api/v1/nonsense"
    Then the response status should be 401

  # The suite runs on an in-memory store on purpose. Conveyor says so rather
  # than looking healthy and losing every queued run on restart.
  Scenario: The queue refuses an in-memory database
    Given I am authenticated against conveyor
    When an authenticated GET is sent to "/api/v1/runs"
    Then the response status should be 503
    And response should contain "Postgres"

  Scenario: A token gets past the middleware
    Given I am authenticated against conveyor
    When an authenticated GET is sent to "/api/v1/repos"
    Then the response status should not be 401

  Scenario: Health is reported without a token
    When GET request is sent to "/health"
    Then response status should be 200

  Scenario: Readiness is reported without a token
    When GET request is sent to "/health/ready"
    Then response status should be 200
    And response should contain "ready"
