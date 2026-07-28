@conveyor
Feature: Conveyor's pages
  As someone signed in to the realm
  I want conveyor's pages to behave like the rest of the estate
  So that the CI service is not a thing I have to learn separately

  Background:
    Given conveyor API is available

  Scenario: The bare root lands on the UI
    When I open the conveyor path ""
    Then response should be a redirect
    And the redirect location should contain "/conveyor/ui"

  Scenario: An anonymous visit to /ui goes to the login page
    When I open the conveyor path "/ui"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: The home page needs a session
    When I open the conveyor path "/ui/home"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  # Gatehouse owns the login form; conveyor only hands the browser over, and
  # carries a return address so the visit comes back here.
  Scenario: Logging in is delegated to gatehouse
    When I open the conveyor path "/ui/login"
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/login"
    And the redirect location should contain "redirect="

  Scenario: Logging out is delegated to gatehouse
    When I open the conveyor path "/ui/logout"
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui/logout"

  Scenario: The auth status endpoint is honest about being anonymous
    When GET request is sent to "/ui/status"
    Then response status should be 200
    And response should contain "authenticated"
    And response should contain "false"

  Scenario: The page is styled like the rest of the estate
    When I open the conveyor path "/ui/assets/css/conveyor.css"
    Then response status should be 200
    And response should contain ".status-success"
    And response should contain ".run-table"

  # Not a 404: an anonymous visitor is sent to sign in before conveyor says
  # anything about whether a run exists.
  Scenario: An anonymous visit to a run page goes to the login page
    When I open the conveyor path "/ui/runs/does-not-exist"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"
