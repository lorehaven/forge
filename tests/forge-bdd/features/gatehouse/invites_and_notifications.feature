@gatehouse
Feature: Invitations, username rules and service notifications
  As an administrator I invite people rather than choosing passwords for them,
  and other services tell people what happened through gatehouse rather than
  each sending mail themselves - and only what those people asked for.

  # `LoggingSender` writes each email to gatehouse's own log, which is how these
  # scenarios read links back (invitation, unsubscribe) and check what was sent.

  Background:
    Given gatehouse API is available

  Scenario: An invited user chooses their own password
    Given I am administering the realm
    And no user "bdd-invitee" exists
    When I invite a user "bdd-invitee" at "bdd-invitee@example.com"
    Then response status should be 201
    And response should contain '"invite_sent":true'
    And gatehouse should have emailed "bdd-invitee@example.com" a "invite" message
    When I log in with username "bdd-invitee" and password "guess"
    Then response status should be 401
    When I accept the invitation emailed to "bdd-invitee@example.com" and choose the password "chosen-secret"
    Then response should be a redirect
    And the redirect location should contain "invited=1"
    When I log in with username "bdd-invitee" and password "chosen-secret"
    Then response status should be 200

  Scenario: An invitation link works once
    Given I am administering the realm
    And no user "bdd-invitee-once" exists
    When I invite a user "bdd-invitee-once" at "bdd-invitee-once@example.com"
    Then response status should be 201
    When I accept the invitation emailed to "bdd-invitee-once@example.com" and choose the password "first-choice"
    Then the redirect location should contain "invited=1"
    When I accept the invitation emailed to "bdd-invitee-once@example.com" and choose the password "second-choice"
    Then the redirect location should contain "ui_login_invite_invalid"

  Scenario: An invitation needs a usable address
    Given I am administering the realm
    And no user "bdd-noaddress" exists
    When I invite a user "bdd-noaddress" at ""
    Then response status should be 400
    When I invite a user "bdd-badaddress" at "not-an-address"
    Then response status should be 400

  Scenario: A username that is markup is refused at sign-up
    When I register as "<img src=x onerror=alert(1)>" with password "secret" and email "markup@example.com"
    Then response should be a redirect
    And the redirect location should contain "err=ui_admin_error_username_invalid"

  Scenario: A service notifies a person who subscribed by default
    When I register as "bdd-notified" with password "secret" and email "bdd-notified@example.com"
    And I follow the verification link emailed to "bdd-notified@example.com"
    And a service notifies "bdd-notified" with "conveyor.run.failed"
    Then response status should be 202
    And response should contain "accepted"
    And gatehouse should have emailed "bdd-notified@example.com" a "notification" message

  Scenario: A kind that is off by default is skipped
    When I register as "bdd-quiet" with password "secret" and email "bdd-quiet@example.com"
    And I follow the verification link emailed to "bdd-quiet@example.com"
    And a service notifies "bdd-quiet" with "conveyor.run.succeeded"
    Then response status should be 200
    And response should contain "not_subscribed"

  Scenario: Unsubscribing from the email itself stops that kind
    When I register as "bdd-unsub" with password "secret" and email "bdd-unsub@example.com"
    And I follow the verification link emailed to "bdd-unsub@example.com"
    And a service notifies "bdd-unsub" with "conveyor.run.failed"
    Then response status should be 202
    When I follow the unsubscribe link emailed to "bdd-unsub@example.com"
    Then response status should be 200
    And response should contain "ui_unsubscribe_done"
    When a service notifies "bdd-unsub" with "conveyor.run.failed"
    Then response status should be 200
    And response should contain "not_subscribed"

  Scenario: A retry with the same key is not sent twice
    When I register as "bdd-retry" with password "secret" and email "bdd-retry@example.com"
    And I follow the verification link emailed to "bdd-retry@example.com"
    And a service notifies "bdd-retry" with "conveyor.run.failed" and the key "run-99"
    Then response status should be 202
    When a service notifies "bdd-retry" with "conveyor.run.failed" and the key "run-99"
    Then response status should be 200
    And response should contain "duplicate"

  Scenario: Nobody is emailed without a confirmed address, and the caller is told why
    When I register as "bdd-unconfirmed" with password "secret" and email "bdd-unconfirmed@example.com"
    And a service notifies "bdd-unconfirmed" with "conveyor.run.failed"
    Then response status should be 200
    And response should contain "no_verified_email"
    When a service notifies "bdd-nobody-at-all" with "conveyor.run.failed"
    Then response status should be 200
    And response should contain "no_such_user"

  Scenario: A malformed notification is refused
    When a service notifies "bdd-notified" with "conveyor.run.exploded"
    Then response status should be 400

  Scenario: An ordinary user cannot send notifications
    Given I am administering the realm
    And a user "bdd-plain" with password "secret" and no permissions
    When I log in with username "bdd-plain" and password "secret"
    And I notify "bdd-notified" with "conveyor.run.failed" using my own token
    Then response status should be 403
