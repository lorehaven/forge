@workbench
Feature: Workbench's pages
  As someone signed in to the realm
  I want workbench's pages to behave like the rest of the estate
  So that the task manager is not a thing I have to learn separately

  Background:
    Given workbench API is available

  Scenario: The bare root lands on the UI
    When I open the workbench path ""
    Then response should be a redirect
    And the redirect location should contain "/workbench/ui"

  Scenario: An anonymous visit to /ui goes to the login page
    When I open the workbench path "/ui"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: The home page needs a session
    When I open the workbench path "/ui/home"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  # Gatehouse owns the login form; workbench only hands the browser over, and
  # starts the authorization-code + PKCE exchange so the visit comes back here
  # (redirect_uri) once it completes.
  Scenario: Logging in is delegated to gatehouse
    When I open the workbench path "/ui/login"
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/api/v1/authorize"
    And the redirect location should contain "redirect_uri="

  Scenario: Logging out is delegated to gatehouse
    When I open the workbench path "/ui/logout"
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/logout"

  Scenario: The auth status endpoint is honest about being anonymous
    When GET request is sent to "/ui/status"
    Then response status should be 200
    And response should contain "authenticated"
    And response should contain "false"

  Scenario: The page is styled like the rest of the estate
    When I open the workbench path "/ui/assets/css/workbench.css"
    Then response status should be 200
    And response should contain ".wb-board"

  # Not a 404: an anonymous visitor is sent to sign in before workbench says
  # anything about whether a project exists.
  Scenario: An anonymous visit to a project board goes to the login page
    When I open the workbench path "/ui/projects/does-not-exist/board"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: An anonymous visit to an issue page goes to the login page
    When I open the workbench path "/ui/issues/does-not-exist"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"
