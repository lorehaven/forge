@gatehouse
Feature: Gatehouse home page
  As someone signed in to the realm
  I want one page listing the services
  So that I can reach whatever this deployment actually runs

  Background:
    Given gatehouse API is available

  Scenario: The home page needs a session
    When I open the home page
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  # The suite configures sage and switchboard, and turns warehouse off through
  # its feature flag - so the page proves both halves of the gating rule.
  Scenario: Enabled services are listed
    Given I am signed in to the realm
    When I open the home page
    Then response status should be 200
    And response should contain "home-card-sage"
    And response should contain "home-card-switchboard"

  Scenario: A service turned off by its feature flag is not listed
    Given I am signed in to the realm
    When I open the home page
    Then response status should be 200
    And response should not contain "home-card-warehouse"

  Scenario: The home page is styled like the rest of the estate
    Given I am signed in to the realm
    When I open the home page
    Then response should contain "/gatehouse/ui/assets/css/gatehouse.css"
    And response should contain "ui_home_group_services"

  # The bare root and the bare base path both belong on the UI, not a 404.
  Scenario: The base path lands on the UI
    When I open the base path
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/ui"

  Scenario: A signed-in visit to /ui goes to the service list
    Given I am signed in to the realm
    When I open the UI root
    Then response should be a redirect
    And the redirect location should contain "/ui/home"

  Scenario: An anonymous visit to /ui goes to the login page
    When I open the UI root
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  # The same JSON shape every other service's /ui/status answers with, so a
  # front-end can ask any of them the same way.
  Scenario: The auth status endpoint is honest about being anonymous
    When GET request is sent to "/ui/status"
    Then response status should be 200
    And response should contain "authenticated"
    And response should contain "false"
