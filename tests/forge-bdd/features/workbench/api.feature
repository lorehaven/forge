@workbench
Feature: Workbench's API
  As an operator
  I want the API to need a realm token and to respect resource-scoped grants
  So that nothing reads or writes a project without the right identity

  Background:
    Given workbench API is available

  # Every one of these was unreachable once, because a scope registered before
  # them swallowed the prefix and 404ed inside itself.
  Scenario Outline: Every API route needs a token
    When GET request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | path                 |
      | /api/v1/projects     |
      | /api/v1/projects/abc |
      | /api/v1/issues/abc   |
      | /api/v1/comments/abc |
      | /api/v1/labels/abc   |

  # 401 rather than 404: the auth middleware wraps the scope, so it answers
  # before a path is matched inside it. That is the right order - an
  # unauthenticated caller should not be able to map the API by probing it.
  Scenario: An unknown path under the API is refused rather than described
    When GET request is sent to "/api/v1/nonsense"
    Then the response status should be 401

  # The suite runs on an in-memory store on purpose. Workbench says so rather
  # than looking healthy and losing every project and issue on restart.
  Scenario: The domain layer refuses an in-memory database
    Given I am authenticated against workbench
    When an authenticated GET request is sent to "/api/v1/projects"
    Then the response status should be 503
    And response should contain "Postgres"

  Scenario: A token gets past the middleware
    Given I am authenticated against workbench
    When an authenticated GET request is sent to "/api/v1/projects"
    Then the response status should not be 401

  Scenario: A caller with only read access cannot create a project
    Given I am authenticated against workbench with scope "workbench:read"
    When an authenticated POST is sent to "/api/v1/projects" with body:
      """
      {"key": "WB", "name": "Workbench"}
      """
    Then the response status should be 403

  # workbench's projects are flat, unlike conveyor's tree - a grant naming one
  # project id never covers another, with no ancestor walk to bend that.
  Scenario: A project-scoped grant does not cover a different project
    Given I am authenticated against workbench with scope "workbench:project:some-other-project:write"
    When an authenticated POST is sent to "/api/v1/projects/not-that-project/issues" with body:
      """
      {"title": "an issue"}
      """
    Then the response status should be 403

  Scenario: Health is reported without a token
    When GET request is sent to "/health"
    Then response status should be 200

  Scenario: Readiness is reported without a token
    When GET request is sent to "/health/ready"
    Then response status should be 200
