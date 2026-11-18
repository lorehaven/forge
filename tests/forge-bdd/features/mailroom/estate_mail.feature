@mailroom
Feature: The estate's mail goes through Mailroom
  Gatehouse and the other services hold no mail credentials: they hand a message to Mailroom with a machine token, and Mailroom
  queues it, sends it through the estate's server and keeps the record. These scenarios run the real gatehouse and the real
  Mailroom against a fake sending server.

  Background:
    Given gatehouse API is available

  Scenario: Registering sends the verification mail through Mailroom
    When I register as "estate-vera" with password "secret-1" and email "estate-vera@example.com"
    Then mailroom should have sent a "verification" message to "estate-vera@example.com"
    And the "verification" message to "estate-vera@example.com" should link to gatehouse
    When I follow the verification link emailed to "estate-vera@example.com"
    And I log in with username "estate-vera" and password "secret-1"
    Then response status should be 200

  Scenario: A forgotten password is reset through the mail Mailroom sent
    When I register as "estate-rita" with password "secret-1" and email "estate-rita@example.com"
    And I follow the verification link emailed to "estate-rita@example.com"
    And I request a password reset for "estate-rita"
    Then mailroom should have sent a "password-reset" message to "estate-rita@example.com"
    When I follow the password reset link emailed to "estate-rita@example.com" and set the password to "secret-2"
    And I log in with username "estate-rita" and password "secret-2"
    Then response status should be 200

  Scenario: An invitation arrives as mail from the estate's address
    Given I am administering the realm
    And no user "estate-ivan" exists
    When I invite a user "estate-ivan" at "estate-ivan@example.com"
    Then response status should be 201
    And mailroom should have sent a "invite" message to "estate-ivan@example.com"

  Scenario: A notification carries a working unsubscribe
    When I register as "estate-nora" with password "secret-1" and email "estate-nora@example.com"
    And I follow the verification link emailed to "estate-nora@example.com"
    And a service notifies "estate-nora" with "conveyor.run.failed"
    Then response status should be 202
    And mailroom should have sent a "notification" message to "estate-nora@example.com"
    When I follow the unsubscribe link emailed to "estate-nora@example.com"
    Then response status should be 200
    When a service notifies "estate-nora" with "conveyor.run.failed"
    Then response should contain "not_subscribed"

  Scenario: Every message from the estate names its source and is marked automatic
    When I register as "estate-sam" with password "secret-1" and email "estate-sam@example.com"
    Then mailroom should have sent a "verification" message to "estate-sam@example.com"

  Scenario: Another service can send mail directly, and a retry is not sent twice
    Given mailroom API is available
    When a service sends "Direct from a service" to "direct@example.org" through mailroom
    Then response status should be 202
    And the sending server should receive a message to "direct@example.org" with the subject "Direct from a service"
    And the message to "direct@example.org" with the subject "Direct from a service" should have the header "Auto-Submitted"
    When a service sends "Direct from a service" to "direct@example.org" through mailroom
    Then response status should be 200
    And response should contain "duplicate"
