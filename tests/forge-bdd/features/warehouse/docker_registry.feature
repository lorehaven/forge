@warehouse
Feature: Docker Registry v2 authentication
  As a container client
  I want the registry to run the standard bearer-token handshake
  So that `docker login` knows where to get a token and nothing is pulled anonymously

  # The `/v2` scope lives at the server root, outside BASE_PATH, because the
  # distribution spec fixes it there. `WarehouseAuth` (src/middleware/auth.rs)
  # guards every `/v2/*` path when docker auth is enabled: no bearer means a 401
  # carrying `WWW-Authenticate: Bearer realm=...`, which is how a client is told
  # to go and fetch a token. The authenticated push/pull paths are in
  # docker.feature and in docker/warehouse-service's own tests.

  Background:
    Given warehouse API is available

  Scenario: The version check demands a token and says where to get one
    When GET request is sent to "/v2/"
    Then response status should be 401
    And response header "WWW-Authenticate" should contain "Bearer realm="

  Scenario: A HEAD of the version check is challenged the same way
    When HEAD request is sent to "/v2/"
    Then response status should be 401

  Scenario: The catalog is not listable without a token
    When GET request is sent to "/v2/_catalog"
    Then response status should be 401

  # Auth is checked before the repository is resolved, so an anonymous probe
  # cannot tell a real repository from a missing one.
  Scenario: Listing tags without a token is refused, not answered with a 404
    When GET request is sent to "/v2/nobody/does-not-exist/tags/list"
    Then response status should be 401

  Scenario: A manifest HEAD without a token is refused
    When HEAD request is sent to "/v2/nobody/does-not-exist/manifests/latest"
    Then response status should be 401

  Scenario: Warehouse health is reported without a token
    When GET request is sent to "/warehouse/health"
    Then response status should be 200
