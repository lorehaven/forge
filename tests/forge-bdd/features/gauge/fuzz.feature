@gauge @fuzz @serial
Feature: gauge survives requests that are wrong in one place
  As the owner of the estate
  I want a malformed, hostile or oversized request to be refused, not to crash the service or leak
  So that what gauge does with input it did not expect is a 4xx and nothing else

  # forge-sectest's fuzzer, driven by the service's own OpenAPI document (`docs/services/gauge/openapi.yaml`): it builds a
  # valid request from each schema and changes one thing at a time. It writes to the service, hence `@serial`.

  Scenario: Every operation answers a mutated request with a refusal
    Given gauge API is available
    Then the fuzzing of gauge finds nothing wrong
