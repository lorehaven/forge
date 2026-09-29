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
    When I follow the verification link emailed to "bdd-registrant@example.com"
    Then response should be a redirect
    When I log in with username "bdd-registrant" and password "secret"
    Then response status should be 200
    And the access token should be valid for "sage"
    And the access token should be valid for "warehouse"
    And the access token should be valid for "switchboard"
    And the access token should be valid for "conveyor"

  Scenario: An unverified address cannot log in until the link is followed
    When I register as "bdd-unverified" with password "secret" and email "bdd-unverified@example.com"
    Then response should be a redirect
    When I log in with username "bdd-unverified" and password "secret"
    Then response status should be 401
    And response should contain "email_unverified"
    When I log in with username "bdd-unverified" and password "wrong-password"
    Then response status should be 401
    And response should not contain "email_unverified"
    When I follow the verification link emailed to "bdd-unverified@example.com"
    Then response should be a redirect
    When I log in with username "bdd-unverified" and password "secret"
    Then response status should be 200

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
    When I follow the verification link emailed to "bdd-forgetful@example.com"
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

  Scenario: A lost verification email can be requested again
    When I register as "bdd-resender" with password "secret" and email "bdd-resender@example.com"
    Then response should be a redirect
    When I request a new verification email for "bdd-resender"
    Then response should be a redirect
    And the redirect location should contain "resend_requested=1"
    When I follow the verification link emailed to "bdd-resender@example.com"
    Then response should be a redirect
    And the redirect location should contain "verified=1"
    When I log in with username "bdd-resender" and password "secret"
    Then response status should be 200

  Scenario: Asking for another verification email straight away is refused
    When I register as "bdd-impatient" with password "secret" and email "bdd-impatient@example.com"
    Then response should be a redirect
    When I request a new verification email for "bdd-impatient"
    Then response should be a redirect
    And the redirect location should contain "resend_requested=1"
    When I request a new verification email for "bdd-impatient"
    Then response should be a redirect
    And the redirect location should contain "err=ui_login_rate_limited"

  Scenario: Requesting a verification email for an unknown user looks the same as a real one
    When I request a new verification email for "nobody-has-this-username-either"
    Then response should be a redirect
    And the redirect location should contain "resend_requested=1"

  Scenario: Asking for a password reset over and over is refused
    When I register as "bdd-persistent" with password "secret" and email "bdd-persistent@example.com"
    Then response should be a redirect
    When I request a password reset for "bdd-persistent"
    And I request a password reset for "bdd-persistent"
    And I request a password reset for "bdd-persistent"
    Then the redirect location should contain "reset_requested=1"
    When I request a password reset for "bdd-persistent"
    Then response should be a redirect
    And the redirect location should contain "err=ui_login_rate_limited"

  Scenario: Registering the same address again and again is refused
    When I register as "bdd-flood-1" with password "secret" and email "bdd-flood@example.com"
    And I register as "bdd-flood-2" with password "secret" and email "bdd-flood@example.com"
    And I register as "bdd-flood-3" with password "secret" and email "bdd-flood@example.com"
    Then the redirect location should contain "registered=1"
    When I register as "bdd-flood-4" with password "secret" and email "bdd-flood@example.com"
    Then response should be a redirect
    And the redirect location should contain "err=ui_register_error_rate_limited"

