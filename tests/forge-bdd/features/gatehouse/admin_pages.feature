@gatehouse
Feature: Gatehouse user administration pages
  As an administrator
  I want to manage the realm's users from a page
  So that granting and removing access does not need an API client

  # As in user_admin.feature: scenarios run concurrently, so each owns its own
  # `bdd-ui-*` account and clears it first.

  Background:
    Given gatehouse API is available
    And I am administering the realm

  Scenario: The page is offered to an administrator
    Given I am signed in to the realm
    When I open the user administration page
    Then response status should be 200
    And response should contain "ui_admin_users_title"
    And response should contain "ui_admin_create_title"
    And the response should never contain a password hash

  Scenario: The home page offers the realm section to an administrator
    Given I am signed in to the realm
    When I open the home page
    Then response status should be 200
    And response should contain "ui_home_group_realm"

  # The guard: a signed-in non-admin is refused, not bounced to the login form,
  # which would look like their session had expired.
  Scenario: An ordinary user is refused the page
    Given no user "bdd-ui-outsider" exists
    And a user "bdd-ui-outsider" with password "secret" and "read" on "sage"
    And I am signed in to the realm as "bdd-ui-outsider" with password "secret"
    When I open the user administration page
    Then response status should be 403
    And response should contain "ui_admin_forbidden"

  Scenario: An ordinary user is not offered the realm section
    Given no user "bdd-ui-plain" exists
    And a user "bdd-ui-plain" with password "secret" and "read" on "sage"
    And I am signed in to the realm as "bdd-ui-plain" with password "secret"
    When I open the home page
    Then response status should be 200
    And response should not contain "ui_home_group_realm"

  Scenario: Visiting without a session goes to the login form
    When I open the user administration page
    Then response status should be 302
    And the redirect location should contain "/gatehouse/ui/login"

  Scenario: Creating a user through the form lands on their editor
    Given no user "bdd-ui-created" exists
    And I am signed in to the realm
    When I submit the new user form for "bdd-ui-created" with password "secret"
    Then response status should be 302
    And the redirect should report "ok=created"
    When I open the administration page for "bdd-ui-created"
    Then response status should be 200

  # The matrix is built from SERVICE_AUDIENCES, so every service the deployment
  # runs gets a control and nothing else does.
  Scenario: The editor offers one access control per service
    Given no user "bdd-ui-matrix" exists
    And a user "bdd-ui-matrix" with password "secret" and no permissions
    And I am signed in to the realm
    When I open the administration page for "bdd-ui-matrix"
    Then response status should be 200
    And the page should offer an access control for "sage"
    And the page should offer an access control for "warehouse"
    And the page should offer an access control for "switchboard"

  Scenario: A wildcard role is shown rather than hidden
    Given I am signed in to the realm
    When I open the administration page for "admin"
    Then response status should be 200
    And response should contain "ui_admin_wildcard_note"

  Scenario: Granting through the form reaches the next token
    Given no user "bdd-ui-granted" exists
    And a user "bdd-ui-granted" with password "secret" and no permissions
    And I am signed in to the realm
    When I submit the permission form giving "bdd-ui-granted" "write" on "warehouse"
    Then the redirect should report "ok=saved"
    When I log in with username "bdd-ui-granted" and password "secret"
    Then the access token should be valid for "warehouse"
    And the access token should not be valid for "sage"

  Scenario: Deleting through the form returns to the list
    Given no user "bdd-ui-doomed" exists
    And a user "bdd-ui-doomed" with password "secret" and "read" on "sage"
    And I am signed in to the realm
    When I submit the delete form for "bdd-ui-doomed"
    Then response status should be 302
    And the redirect should report "ok=deleted"
    When I open the administration page for "bdd-ui-doomed"
    Then response status should be 302
    And the redirect should report "err=ui_admin_error_not_found"

  # The pages share the API's rules, so this cannot pass while the API's own
  # scenario for the same rule fails.
  Scenario: The form will not let an administrator demote themselves
    Given I am signed in to the realm
    When I submit the form removing my own admin role
    Then response status should be 302
    And the redirect should report "err=ui_admin_error_self_demote"

  Scenario: The form will not let an administrator delete themselves
    Given I am signed in to the realm
    When I submit the delete form for "admin"
    Then response status should be 302
    And the redirect should report "err=ui_admin_error_self_delete"

  # A hand-crafted link cannot put arbitrary text on the page: only keys
  # `RealmError` can produce are rendered.
  Scenario: An unknown error key is not reflected onto the page
    Given I am signed in to the realm
    When I open the user administration page with error "ui_admin_error_fabricated"
    Then response status should be 200
    And response should not contain "ui_admin_error_fabricated"
