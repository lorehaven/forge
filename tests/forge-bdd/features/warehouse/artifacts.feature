@warehouse
Feature: Artifacts API permission enforcement
  As the estate's realm
  I want publishing an artifact to need a write grant, and browsing only a valid identity
  So that a `read` grant can see what is hosted but not overwrite it

  # `routers::artifacts::scope` mounts `RequireWrite` + `Auth`. `RequireWrite`
  # gates the mutating verbs only (PUT/DELETE), so the read/write split here
  # lands on publish, not on the catalog GET - the same shape files.feature
  # proves for the files API. The `/api/v1/apk` alias shares the middleware.

  Background:
    Given warehouse API is available

  Scenario: No token cannot list the catalog
    Given I hold no token
    When I request the artifacts catalog
    Then response status should be 401

  Scenario: A read grant can browse the catalog
    Given I hold a token scoped "user warehouse:read"
    When I request the artifacts catalog
    Then response status should be 200

  Scenario: A read grant cannot publish
    Given I hold a token scoped "user warehouse:read"
    When I publish artifact "demo/linux/1.0.0"
    Then response status should be 403

  # A write grant gets past RequireWrite; the request then fails on its body
  # rather than on permission, so the interesting thing is that it is not a 403.
  Scenario: A write grant gets past the permission gate
    Given I hold a token scoped "user warehouse:write"
    When I publish artifact "demo/linux/1.0.0"
    Then the response status should not be 403

  Scenario: A write grant gets an honest 404 for an artifact that was never published
    Given I hold a token scoped "user warehouse:write"
    When I request artifact metadata for "nothing/linux/1.0.0"
    Then response status should be 404

  # The token's audience is warehouse, but a scope that names another service
  # carries no warehouse write action - so publish is still refused.
  Scenario: A write action for another service does not authorise a publish here
    Given I hold a token scoped "user sage:write"
    When I publish artifact "demo/linux/1.0.0"
    Then response status should be 403

  Scenario: The apk alias enforces the same rule
    Given I hold no token
    When I request the apk catalog
    Then response status should be 401
