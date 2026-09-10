@gatehouse
Feature: Signing keys and JWKS
  As the estate's identity service
  I want my public keys served and rotatable
  So that every relying party can verify a token and a leaked key can be retired

  Background:
    Given gatehouse API is available

  # Every service's `JwksVerifier` polls this, unauthenticated, and the path is
  # fixed by RFC 7517. If it stops serving a usable key, the whole estate stops
  # trusting logins.
  Scenario: The JWKS is served without a token
    When I fetch the realm JWKS
    Then response status should be 200
    And response should contain "keys"
    And response should contain "kty"
    And response should contain "kid"

  Scenario: Rotation is refused without a token
    When I rotate the signing keys with no token
    Then response status should be 401

  Scenario: Rotation is refused to an ordinary user
    Given I am administering the realm
    And no user "bdd-keys-plain" exists
    And a user "bdd-keys-plain" with password "secret" and "read" on "sage"
    When I log in with username "bdd-keys-plain" and password "secret"
    And I rotate the signing keys with my own token
    Then response status should be 403

  Scenario: An administrator can rotate, and the JWKS still verifies afterwards
    Given I am administering the realm
    When I rotate the signing keys as an administrator
    Then response status should be 204
    When I fetch the realm JWKS
    Then response status should be 200
    And response should contain "kty"
