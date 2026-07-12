Feature: File Upload and RAG Support
  As a user
  I want to upload files to conversations and projects
  So that the agent can use them for retrieval-augmented answers

  Background:
    Given sage API is available

  Scenario: File list requires authentication
    When I request the file list without authentication
    Then the response status should be 401

  Scenario: File list requires a conversation or project scope
    When I request the file list without a scope
    Then the response status should be 400

  Scenario: File list for an unknown conversation
    When I request the file list for conversation "missing-conversation"
    Then the response status should be 404

  Scenario: File list for an unknown project
    When I request the file list for project "missing-project"
    Then the response status should be 404

  Scenario: Upload rejects unsupported file types
    When I upload file "malware.exe" to project "any-project"
    Then the response status should be 400
    And the response should mention "Unsupported file type"

  Scenario: Upload rejects requests without a target scope
    When I upload file "notes.md" without a target scope
    Then the response status should be 400
    And the response should mention "conversation_id or project_id"

  Scenario: Upload to an unknown conversation
    When I upload file "notes.md" to conversation "missing-conversation"
    Then the response status should be 404

  Scenario: Upload to an unknown project
    When I upload file "notes.md" to project "missing-project"
    Then the response status should be 404

  Scenario: Upload requires authentication
    When I upload file "notes.md" to project "any-project" without authentication
    Then the response status should be 401

  Scenario: Metadata of an unknown file
    When I request metadata for file "missing-file"
    Then the response status should be 404

  Scenario: Deleting an unknown file
    When I delete file "missing-file"
    Then the response status should be 404

  Scenario: Reprocessing an unknown file
    When I reprocess file "missing-file"
    Then the response status should be 404

  Scenario: Downloading an unknown file
    When I download file "missing-file"
    Then the response status should be 404

  Scenario: Download requires authentication
    When I download file "missing-file" without authentication
    Then the response status should be 401

  Scenario: Listing chunks of an unknown file
    When I request chunks for file "missing-file"
    Then the response status should be 404

  Scenario: file_search tool is advertised in capabilities
    When I request the chat capabilities
    Then the response status should be 200
    And the response should mention "file_search"

  Scenario: Files UI panel requires authentication
    When I request the files panel without authentication
    Then the response status should be 401
