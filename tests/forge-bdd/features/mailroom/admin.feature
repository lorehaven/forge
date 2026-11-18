@mailroom
Feature: The administrator's panel
  The panel shows and changes the estate's sending server, who services may send as, and the limits; it lists everybody's
  mailboxes as metadata and never their mail; and it keeps a record of what was done.

  Background:
    Given mailroom API is available
    And I am signed in to mailroom as "panel-admin" with "mailroom:admin"

  Scenario: The settings never show the sending server's password
    When I read "/api/v1/admin/settings" on the mailroom panel
    Then response status should be 200
    And response should contain "\"has_password\":true"
    And response should contain "noreply@bdd.example"
    And response should not contain "bdd-relay-password"

  Scenario: Changing a limit is recorded
    When I set the mailroom daily budget to 777
    Then response status should be 200
    And response should contain "\"daily_budget\":777"
    When I read "/api/v1/admin/audit" on the mailroom panel
    Then response should contain "settings.update"
    And response should contain "panel-admin"
    When I set the mailroom daily budget to 500
    Then response status should be 200

  Scenario: Nonsense settings are refused
    When I set the mailroom daily budget to 0
    Then response status should be 400

  Scenario: The sending server can be tried
    When I post "/api/v1/admin/transport/test" on the mailroom panel
    Then response status should be 200
    And response should contain "\"ok\":true"

  Scenario: The panel lists mailboxes but never the mail in them
    Given the provider mailbox holds a message "Confidential merger plans" from "Alice <alice@provider.example>"
    And I am signed in to mailroom as "panel-user" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    Given I am signed in to mailroom as "panel-admin" with "mailroom:admin"
    When I read "/api/v1/admin/accounts" on the mailroom panel
    Then response status should be 200
    And response should contain "panel-user"
    And response should not contain "Confidential merger plans"
    And the response should not contain the mailbox password

  Scenario: The queue shows what the estate sent, without the messages
    When a service sends "Queue visibility check" to "queue@example.org" through mailroom
    Then response status should be 202
    Given I am signed in to mailroom as "panel-admin" with "mailroom:admin"
    When I read "/api/v1/admin/outbox" on the mailroom panel
    Then response status should be 200
    And response should contain "Queue visibility check"
    And response should not contain "Text of Queue visibility check"

  Scenario: Only a sender address the administrator allowed can be used
    When a service sends "Spoof attempt" to "victim@example.org" as "ceo@bdd.example" through mailroom
    Then response status should be 400
    And response should contain "not allowed"
    Given I am signed in to mailroom as "panel-admin" with "mailroom:admin"
    When I allow the sender address "alerts@bdd.example"
    Then response status should be 200
    When a service sends "Allowed sender" to "victim@example.org" as "alerts@bdd.example" through mailroom
    Then response status should be 202
    And the message to "victim@example.org" with the subject "Allowed sender" should be sent as "alerts@bdd.example"
