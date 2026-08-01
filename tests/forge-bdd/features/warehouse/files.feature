@warehouse
Feature: Files API permission enforcement
  As the estate's realm
  I want a `read` grant to mean something narrower than `write`
  So that a permission level is not just decoration

  # `RequireWrite`, mounted on `routers::files::scope`. Upload and delete are
  # PUT/DELETE; download, head and list are all GET, so the method-shape rule
  # needs no exceptions here.

  Background:
    Given warehouse API is available

  Scenario: No token at all is rejected before any permission is considered
    Given I hold no token
    When I upload "no-token.txt" to the test storage
    Then response status should be 401

  Scenario: A read grant can download and list, but not write
    Given I hold a token scoped "user warehouse:read"
    When I upload "blocked.txt" to the test storage
    Then response status should be 403
    When I list the test storage
    Then response status should be 200
    When I download "does-not-exist.txt" from the test storage
    Then response status should be 404

  Scenario: A write grant can upload, and the upload is readable back
    Given I hold a token scoped "user warehouse:write"
    When I upload "written.txt" to the test storage
    Then response status should be 201
    And the test storage should contain "written.txt"

  Scenario: A write grant can delete what it uploaded
    Given I hold a token scoped "user warehouse:write"
    When I upload "to-delete.txt" to the test storage
    Then response status should be 201
    When I delete "to-delete.txt" from the test storage
    Then response status should be 204
    When I download "to-delete.txt" from the test storage
    Then response status should be 404

  Scenario: A read grant cannot delete
    Given I hold a token scoped "user warehouse:write"
    When I upload "protected.txt" to the test storage
    Then response status should be 201
    Given I hold a token scoped "user warehouse:read"
    When I delete "protected.txt" from the test storage
    Then response status should be 403
    And the test storage should contain "protected.txt"

  # A grant on the wrong service must not satisfy this one - the check is
  # `{SERVICE_NAME}:write`, not "holds a write grant on anything".
  Scenario: A write grant on a different service does not count here
    Given I hold a token scoped "user sage:write"
    When I upload "wrong-service.txt" to the test storage
    Then response status should be 403

  # The wildcard: admin and service accounts need no enumerated grant.
  Scenario: A wildcard role writes without an enumerated grant
    Given I hold a token scoped "admin"
    When I upload "admin-written.txt" to the test storage
    Then response status should be 201
    And the test storage should contain "admin-written.txt"
