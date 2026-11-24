@gatehouse @fuzz @serial
Feature: Gatehouse under forge-sectest's fuzz check
  As the owner of the realm
  I want the service that signs everyone in to be held to the same checks as the rest
  So that what it does with a hostile or forged request is a refusal and nothing else

  # See `tests/forge-sectest/README.md`. The fuzzer works on one throwaway account, `sectest-victim`, never on `admin`.

  Scenario: The check finds nothing wrong
    Given gatehouse API is available
    Then the fuzzing of gatehouse finds nothing wrong
