@workbench @postgres
Feature: Workbench keeps projects and issues on a database
  As a team
  I want projects and issues to stay where they were put
  So that who may see and change them is decided by the grant a token carries, not by luck

  # Run only when the services were given a Postgres (`FORGE_BDD_DATABASE_URL`, the workbench module installed): the in-memory
  # run cannot say what a grant lets a person do to a real project.

  Background:
    Given workbench API is available

  Scenario: A project is created and listed
    Given a workbench project "BDDA" exists
    And I am authenticated against workbench
    When an authenticated GET request is sent to "/api/v1/projects"
    Then response status should be 200
    And response should contain "BDDA"

  Scenario: An issue starts as todo and moves through its statuses
    Given a workbench project "BDDB" exists
    And I am authenticated against workbench
    When an authenticated POST is sent to "/api/v1/projects/{project_id}/issues" with body:
      """
      {"title": "Fix the thing"}
      """
    Then response status should be 201
    And response should contain "\"status\":\"todo\""
    When an authenticated POST is sent to "/api/v1/issues/{last_id}/transition" with body:
      """
      {"status": "in-progress"}
      """
    Then response status should be 200
    And response should contain "in-progress"

  Scenario: An unknown status is refused
    Given a workbench project "BDDC" exists
    And I am authenticated against workbench
    When an authenticated POST is sent to "/api/v1/projects/{project_id}/issues" with body:
      """
      {"title": "Odd one"}
      """
    And an authenticated POST is sent to "/api/v1/issues/{last_id}/transition" with body:
      """
      {"status": "archived"}
      """
    Then the response status should not be 200
    And the response status should not be 500

  Scenario: A grant on one project lets a person work in it
    Given a workbench project "BDDD" exists
    And I am authenticated against workbench with scope "workbench:project:{project_id}:write"
    When an authenticated POST is sent to "/api/v1/projects/{project_id}/issues" with body:
      """
      {"title": "Mine to file"}
      """
    Then response status should be 201

  Scenario: A grant on one project does not reach another
    Given a workbench project "BDDE" exists
    And I am authenticated against workbench with scope "workbench:project:some-other-project:write"
    When an authenticated POST is sent to "/api/v1/projects/{project_id}/issues" with body:
      """
      {"title": "Not mine"}
      """
    Then response status should be 403

  Scenario: A reader can look but not file
    Given a workbench project "BDDF" exists
    And I am authenticated against workbench with scope "workbench:read"
    When an authenticated GET request is sent to "/api/v1/projects/{project_id}/issues"
    Then response status should be 200
    When an authenticated POST is sent to "/api/v1/projects/{project_id}/issues" with body:
      """
      {"title": "Reader's attempt"}
      """
    Then response status should be 403

  Scenario: A project that is not there is not found
    Given I am authenticated against workbench
    When an authenticated GET request is sent to "/api/v1/projects/00000000-0000-0000-0000-000000000000"
    Then response status should be 404

  Scenario: A project with no key is refused
    Given I am authenticated against workbench
    When an authenticated POST is sent to "/api/v1/projects" with body:
      """
      {"key": "  ", "name": "Anything"}
      """
    Then response status should be 400
