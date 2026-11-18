@mailroom
Feature: People write to each other
  Every user has a mailbox, made the first time they open it. Mail goes only to other people of this estate: to somebody who
  has no mailbox here, or outside it, the answer is a sentence saying why, and nothing is half delivered.

  Background:
    Given mailroom API is available

  Scenario: A mailbox has an address and three folders
    Given I am signed in to mailroom as "pia" with "mailroom:write"
    When I open my mailbox
    Then response status should be 200
    And the answer should name the address "pia"
    When I list my folders
    Then response should contain "inbox"
    And response should contain "sent"
    And response should contain "trash"

  Scenario: A message reaches the other person and a copy stays in my sent mail
    Given "paul-rcv" has a mailbox
    And I am signed in to mailroom as "paul-snd" with "mailroom:write"
    When I write "Lunch tomorrow" to "paul-rcv"
    Then response status should be 201
    When I list the messages in my sent
    Then the message list should show 1 message
    When "paul-rcv" signs in to mailroom with "mailroom:write"
    And I list the messages in my inbox
    Then the message list should show 1 message
    And the first message should be from "paul-snd@bdd.example"
    When I open the message "Lunch tomorrow"
    Then response should contain "Text of Lunch tomorrow"

  Scenario: An address with the estate's domain is the same as the name
    Given "addr-rcv" has a mailbox
    And I am signed in to mailroom as "addr-snd" with "mailroom:write"
    When I write "By address" to "addr-rcv@bdd.example"
    Then response status should be 201

  Scenario: Nothing is sent outside the estate
    Given I am signed in to mailroom as "out-snd" with "mailroom:write"
    When I write "Hello" to "somebody@gmail.com"
    Then response status should be 400
    And response should contain "stays inside this estate"

  Scenario: Someone who has no mailbox cannot be written to, and nobody gets half of it
    Given "half-rcv" has a mailbox
    And I am signed in to mailroom as "half-snd" with "mailroom:write"
    When I write "Hello" to "nobody-has-this-name"
    Then response status should be 400
    And response should contain "has no mailbox here"
    When I write "Both" to "half-rcv" with a copy to "nobody-has-this-name"
    Then response status should be 400
    When "half-rcv" signs in to mailroom with "mailroom:write"
    And I list the messages in my inbox
    Then the message list should show 0 messages

  Scenario: A blind copy arrives without being named
    Given "bcc-to" has a mailbox
    And "bcc-hidden" has a mailbox
    And I am signed in to mailroom as "bcc-snd" with "mailroom:write"
    When I write "Quiet one" to "bcc-to" with a blind copy to "bcc-hidden"
    Then response status should be 201
    When "bcc-hidden" signs in to mailroom with "mailroom:write"
    And I list the messages in my inbox
    Then the message list should show 1 message
    And response should not contain "bcc-hidden@"

  Scenario: A reply is threaded and marks the original answered
    Given "reply-b" has a mailbox
    And I am signed in to mailroom as "reply-a" with "mailroom:write"
    When I write "Plan" to "reply-b"
    And "reply-b" signs in to mailroom with "mailroom:write"
    And I open the message "Plan"
    And I reply "Re: Plan" to the opened message
    Then response status should be 201
    When I list the messages in my inbox
    Then response should contain "\"answered\":true"
    When "reply-a" signs in to mailroom with "mailroom:write"
    And I list the messages in my inbox
    Then the message list should show 1 message

  Scenario: People are found by the start of their name
    Given "find-ann" has a mailbox
    And "find-anna" has a mailbox
    And I am signed in to mailroom as "find-me" with "mailroom:write"
    When I look for people whose name starts with "find-an"
    Then response should contain "find-ann@bdd.example"
    And response should contain "find-anna@bdd.example"
    And response should not contain "find-me@"

  Scenario: A line break in the subject or a recipient cannot add a hidden recipient
    Given "inj-rcv" has a mailbox
    And I am signed in to mailroom as "inj-snd" with "mailroom:write"
    When I write to "inj-rcv" with a line break in the subject
    Then response status should be 400
    When I write to "inj-rcv" with a line break in a recipient
    Then response status should be 400
