@mailroom
Feature: Who may use Mailroom
  Mailroom holds the passwords of people's mailboxes, so the first thing to show is who is kept out: no sign-in, no mail; a
  grant of another service, no mail; a reader cannot change anything; only an administrator reaches the panel.

  Background:
    Given mailroom API is available

  Scenario: The pages send a visitor who is not signed in to the sign-in
    When I open the mailroom path "/ui/home"
    Then response should be a redirect

  Scenario: The health probe answers without a sign-in
    When an unauthenticated GET request is sent to "/health/live"
    Then response status should be 200

  Scenario Outline: The API refuses a caller with no token
    When an unauthenticated GET request is sent to "<path>"
    Then response status should be 401

    Examples:
      | path                    |
      | /api/v1/me              |
      | /api/v1/accounts        |
      | /api/v1/messages        |
      | /api/v1/admin/settings  |
      | /api/v1/admin/stats     |
      | /api/v1/does-not-exist  |

  Scenario: A grant of another service opens nothing here
    Given I am signed in to mailroom as "access-sage" with "sage:write"
    When I list my mailboxes
    Then response status should be 403

  Scenario: A reader can look but not change
    Given I am signed in to mailroom as "access-reader" with "mailroom:read"
    When I list my mailboxes
    Then response status should be 200
    When I add my provider mailbox as "Not allowed"
    Then response status should be 403

  Scenario: The panel is for administrators
    Given I am signed in to mailroom as "access-writer" with "mailroom:write"
    When I read "/api/v1/admin/stats" on the mailroom panel
    Then response status should be 403
    When I read "/api/v1/admin/settings" on the mailroom panel
    Then response status should be 403
    Given I am signed in to mailroom as "access-admin" with "mailroom:admin"
    When I read "/api/v1/admin/stats" on the mailroom panel
    Then response status should be 200

  Scenario: A service's own credential sends mail and opens nothing else
    Given I am signed in to mailroom as "access-machine" with "service"
    When I list my mailboxes
    Then response status should be 403
    When I read "/api/v1/admin/settings" on the mailroom panel
    Then response status should be 403

  Scenario: A person cannot use the estate's send endpoint
    When I post to the estate send endpoint as "access-person" with "mailroom:admin"
    Then response status should be 403
    When I post to the estate send endpoint as "access-person" with "mailroom:write"
    Then response status should be 403

  Scenario: Every answer is private
    Given I am signed in to mailroom as "access-private" with "mailroom:read"
    When I list my mailboxes
    Then the last answer should have the header "cache-control" with "no-store"
    And the last answer should have the header "x-content-type-options" with "nosniff"
    And the last answer should have the header "referrer-policy" with "no-referrer"
