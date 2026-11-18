@mailroom
Feature: Reading mail
  Mailroom keeps a cache of each mailbox's headers and fetches a message when it is opened. What it shows of a stranger's HTML
  is only what is safe: no script, no tracking pictures unless asked, no link that runs code, and attachments are only ever
  downloaded.

  Background:
    Given mailroom API is available

  Scenario: Mail at the provider appears in the inbox and can be searched
    Given the provider mailbox holds a message "Quarterly zeppelin schedule" from "Alice <alice@provider.example>"
    And I am signed in to mailroom as "reading-ann" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I list my inbox
    Then response should contain "Quarterly zeppelin schedule"
    And response should contain "alice@provider.example"
    When I search my inbox for "zeppelin"
    Then the message list should show 1 message
    When I search my inbox for "nothing-matches-this"
    Then the message list should show 0 messages

  Scenario: Opening a message shows its text and marks nothing until asked
    Given the provider mailbox holds a message "Open me please" from "Alice <alice@provider.example>"
    And I am signed in to mailroom as "reading-bob" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I open the message "Open me please"
    Then response status should be 200
    And response should contain "Body of Open me please"
    When I mark the opened message as read
    Then response status should be 204
    And the provider message "Open me please" should be flagged "\Seen"

  Scenario: A message moved to the trash is moved at the provider too
    Given the provider mailbox holds a message "Throw this away" from "Spam <spam@provider.example>"
    And I am signed in to mailroom as "reading-carol" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I open the message "Throw this away"
    And I move the opened message to the trash
    Then response status should be 204
    And the provider folder "Trash" should hold a message with the subject "Throw this away"
    And the provider folder "INBOX" should not hold a message with the subject "Throw this away"

  Scenario: The HTML of a stranger's mail is made safe and its attachment only downloads
    Given the provider mailbox holds an HTML message "Special offer inside" with an attachment
    And I am signed in to mailroom as "reading-dave" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I open the message "Special offer inside"
    Then response should contain "report.pdf"
    When I fetch the html of the opened message
    Then response status should be 200
    And response should contain "<b>reader</b>"
    And response should not contain "<script"
    And response should not contain "onclick"
    And response should not contain "javascript:"
    And response should not contain "tracker.example"
    And the last answer should have the header "content-security-policy" with "default-src 'none'"
    And the last answer should have the header "content-security-policy" with "sandbox"
    And the last answer should not have the header "content-security-policy" with "allow-scripts"
    When I fetch the html of the opened message with the images allowed
    Then response should contain "tracker.example"
    When I download attachment 0 of the opened message
    Then response status should be 200
    And the last answer should have the header "content-type" with "application/octet-stream"
    And the last answer should have the header "content-disposition" with "attachment"

  Scenario: Nobody else can read my mail
    Given the provider mailbox holds a message "For my eyes only" from "Alice <alice@provider.example>"
    And I am signed in to mailroom as "reading-erin" with "mailroom:write"
    When I add my provider mailbox as "Main"
    And I refresh the mailbox "Main"
    And I open the message "For my eyes only"
    And I remember the opened message
    Given I am signed in to mailroom as "reading-mallory" with "mailroom:write"
    When I list my inbox
    Then response should not contain "For my eyes only"
    When I try to open the remembered message
    Then response status should be 404
    When I try to open the html of the remembered message
    Then response status should be 404
    When I try to move the remembered message to the trash
    Then response status should be 404
    And the provider folder "INBOX" should hold a message with the subject "For my eyes only"
