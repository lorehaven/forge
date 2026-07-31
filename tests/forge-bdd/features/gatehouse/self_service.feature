@gatehouse
Feature: Self-service registration and password reset
  As a new or locked-out user
  I want to create an account and recover a forgotten password myself
  So that an administrator does not have to do it for me

  # `LoggingSender` writes the verification/reset link to gatehouse's own log
  # instead of anywhere the recipient would see it - this is dev-only, and it
  # is exactly what lets these scenarios read the link back.

  Background:
    Given gatehouse API is available

  Scenario: Registering creates an account with the catalog's default grants
    When I register as "bdd-registrant" with password "secret" and email "bdd-registrant@example.com"
    Then response should be a redirect
    And the redirect location should contain "registered=1"
    When I log in with username "bdd-registrant" and password "secret"
    Then response status should be 200
    And the access token should be valid for "sage"
    And the access token should be valid for "warehouse"
    And the access token should be valid for "switchboard"
    And the access token should be valid for "conveyor"

  Scenario: Following the verification link marks the address verified
    When I register as "bdd-verifying" with password "secret" and email "bdd-verifying@example.com"
    Then response should be a redirect
    When I follow the verification link emailed to "bdd-verifying@example.com"
    Then response should be a redirect
    And the redirect location should contain "verified=1"

  Scenario: A verification link only works once
    When I register as "bdd-reverifying" with password "secret" and email "bdd-reverifying@example.com"
    Then response should be a redirect
    When I follow the verification link emailed to "bdd-reverifying@example.com"
    Then response should be a redirect
    And the redirect location should contain "verified=1"
    When I follow the verification link emailed to "bdd-reverifying@example.com"
    Then response should be a redirect
    And the redirect location should contain "err=ui_login_verify_invalid"

  Scenario: Resetting a password by email takes effect on the next login
    When I register as "bdd-forgetful" with password "old-secret" and email "bdd-forgetful@example.com"
    Then response should be a redirect
    When I request a password reset for "bdd-forgetful"
    Then response should be a redirect
    And the redirect location should contain "reset_requested=1"
    When I follow the password reset link emailed to "bdd-forgetful@example.com" and set the password to "new-secret"
    Then response should be a redirect
    And the redirect location should contain "reset=1"
    When I log in with username "bdd-forgetful" and password "old-secret"
    Then response status should be 401
    When I log in with username "bdd-forgetful" and password "new-secret"
    Then response status should be 200

  Scenario: Requesting a reset for a user with no email on file is silent either way
    Given I am administering the realm
    And no user "bdd-noemail" exists
    And a user "bdd-noemail" with password "secret" and no permissions
    When I request a password reset for "bdd-noemail"
    Then response should be a redirect
    And the redirect location should contain "reset_requested=1"

  Scenario: Requesting a reset for an unknown username looks the same as a real one
    When I request a password reset for "nobody-has-this-username"
    Then response should be a redirect
    And the redirect location should contain "reset_requested=1"
