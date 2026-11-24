@gatehouse @security
Feature: Gatehouse resists attacks on its sign-in flows
  As the owner of the realm
  I want the ways a sign-in is usually attacked to get nowhere
  So that a planted session, a stolen token, a stopwatch or a poisoned email does not become someone's account

  # Each scenario owns one `atk-*` account (they run concurrently against a shared realm) and passes when the defence
  # holds. The functional scenarios in the other files show that sign-in works; these ask what an attacker would.

  Background:
    Given gatehouse API is available
    And I am administering the realm

  Scenario: A session id chosen by the client is not adopted at sign-in
    Given no user "atk-fixation" exists
    And a user "atk-fixation" with password "Atk-fixation-1" and no permissions
    Then gatehouse does not adopt a session id the client chose for "atk-fixation" with password "Atk-fixation-1"

  Scenario: A spent refresh token yields nothing new when replayed
    Given no user "atk-family" exists
    And a user "atk-family" with password "Atk-family-1" and no permissions
    Then a spent refresh token yields nothing new when replayed for "atk-family" with password "Atk-family-1"

  Scenario: An unknown user is answered like a wrong password
    Given no user "atk-enum" exists
    And a user "atk-enum" with password "Atk-enum-1" and no permissions
    Then gatehouse answers an unknown user like a wrong password for "atk-enum"

  Scenario: Other spellings of a name are not a way round the lockout
    Given no user "atk-spelling" exists
    And a user "atk-spelling" with password "Atk-spelling-1" and no permissions
    Then gatehouse counts failed logins across spellings of "atk-spelling" with password "Atk-spelling-1"

  Scenario: A password reset ends what was signed in and the link works once
    Given no user "atk-reset" exists
    When I register as "atk-reset" with password "Atk-reset-old-1" and email "atk-reset@example.com"
    And I follow the verification link emailed to "atk-reset@example.com"
    Then resetting the password of "atk-reset" at "atk-reset@example.com" ends what was signed in and spends the link

  Scenario: A reset email names gatehouse, whatever Host the request claimed
    Given no user "atk-host" exists
    When I register as "atk-host" with password "Atk-host-pass-1" and email "atk-host@example.com"
    And I follow the verification link emailed to "atk-host@example.com"
    Then the reset email for "atk-host" at "atk-host@example.com" names gatehouse, whatever Host the request claimed

  Scenario: One guess at each of many names from one address is throttled
    Given no user "atk-spray" exists
    And a user "atk-spray" with password "Atk-spray-1" and no permissions
    Then gatehouse turns away an address that keeps failing to sign in, and signs "atk-spray" in with "Atk-spray-1" from another
