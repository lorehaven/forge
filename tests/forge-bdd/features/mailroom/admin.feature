@mailroom
Feature: The administrator's panel
  The panel shows how the estate's mail is doing and manages mailboxes, but it never shows anybody's mail: not the subjects of
  what people wrote to each other, not the text of anything.

  Background:
    Given mailroom API is available
    And I am signed in to mailroom as "panel-adam" with "mailroom:admin"

  Scenario: The overview counts what is there
    When I read "/api/v1/admin/stats" on the mailroom panel
    Then response status should be 200
    And response should contain "mailboxes"
    And response should contain "budget_left"

  Scenario: Settings are shown and changed, and a bad value changes nothing
    When I read "/api/v1/admin/settings" on the mailroom panel
    Then response should contain "bdd.example"
    When I set the mailroom daily budget to 777
    Then response status should be 200
    And response should contain "777"
    When I set the mailroom daily budget to 0
    Then response status should be 400
    When I read "/api/v1/admin/settings" on the mailroom panel
    Then response should contain "777"
    When I set the mailroom daily budget to 2000
    Then response status should be 200

  Scenario: The mailboxes are listed with their size, never their mail
    Given "panel-ann" has a mailbox
    And "panel-bob" has a mailbox
    And I am signed in to mailroom as "panel-ann" with "mailroom:write"
    When I write "A very private subject" to "panel-bob"
    And "panel-adam" signs in to mailroom with "mailroom:admin"
    And I read "/api/v1/admin/mailboxes?limit=200" on the mailroom panel
    Then response status should be 200
    And response should contain "panel-bob@bdd.example"
    And response should not contain "A very private subject"
    And response should not contain "Text of"

  Scenario: What the services delivered is listed, what people wrote to each other is not
    Given "log-ann" has a mailbox
    And "log-bob" has a mailbox
    And I am signed in to mailroom as "log-ann" with "mailroom:write"
    When I write "Between friends" to "log-bob"
    And a service sends "From a service" to "log-bob" through mailroom
    And "panel-adam" signs in to mailroom with "mailroom:admin"
    And I read "/api/v1/admin/deliveries?limit=200" on the mailroom panel
    Then response should contain "From a service"
    And response should not contain "Between friends"

  Scenario: A mailbox can be switched off, still receives mail, and can be switched on again
    Given "off-ann" has a mailbox
    When I post "/api/v1/admin/mailboxes/off-ann/disable" on the mailroom panel
    Then response status should be 204
    When a service sends "While you were off" to "off-ann" through mailroom
    Then response status should be 201
    When "off-ann" signs in to mailroom with "mailroom:write"
    And I list my folders
    Then response status should be 403
    When "panel-adam" signs in to mailroom with "mailroom:admin"
    And I post "/api/v1/admin/mailboxes/off-ann/enable" on the mailroom panel
    And "off-ann" signs in to mailroom with "mailroom:write"
    And I list the messages in my inbox
    Then the message list should show 1 message

  Scenario: A mailbox is deleted with everything in it, and the deletion is in the audit log
    Given "gone-ann" has a mailbox
    When a service sends "Soon gone" to "gone-ann" through mailroom
    And I delete "/api/v1/admin/mailboxes/gone-ann" on the mailroom panel
    Then response status should be 204
    When I delete "/api/v1/admin/mailboxes/gone-ann" on the mailroom panel
    Then response status should be 404
    When I read "/api/v1/admin/audit" on the mailroom panel
    Then response should contain "delete-mailbox"
    And response should contain "gone-ann"

  Scenario: The size of a mailbox is limited
    Given "full-ann" has a mailbox
    When I set the mailroom mailbox size to 1 megabytes
    And a service sends "First" to "full-ann" through mailroom
    Then response status should be 201
    When I set the mailroom mailbox size to 500 megabytes
    Then response status should be 200
