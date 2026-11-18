@mailroom
Feature: Sending from a mailbox
  A message written in Mailroom goes out through the person's own mailbox at their provider, logged in as them, and a copy is
  kept in Sent. A reply is threaded.

  Background:
    Given mailroom API is available

  Scenario: A message goes out as the person, and a copy is kept
    Given I am signed in to mailroom as "sending-ann" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I send "Dinner on friday" to "friend@example.org" from the mailbox "Main"
    Then response status should be 201
    And the sending server should receive a message to "friend@example.org" with the subject "Dinner on friday"
    And the message to "friend@example.org" with the subject "Dinner on friday" should be sent as "me@provider.example"
    And the message to "friend@example.org" with the subject "Dinner on friday" should be logged in as "me@provider.example"
    And the message to "friend@example.org" with the subject "Dinner on friday" should contain "Text of Dinner on friday"
    And the provider folder "Sent" should hold a message with the subject "Dinner on friday"

  Scenario: A reply is threaded to the message it answers
    Given the provider mailbox holds a message "Where shall we meet" from "News <news@provider.example>"
    And I am signed in to mailroom as "sending-bob" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I open the message "Where shall we meet"
    And I reply "Re: Where shall we meet" to the opened message from the mailbox "Main"
    Then response status should be 201
    And the message to "news@provider.example" with the subject "Re: Where shall we meet" should have the header "In-Reply-To"
    And the message to "news@provider.example" with the subject "Re: Where shall we meet" should have the header "References"

  Scenario: A message with a line break in the subject is refused, not sent with a hidden recipient
    Given I am signed in to mailroom as "sending-carol" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I send a message to "friend@example.org" from the mailbox "Main" whose subject has a line break
    Then response status should be 400
    When I send a message to "friend@example.org" and "other@example.org" from the mailbox "Main" with a line break in an address
    Then response status should be 400

  Scenario: Only the person's own mailboxes can be sent from
    Given I am signed in to mailroom as "sending-dave" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I remember the mailbox "Main"
    Given I am signed in to mailroom as "sending-mallory" with "mailroom:write"
    When I try to send from the remembered mailbox
    Then response status should be 404
