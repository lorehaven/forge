@mailroom
Feature: Adding mailboxes from other providers
  A person adds a mailbox at any provider with an address and a password. The login has to work before anything is kept, the
  password is never shown again, nobody else sees the mailbox, and Mailroom will not be talked into dialling something that is
  not a mail server.

  Background:
    Given mailroom API is available

  Scenario: A working login adds the mailbox, and the password is not echoed
    Given I am signed in to mailroom as "boxes-ann" with "mailroom:write"
    When I add my provider mailbox as "Personal"
    Then response status should be 201
    And response should contain "\"label\":\"Personal\""
    And the response should not contain the mailbox password
    When I list my mailboxes
    Then response status should be 200
    And response should contain "Personal"
    And the response should not contain the mailbox password

  Scenario: A wrong password is refused with a sentence that helps and nothing is kept
    Given I am signed in to mailroom as "boxes-bob" with "mailroom:write"
    When I add a mailbox "Wrong" with the password "not-the-password"
    Then response status should be 502
    And response should contain "app password"
    And response should not contain "not-the-password"
    When I list my mailboxes
    Then response should not contain "Wrong"

  Scenario: Somebody else cannot see or remove my mailbox
    Given I am signed in to mailroom as "boxes-carol" with "mailroom:write"
    When I add my provider mailbox as "Carol's"
    Then response status should be 201
    Given I am signed in to mailroom as "boxes-dave" with "mailroom:write"
    When I list my mailboxes
    Then response should not contain "Carol's"

  Scenario: A mailbox can be removed
    Given I am signed in to mailroom as "boxes-erin" with "mailroom:write"
    When I add my provider mailbox as "Temporary"
    And I remove the mailbox "Temporary"
    Then response status should be 204
    When I list my mailboxes
    Then response should not contain "Temporary"

  Scenario Outline: Mailroom does not dial servers that are not on the internet
    Given I am signed in to mailroom as "boxes-mallory" with "mailroom:write"
    When I add a mailbox "Probe" on the server "<host>"
    Then response status should be 502
    And response should contain "cannot be used"

    Examples:
      | host            |
      | 169.254.169.254 |
      | 10.43.0.1       |
      | 192.168.1.1     |
      | 127.0.0.1       |
      | ::1             |

  Scenario: The settings can be tried before they are saved
    Given I am signed in to mailroom as "boxes-frank" with "mailroom:write"
    When I test the settings of a mailbox with the password "bdd-mailbox-app-password"
    Then response status should be 200
    And response should contain "\"ok\":true"
    When I test the settings of a mailbox with the password "wrong"
    Then response should contain "\"ok\":false"

  Scenario: A form with a bad address is refused before any connection
    Given I am signed in to mailroom as "boxes-gina" with "mailroom:write"
    When I add a mailbox "Bad" on the server "not a host"
    Then response status should be 400
