@mailroom
Feature: The estate's mail goes through Mailroom
  Gatehouse and the other services hold no mail credentials and send nothing outside: they write to people through Mailroom
  with a machine token, and the message lands in the person's own mailbox. These scenarios run the real gatehouse and the
  real Mailroom.

  Background:
    Given gatehouse API is available

  Scenario: Registering delivers the verification message and its link
    When I register as "estate-vera" with password "secret-1" and email "estate-vera@example.com"
    Then mailroom should have delivered a "verification" message to "estate-vera"
    When I follow the verification link emailed to "estate-vera@example.com"
    And I log in with username "estate-vera" and password "secret-1"
    Then response status should be 200
    And "estate-vera" should find a message from gatehouse in their mailroom inbox

  Scenario: The stored address is not where mail goes
    When I register as "estate-wren" with password "secret-1" and email "estate-wren@example.com"
    Then mailroom should have delivered a "verification" message to "estate-wren"

  Scenario: A forgotten password is reset through the link
    When I register as "estate-rita" with password "secret-1" and email "estate-rita@example.com"
    And I follow the verification link emailed to "estate-rita@example.com"
    And I request a password reset for "estate-rita"
    Then mailroom should have delivered a "password-reset" message to "estate-rita"
    When I follow the password reset link emailed to "estate-rita@example.com" and set the password to "secret-2"
    And I log in with username "estate-rita" and password "secret-2"
    Then response status should be 200

  Scenario: An invitation is delivered
    Given I am administering the realm
    And no user "estate-ivan" exists
    When I invite a user "estate-ivan" at "estate-ivan@example.com"
    Then response status should be 201
    And mailroom should have delivered a "invite" message to "estate-ivan"

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

  Scenario: Nobody outside the estate can be written to, and a service cannot reach a mailbox by guessing
    Given mailroom API is available
    When a service sends "Outside" to "someone@gmail.com" through mailroom
    Then response status should be 400
    And response should contain "stays inside this estate"
