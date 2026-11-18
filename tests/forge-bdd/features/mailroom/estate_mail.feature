@mailroom
Feature: The estate's mail goes through Mailroom
  Gatehouse and the other services hold no mail credentials and send nothing outside: they write to people through Mailroom
  with a machine token, and the message lands in the person's own mailbox. The messages that carry a link which grants
  something (an account, a password) are the exception: they are never put in the account's own mailbox (whoever held a
  live session could use them to take the account over) but handed over by the administrator from Gatehouse's log. These
  scenarios run the real gatehouse and the real Mailroom.

  Background:
    Given gatehouse API is available

  Scenario: Registering works through the link in Gatehouse's log, and nothing is put in the mailbox
    When I register as "estate-vera" with password "secret-1" and email "estate-vera@example.com"
    Then mailroom should not have delivered a "verification" message to "estate-vera"
    When I follow the verification link emailed to "estate-vera@example.com"
    And I log in with username "estate-vera" and password "secret-1"
    Then response status should be 200

  Scenario: A forgotten password is reset through the link, and the notice arrives in the mailbox
    When I register as "estate-rita" with password "secret-1" and email "estate-rita@example.com"
    And I follow the verification link emailed to "estate-rita@example.com"
    And I request a password reset for "estate-rita"
    Then mailroom should not have delivered a "password-reset" message to "estate-rita"
    When I follow the password reset link emailed to "estate-rita@example.com" and set the password to "secret-2"
    And I log in with username "estate-rita" and password "secret-2"
    Then response status should be 200
    And mailroom should have delivered a "password-changed" message to "estate-rita"
    And "estate-rita" should find a message from gatehouse in their mailroom inbox

  Scenario: An invitation is handed over from the log, not delivered
    Given I am administering the realm
    And no user "estate-ivan" exists
    When I invite a user "estate-ivan" at "estate-ivan@example.com"
    Then response status should be 201
    And mailroom should not have delivered a "invite" message to "estate-ivan"

  Scenario: A notification carries a working unsubscribe
    When I register as "estate-nora" with password "secret-1" and email "estate-nora@example.com"
    And I follow the verification link emailed to "estate-nora@example.com"
    And a service notifies "estate-nora" with "conveyor.run.failed"
    Then response status should be 202
    And mailroom should have delivered a "notification" message to "estate-nora"
    When I follow the unsubscribe link emailed to "estate-nora@example.com"
    Then response status should be 200
    When a service notifies "estate-nora" with "conveyor.run.failed"
    Then response should contain "not_subscribed"

  Scenario: Another service can write to a person directly, and a retry is not delivered twice
    Given mailroom API is available
    When a service sends "Direct from a service" to "direct-dana" through mailroom
    Then response status should be 201
    When a service sends "Direct from a service" to "direct-dana" through mailroom
    Then response status should be 200
    And response should contain "duplicate"
    When "direct-dana" signs in to mailroom with "mailroom:read"
    And I list the messages in my inbox
    Then the message list should show 1 message
    And the first message should be from "bdd-service@bdd.example"
    And the first message should be marked as automatic

  Scenario Outline: A service can only name a mailbox exactly: no address, no other case, nobody outside the estate
    Given mailroom API is available
    When a service sends "Odd" to "<name>" through mailroom
    Then response status should be 400
    And response should contain "cannot have a mailbox here"

    Examples:
      | name              |
      | someone@gmail.com |
      | Alice             |
      | alice@bdd.example |
      | a+b               |

  Scenario: Nobody can have a mailbox under a service's name
    Given mailroom API is available
    When a service sends "Spoof" to "gatehouse" through mailroom
    Then response status should be 400
    Given I am signed in to mailroom as "gatehouse" with "mailroom:write"
    When I open my mailbox
    Then response status should be 400
