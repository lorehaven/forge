@daybook @postgres
Feature: Daybook keeps each person's tasks on a database
  As a person with tasks
  I want my tasks kept for me and nobody else
  So that what I write down is mine, whoever else uses the service

  # These run only when the services were given a Postgres (`FORGE_BDD_DATABASE_URL`, the daybook module installed):
  # a store that forgets everything on restart cannot show what a database must - that a task is still there, and still
  # only its owner's.

  Background:
    Given daybook API is available

  Scenario: A task is kept and comes back in the list
    Given "bdd-alice" created the task "Buy bulbs"
    When GET request is sent to "/api/v1/tasks"
    Then response status should be 200
    And response should contain "Buy bulbs"

  Scenario: A task can be read back, changed and completed
    Given "bdd-alice" created the task "Post parcel"
    When an authenticated Daybook PUT is sent to "/api/v1/tasks/{last_id}" with body:
      """
      {"content": "Post the parcel", "is_completed": true}
      """
    Then response status should be 200
    And response should contain "Post the parcel"
    And response should contain "\"is_completed\":true"

  Scenario: Somebody else's list does not show it
    Given "bdd-alice" created the task "Alice's private errand"
    And I am authenticated against daybook as "bdd-bob"
    When GET request is sent to "/api/v1/tasks"
    Then response status should be 200
    And response should not contain "Alice's private errand"

  Scenario: Somebody else cannot change it
    Given "bdd-alice" created the task "Alice's other errand"
    And I am authenticated against daybook as "bdd-bob"
    When an authenticated Daybook PUT is sent to "/api/v1/tasks/{last_id}" with body:
      """
      {"content": "Stolen", "is_completed": true}
      """
    Then response status should be 404

  Scenario: Somebody else cannot delete it, and its owner still can
    Given "bdd-alice" created the task "Alice's third errand"
    And I am authenticated against daybook as "bdd-bob"
    When DELETE request is sent to "/api/v1/tasks/{last_id}"
    Then response status should be 404
    Given I am authenticated against daybook as "bdd-alice"
    When DELETE request is sent to "/api/v1/tasks/{last_id}"
    Then the response status should not be 404
    When GET request is sent to "/api/v1/tasks"
    Then response should not contain "Alice's third errand"

  Scenario: A task with no content is refused
    Given I am authenticated against daybook as "bdd-alice"
    When an authenticated Daybook POST is sent to "/api/v1/tasks" with body:
      """
      {"content": "   "}
      """
    Then response status should be 400
