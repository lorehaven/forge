@gatehouse
Feature: Gatehouse authentication API
  As the estate's identity service
  I want to issue and revoke realm tokens
  So that every service can trust one login

  Background:
    Given gatehouse API is available

  Scenario: Login with valid credentials issues a realm token
    When I log in with username "admin" and password "password"
    Then response status should be 200
    And the access token should be valid for "sage"
    And the access token should be valid for "warehouse"
    And the access token should carry a session id

  Scenario: Login with the wrong password is rejected
    When I log in with username "admin" and password "wrong-password"
    Then response status should be 401

  Scenario: Login with an unknown user is rejected
    When I log in with username "nobody" and password "password"
    Then response status should be 401

  Scenario: Userinfo reports the subject behind a token
    Given I am logged in as "admin"
    When I request userinfo with the access token
    Then response status should be 200
    And response should contain "admin"

  Scenario: Userinfo rejects a missing token
    When I request userinfo without a token
    Then response status should be 401

  Scenario: Refreshing rotates the refresh token
    Given I am logged in as "admin"
    When I refresh the session
    Then response status should be 200
    And the refresh token should have changed

  Scenario: A rotated refresh token cannot be reused
    Given I am logged in as "admin"
    When I refresh the session
    And I refresh with the previous refresh token
    Then response status should be 401


  Scenario: Logout revokes the session everywhere
    Given I am logged in as "admin"
    When I log out
    Then response status should be 204
    When I request userinfo with the access token
    Then response status should be 401
