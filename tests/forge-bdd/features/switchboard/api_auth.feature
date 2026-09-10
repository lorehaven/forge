@switchboard
Feature: API authentication
  As the estate's identity service
  I want every switchboard API route to need a realm token
  So that GPU state and vLLM control are not readable or reachable anonymously

  # The other switchboard features authenticate in their Background, so this is
  # the one place the bare "no token at all" path is exercised. Switchboard has
  # no UserDb of its own - identity is federated through gatehouse-issued
  # tokens - so with SERVICE_AUTH_ENABLED=true a request with no bearer is
  # refused by the middleware before any handler runs.

  Background:
    Given switchboard API is available

  Scenario Outline: A GET API route needs a token
    When an unauthenticated GET request is sent to "<path>"
    Then the response status should be 401

    Examples:
      | path                      |
      | /api/v1/gpu/status        |
      | /api/v1/vllm/list         |
      | /api/v1/vllm/instances    |
      | /api/v1/models/running    |

  Scenario: Listing models needs a token
    When an unauthenticated POST request is sent to "/api/v1/models/list"
    Then the response status should be 401

  Scenario: Launching an instance needs a token
    When an unauthenticated POST request is sent to "/api/v1/vllm/instances"
    Then the response status should be 401
