@conveyor @authz
Feature: Authorisation holds in conveyor
  As the owner of the estate
  I want a token to open no more than it was given
  So that another service's token, a missing grant or another person's account gets nowhere

  # forge-sectest's `authz` check, acting as several identities minted for the occasion: see
  # `tests/forge-sectest/authz.toml` for the grants and ownership recipes it uses.

  Scenario: No identity gets further than its grants
    Given conveyor API is available
    Then the authorisation of conveyor holds
