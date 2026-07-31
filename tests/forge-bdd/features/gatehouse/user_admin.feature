@gatehouse
Feature: Gatehouse user administration
  As the estate's identity service
  I want administrators to manage users, roles and permissions
  So that access to each service can be granted and taken away

  # Scenarios run concurrently, so each one owns a differently named account and
  # clears it first. Sharing one fixture name here means one scenario's login
  # reads the row another has just rewritten.

  Background:
    Given gatehouse API is available
    And I am administering the realm

  Scenario: An administrator can create a user and list the realm
    Given no user "bdd-created" exists
    When I create a user "bdd-created" with password "secret" and "read" on "sage"
    Then response status should be 201
    And the response should never contain a password hash
    When I list the realm's users
    Then response status should be 200
    And response should contain "bdd-created"
    And the response should never contain a password hash

  # The point of narrowing the audience list: the relying party's own audience
  # check is what refuses the request, with no permission logic in the service.
  Scenario: A grant decides which services a token is valid for
    Given no user "bdd-audience" exists
    And a user "bdd-audience" with password "secret" and "read" on "sage"
    When I log in with username "bdd-audience" and password "secret"
    Then response status should be 200
    And the access token should be valid for "sage"
    And the access token should be valid for "gatehouse"
    And the access token should not be valid for "switchboard"
    And the access token should not be valid for "warehouse"

  Scenario: A user with no grants can still reach gatehouse and nothing else
    Given no user "bdd-ungranted" exists
    And a user "bdd-ungranted" with password "secret" and no permissions
    When I log in with username "bdd-ungranted" and password "secret"
    Then response status should be 200
    And the access token should be valid for "gatehouse"
    And the access token should not be valid for "sage"

  Scenario: The scope claim carries the grant alongside the role
    Given no user "bdd-scope" exists
    And a user "bdd-scope" with password "secret" and "write" on "sage"
    When I log in with username "bdd-scope" and password "secret"
    Then the access token scope should be "user sage:write"

  # The wildcard: an admin's token enumerates nothing and reaches everything.
  Scenario: An admin token names the role rather than every grant
    When I log in with username "admin" and password "password"
    Then the access token scope should be "admin"
    And the access token should be valid for "sage"
    And the access token should be valid for "switchboard"
    And the access token should be valid for "warehouse"

  Scenario: Managing users is refused to an ordinary user and allowed to an admin
    Given no user "bdd-promoted" exists
    And a user "bdd-promoted" with password "secret" and "read" on "sage"
    When I log in with username "bdd-promoted" and password "secret"
    And I list the realm's users with my own token
    Then response status should be 403
    When I make "bdd-promoted" an admin
    Then response status should be 200
    When I log in with username "bdd-promoted" and password "secret"
    And I list the realm's users with my own token
    Then response status should be 200

  Scenario: Granting access takes effect on the next login
    Given no user "bdd-granted-later" exists
    And a user "bdd-granted-later" with password "secret" and no permissions
    When I grant "write" on "warehouse" to "bdd-granted-later"
    Then response status should be 200
    When I log in with username "bdd-granted-later" and password "secret"
    Then the access token should be valid for "warehouse"
    And the access token should not be valid for "sage"

  Scenario: Applying a template grants exactly what it lists
    Given no user "bdd-templated" exists
    And a user "bdd-templated" with password "secret" and no permissions
    When I apply the "editor" template to "bdd-templated"
    Then response status should be 200
    When I log in with username "bdd-templated" and password "secret"
    And I ask what I may do
    Then response status should be 200
    And the response should report "write" on "sage"
    And the response should report "launch" on "switchboard"

  Scenario: Applying an unknown template is rejected
    Given no user "bdd-bad-template" exists
    And a user "bdd-bad-template" with password "secret" and no permissions
    When I apply the "not-a-template" template to "bdd-bad-template"
    Then response status should be 404

  # gatehouse administers itself through the same catalog it hands to every
  # other service (`permissions.toml`'s `[services.gatehouse]`), so a
  # "user-manager" doing all of this needs no admin role at all - that
  # delegation, and its one deliberate limit, is what these four prove.

  Scenario: A user-manager can create, grant and delete users without holding admin
    Given no user "bdd-manager" exists
    And a user "bdd-manager" with password "secret" and no permissions
    When I apply the "user-manager" template to "bdd-manager"
    Then response status should be 200
    When I log in with username "bdd-manager" and password "secret"
    Then response status should be 200
    When I create a user "bdd-managed" with password "secret" and no permissions using my own token
    Then response status should be 201
    When I grant "read" on "sage" to "bdd-managed" using my own token
    Then response status should be 200
    When I delete "bdd-managed" using my own token
    Then response status should be 204

  Scenario: A user-manager cannot create an admin
    Given no user "bdd-manager-noescalate" exists
    And a user "bdd-manager-noescalate" with password "secret" and no permissions
    When I apply the "user-manager" template to "bdd-manager-noescalate"
    Then response status should be 200
    When I log in with username "bdd-manager-noescalate" and password "secret"
    Then response status should be 200
    When I create a user "bdd-escalation-attempt" with password "secret" and role "admin" using my own token
    Then response status should be 403

  Scenario: A user-manager cannot promote an existing user to admin
    Given no user "bdd-manager-nopromote" exists
    And a user "bdd-manager-nopromote" with password "secret" and no permissions
    When I apply the "user-manager" template to "bdd-manager-nopromote"
    Then response status should be 200
    Given no user "bdd-promotion-target" exists
    And a user "bdd-promotion-target" with password "secret" and no permissions
    When I log in with username "bdd-manager-nopromote" and password "secret"
    Then response status should be 200
    When I make "bdd-promotion-target" an admin using my own token
    Then response status should be 403

  Scenario: Holding only read-users is not enough to create a user
    Given no user "bdd-viewer" exists
    And a user "bdd-viewer" with password "secret" and "read-users" on "gatehouse"
    When I log in with username "bdd-viewer" and password "secret"
    Then response status should be 200
    When I create a user "bdd-should-not-exist" with password "secret" and no permissions using my own token
    Then response status should be 403

  Scenario: A user can ask what they may do
    Given no user "bdd-asking" exists
    And a user "bdd-asking" with password "secret" and "read" on "sage"
    When I log in with username "bdd-asking" and password "secret"
    And I ask what I may do
    Then response status should be 200
    And the response should report "read" on "sage"

  Scenario: A grant naming a service this deployment does not run is rejected
    Given no user "bdd-bad-service" exists
    When I create a user "bdd-bad-service" with password "secret" and "read" on "not-a-service"
    Then response status should be 400

  Scenario: Creating the same user twice is refused
    Given no user "bdd-duplicate" exists
    And a user "bdd-duplicate" with password "secret" and "read" on "sage"
    When I create a user "bdd-duplicate" with password "secret" and "read" on "sage"
    Then response status should be 409

  # Without these two rules a single mistaken edit locks everybody out of the
  # estate, recoverable only by SQL.
  Scenario: An administrator cannot remove their own admin role
    When I remove my own admin role
    Then response status should be 409
    When I list the realm's users
    Then response status should be 200

  Scenario: An administrator cannot delete their own account
    When I delete my own account
    Then response status should be 409
