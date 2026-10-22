@daybook
Feature: Daybook's personal task API
  As an authenticated user
  I want to manage my inbox and custom task lists
  So that every task belongs to my Gatehouse identity

  Background:
    Given daybook API is available

  Scenario Outline: Every task and project route requires a Gatehouse token
    When GET request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | path                    |
      | /api/v1/tasks           |
      | /api/v1/tasks/missing   |
      | /api/v1/projects        |
      | /api/v1/projects/missing |
      | /api/v1/labels          |

  Scenario Outline: Every write verb requires a Gatehouse token
    When an unauthenticated <method> request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | method | path                       |
      | POST   | /api/v1/tasks              |
      | PUT    | /api/v1/tasks/missing      |
      | DELETE | /api/v1/tasks/missing      |
      | POST   | /api/v1/projects           |
      | PUT    | /api/v1/projects/missing   |
      | DELETE | /api/v1/projects/missing   |
      | POST   | /api/v1/labels             |
      | PUT    | /api/v1/labels/missing     |
      | DELETE | /api/v1/labels/missing     |

  Scenario: A token without the Daybook audience is rejected
    Given I have a Gatehouse token with audience "sage"
    When GET request is sent to "/api/v1/tasks"
    Then the response status should be 401

  Scenario: A read audience is enough to reach the API
    Given I am authenticated against daybook with scope "daybook:read"
    When GET request is sent to "/api/v1/tasks"
    Then the response status should be 503
    And response should contain "Postgres"

  Scenario: A read-only token cannot write
    Given I am authenticated against daybook with scope "daybook:read"
    When an authenticated Daybook POST is sent to "/api/v1/tasks" with body:
      """
      {"content":"Reading is not enough to write"}
      """
    Then the response status should be 403

  Scenario: A write grant reaches the database layer
    Given I am authenticated against daybook with scope "daybook:write"
    When an authenticated Daybook POST is sent to "/api/v1/tasks" with body:
      """
      {"content":"Write permission is enough"}
      """
    Then the response status should be 503
    And response should contain "Postgres"

  Scenario Outline: Blank task, project and label names are rejected before database access
    Given I am authenticated against daybook with scope "daybook:write"
    When an authenticated Daybook POST is sent to "<path>" with body:
      """
      <body>
      """
    Then the response status should be 400

    Examples:
      | path             | body              |
      | /api/v1/tasks    | {"content":" "}   |
      | /api/v1/projects | {"name":" "}      |
      | /api/v1/labels   | {"name":" "}      |

  Scenario Outline: An invalid repeat rule is rejected before database access
    Given I am authenticated against daybook with scope "daybook:write"
    When an authenticated Daybook POST is sent to "/api/v1/tasks" with body:
      """
      <body>
      """
    Then the response status should be 400

    Examples:
      | body                                                                                             |
      | {"content":"x","recurrence":{"unit":"day","interval":1}}                                         |
      | {"content":"x","due_at":"2090-03-02T09:00:00Z","recurrence":{"unit":"day","interval":0}}        |
      | {"content":"x","due_at":"2090-03-02T09:00:00Z","recurrence":{"unit":"day","interval":1,"weekdays":[1]}} |

  Scenario: Daybook does not expose a web interface
    When GET request is sent to "/ui/home"
    Then the response status should be 404

  Scenario: Health endpoints are public
    When GET request is sent to "/health"
    Then the response status should be 200
