@gauge
Feature: Gauge's host metrics API
  As an administrator of the estate
  I want one place that shows how every host is doing
  So that a host that is down or full is noticed before it matters

  # Gauge watches two hosts in this suite: `bdd-host`, a stand-in for node_exporter that answers, and
  # `bdd-missing`, which nothing listens for. Its history needs a database and the suite runs without one.

  Background:
    Given gauge API is available

  Scenario Outline: Every API route needs a Gatehouse token
    When an unauthenticated GET request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | path                            |
      | /api/v1/hosts                   |
      | /api/v1/hosts/bdd-host          |
      | /api/v1/hosts/bdd-host/history  |
      | /api/v1/forecast                |
      | /api/v1/alerts/history          |

  Scenario: A token for another service is refused
    Given I have a Gatehouse token with audience "sage"
    When GET request is sent to "/api/v1/hosts"
    Then the response status should be 401

  Scenario: Every watched host is listed, the one that answers as up
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts"
    Then response status should be 200
    And response should be a JSON array
    And response should contain "\"name\":\"bdd-host\""
    And response should contain "\"name\":\"bdd-missing\""

  Scenario: A host that answers is reported with what it said
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts/bdd-host"
    Then response status should be 200
    And response should contain "\"up\":true"
    And response should contain "\"mem_total\":17179869184"
    And response should contain "\"device\":\"/dev/sda1\""

  Scenario: A host that does not answer is down, with the reason
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts/bdd-missing"
    Then response status should be 200
    And response should contain "\"up\":false"
    And response should contain "Connection refused"

  Scenario: An unknown host is not found
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts/no-such-host"
    Then response status should be 404
    And response should contain "no such host"

  # The host's name goes into a lookup, never into a path or a query.
  Scenario Outline: A hostile host name is not found
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts/<name>"
    Then the response status should not be 200
    And the response status should not be 500

    Examples:
      | name                    |
      | ..%2f..%2fetc%2fpasswd  |
      | %27%20OR%201%3D1--      |
      | %00                     |

  Scenario: Long-range history says plainly that it needs a database
    Given I am authenticated against gauge with scope "user gauge:read"
    When GET request is sent to "/api/v1/hosts/bdd-host/history"
    Then response status should be 503
    And response should contain "history is not available"

  Scenario: A page needs a session
    When I open the gauge path "/ui/fleet"
    Then response should be a redirect
    And the redirect location should contain "/ui/login"

  Scenario: Logging in is delegated to gatehouse
    When I open the gauge path "/ui/login"
    Then response should be a redirect
    And the redirect location should contain "/gatehouse/api/v1/authorize"
    And the redirect location should contain "client_id=gauge"

  Scenario: The application is served to anyone
    When I open the gauge path "/ui/assets/app.wasm"
    Then response status should be 200

  Scenario: The auth status is honest about being anonymous
    When GET request is sent to "/ui/status"
    Then response status should be 200
    And response should contain "\"authenticated\":false"

  Scenario: The service answers its health check without a token
    When GET request is sent to "/health"
    Then response status should be 200
