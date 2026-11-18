@mailroom
Feature: Reading mail
  The inbox lists, searches and opens mail; the HTML of a message is shown only inside a cage; nobody opens somebody else's
  message, whatever they guess; the trash is emptied only by the person.

  Background:
    Given mailroom API is available

  Scenario: Mail is searched by what it says
    Given I am signed in to mailroom as "read-search" with "mailroom:write"
    When a service sends "Invoice for March" to "read-search" through mailroom
    And a service sends "Holiday photos" to "read-search" through mailroom
    And I search my inbox for "invoice"
    Then the message list should show 1 message
    When I search my inbox for "nothing like it"
    Then the message list should show 0 messages
    When I search my inbox for "100%"
    Then the message list should show 0 messages

  Scenario: Opening, marking and starring
    Given I am signed in to mailroom as "read-mark" with "mailroom:write"
    When a service sends "Mark me" to "read-mark" through mailroom
    And I open the message "Mark me"
    Then response status should be 200
    When I mark the opened message as read
    And I star the opened message
    Then response status should be 204
    When I list my folders
    Then response should contain "\"unread\":0"

  Scenario: HTML is shown only inside a cage
    Given I am signed in to mailroom as "read-html" with "mailroom:write"
    When a service sends an html message "News" to "read-html" through mailroom
    And I open the message "News"
    Then response should contain "\"has_html\":true"
    And response should contain "https://example.test/unsubscribe"
    When I fetch the html of the opened message
    Then response status should be 200
    And the last answer should have the header "content-security-policy" with "sandbox"
    And the last answer should have the header "content-security-policy" with "default-src 'none'"
    And the last answer should not have the header "content-security-policy" with "allow-scripts"
    And the last answer should have the header "x-content-type-options" with "nosniff"
    And response should contain "Hello"
    And response should not contain "<script"
    And response should not contain "onclick"
    And response should not contain "javascript:"
    And response should not contain "tracker.example"
    When I fetch the html of the opened message with the images allowed
    Then response should contain "tracker.example"
    And the last answer should have the header "content-security-policy" with "https:"

  Scenario: Nobody opens somebody else's message
    Given I am signed in to mailroom as "read-owner" with "mailroom:write"
    When a service sends "Only for the owner" to "read-owner" through mailroom
    And I open the message "Only for the owner"
    And I remember the opened message
    And "read-thief" signs in to mailroom with "mailroom:write"
    And I try to open the remembered message
    Then response status should be 404
    When I try to open the html of the remembered message
    Then response status should be 404
    When I try to move the remembered message to the trash
    Then response status should be 404

  Scenario: The trash is emptied only by the person, and only from the trash
    Given I am signed in to mailroom as "read-trash" with "mailroom:write"
    When a service sends "Throw away" to "read-trash" through mailroom
    And I open the message "Throw away"
    And I delete the opened message for good
    Then response status should be 400
    When I move the opened message to the trash
    Then response status should be 204
    When I list the messages in my trash
    Then the message list should show 1 message
    When I move the opened message back to the inbox
    And I list the messages in my trash
    Then the message list should show 0 messages
    When I move the opened message to the trash
    And I delete the opened message for good
    Then response status should be 204
    When I list the messages in my trash
    Then the message list should show 0 messages

  Scenario: Hostile input in a request is refused cleanly
    Given I am signed in to mailroom as "read-hostile" with "mailroom:write"
    When I list the messages in my spam
    Then response status should be 400
    When I search my inbox for "'; DROP TABLE messages; --"
    Then response status should be 200
    And the message list should show 0 messages
