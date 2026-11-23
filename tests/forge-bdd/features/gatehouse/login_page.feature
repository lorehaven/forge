@gatehouse
Feature: Gatehouse login page
  As a user of any Forge service
  I want one login page
  So that signing in once signs me in everywhere

  Background:
    Given gatehouse API is available

  # The pages are a client-rendered application: the server sends one shell, and the labels are
  # i18n keys the client resolves, so what is asserted here is the shell and its assets.
  Scenario: The login page is the application shell
    When GET request is sent to "/ui/login"
    Then response status should be 200
    And response should contain "<div id=\"app\">"
    And response should contain "/gatehouse/ui/assets/quench.js"
    And response should contain "/gatehouse/ui/assets/app.wasm"

  Scenario: The login page is styled like the rest of the estate
    When GET request is sent to "/ui/login"
    Then response status should be 200
    And response should contain "/gatehouse/ui/assets/app.css"

  Scenario: The stylesheet and the application are served
    When GET request is sent to "/ui/assets/app.css"
    Then response status should be 200
    When GET request is sent to "/ui/assets/app.wasm"
    Then response status should be 200

  Scenario: Signing in sets the realm cookie
    When I submit the login form with username "admin" and password "password"
    Then response should be a redirect
    And a realm session cookie should be set
    And the session cookie should be scoped to the whole site

  Scenario: Bad credentials return to the form with an error
    When I submit the login form with username "admin" and password "nope"
    Then response should be a redirect
    And the redirect location should contain "err=1"
    And no realm session cookie should be set

  # The client reads ?redirect= itself; the server only ever sends the shell, whatever it is given.
  Scenario: The login page is the same shell whatever the return address
    When GET request is sent to "/ui/login?redirect=%2Fwarehouse%2Fui%2Fhome"
    Then response status should be 200
    And response should contain "/gatehouse/ui/assets/quench.js"

  Scenario: Signing in returns the browser to the relying party
    When I submit the login form with redirect "/warehouse/ui/home"
    Then response should be a redirect
    And the redirect location should contain "/warehouse/ui/home"

  # An unvalidated ?redirect= would make the login page a phishing primitive.
  Scenario: An off-realm return address is refused
    When GET request is sent to "/ui/login?redirect=https%3A%2F%2Fevil.example.com"
    Then response status should be 200
    And response should not contain "evil.example.com"

  Scenario: A protocol-relative return address is refused
    When GET request is sent to "/ui/login?redirect=%2F%2Fevil.example.com"
    Then response status should be 200
    And response should not contain "evil.example.com"

  Scenario: Logging out clears the realm cookie
    Given I am logged in as "admin"
    When I visit the logout page
    Then response should be a redirect
    And the realm session cookie should be cleared
