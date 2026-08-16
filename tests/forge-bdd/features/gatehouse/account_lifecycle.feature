@gatehouse
Feature: Account lifecycle and multi-factor authentication
  As an administrator or a signed-in user
  I want disabled and locked accounts rejected, repeated failures to lock an
  account, and an optional second factor on login
  So that a compromised or misused account can be contained

  # Each scenario owns its own `bdd-lifecycle-*` account, as the admin pages
  # scenarios do, so the suite can run concurrently against a shared realm.

  Background:
    Given gatehouse API is available
    And I am administering the realm
    And I am signed in to the realm

  Scenario: A disabled account cannot log in
    Given no user "bdd-lifecycle-disabled" exists
    And a user "bdd-lifecycle-disabled" with password "secret" and no permissions
    When I submit the disable form for "bdd-lifecycle-disabled"
    Then the redirect should report "ok=saved"
    When I log in with username "bdd-lifecycle-disabled" and password "secret"
    Then response status should be 401
    And the response body should report "account_disabled"

  Scenario: Re-enabling a disabled account restores login
    Given no user "bdd-lifecycle-reenabled" exists
    And a user "bdd-lifecycle-reenabled" with password "secret" and no permissions
    And I submit the disable form for "bdd-lifecycle-reenabled"
    When I submit the enable form for "bdd-lifecycle-reenabled"
    Then the redirect should report "ok=saved"
    When I log in with username "bdd-lifecycle-reenabled" and password "secret"
    Then response status should be 200

  Scenario: An administrator cannot disable their own account
    When I submit the disable form for myself
    Then the redirect should report "err=ui_admin_error_self_disable"

  Scenario: Repeated failed logins lock the account
    Given no user "bdd-lifecycle-lockout" exists
    And a user "bdd-lifecycle-lockout" with password "secret" and no permissions
    When I attempt to log in with username "bdd-lifecycle-lockout" and the wrong password 5 times
    And I log in with username "bdd-lifecycle-lockout" and password "secret"
    Then response status should be 401
    And the response body should report "account_locked"

  Scenario: An admin can unlock a locked account
    Given no user "bdd-lifecycle-unlock" exists
    And a user "bdd-lifecycle-unlock" with password "secret" and no permissions
    And I attempt to log in with username "bdd-lifecycle-unlock" and the wrong password 5 times
    When I submit the unlock form for "bdd-lifecycle-unlock"
    Then the redirect should report "ok=saved"
    When I log in with username "bdd-lifecycle-unlock" and password "secret"
    Then response status should be 200

  Scenario: Enrolling MFA and completing a challenged login
    Given no user "bdd-lifecycle-mfa" exists
    And a user "bdd-lifecycle-mfa" with password "secret" and no permissions
    And I am signed in to the realm as "bdd-lifecycle-mfa" with password "secret"
    When I enroll two-factor authentication
    Then response should be a redirect
    And the redirect location should contain "ok=mfa_enabled"
    When I submit the login form with username "bdd-lifecycle-mfa" and password "secret"
    Then response should be a redirect
    And the redirect location should contain "/login/mfa"
    And no realm session cookie should be set
    When I complete the MFA challenge with a valid code
    Then response should be a redirect
    And a realm session cookie should be set

  Scenario: A wrong MFA code does not complete the login
    Given no user "bdd-lifecycle-mfa-wrong" exists
    And a user "bdd-lifecycle-mfa-wrong" with password "secret" and no permissions
    And I am signed in to the realm as "bdd-lifecycle-mfa-wrong" with password "secret"
    And I enroll two-factor authentication
    When I submit the login form with username "bdd-lifecycle-mfa-wrong" and password "secret"
    And I complete the MFA challenge with code "000000"
    Then response should be a redirect
    And the redirect location should contain "/login/mfa"
    And no realm session cookie should be set

  Scenario: An admin can force-disable a lost authenticator
    Given no user "bdd-lifecycle-mfa-recovery" exists
    And a user "bdd-lifecycle-mfa-recovery" with password "secret" and no permissions
    And I am signed in to the realm as "bdd-lifecycle-mfa-recovery" with password "secret"
    And I enroll two-factor authentication
    When I submit the force-disable MFA form for "bdd-lifecycle-mfa-recovery"
    Then the redirect should report "ok=saved"
    When I submit the login form with username "bdd-lifecycle-mfa-recovery" and password "secret"
    Then response should be a redirect
    And a realm session cookie should be set
