@switchboard @scan @serial
Feature: switchboard holds up to the generic security checks
  As the owner of the estate
  I want the checks that the nightly runs against the deployed estate to run on every build too
  So that a route left open, a forged token that works or a smuggled request is found before the deploy

  # forge-sectest's auth, tokens, traversal, headers, cors, injection and protocol checks (see `tests/forge-sectest/README.md`). They send
  # hostile requests, hence `@serial`.

  Scenario: Nothing at high or above
    Given switchboard API is available
    Then the generic security checks of switchboard find nothing serious
