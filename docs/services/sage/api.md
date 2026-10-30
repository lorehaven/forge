---
title: API
parent: Sage
grand_parent: Services
nav_order: 1
---

The HTTP API of Sage 0.6.3, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1.0). LLM chat: conversations as message trees, streamed turns, file upload with retrieval, optional web grounding. /api/v1 is behind the realm Auth middleware (bearer or session cookie, audience sage); RequireWrite additionally demands sage:write for every method other than GET, HEAD and OPTIONS. Handlers take the user from the token and scope every record to its owner. Error bodies are plain-text translation keys (api_error_*), not JSON.

Paths below are relative to the base path (`BASE_PATH`, server variable `base`).

## models

Models and readiness.

### `GET /api/v1/models`

Models the composer offers and whether the chat can be used. Operation id `listModels`.

Models whose name matches no pattern of SAGE_SUPPORTED_MODELS are neither listed in `instances` nor waited for in `ready`/`defaults`. Embedding instances (task embed/embedding) are not listed. When Switchboard fails, `switchboard_ok` is false, `ready` false, `instances` empty and every default `unknown`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`ModelsView`](#modelsview) | Switchboard answered or not; `ready` is true when every supported default model has a running instance. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "switchboard_ok": true,
  "ready": true,
  "instances": [
    {
      "id": "pid-48213",
      "model": "qwen3-8b"
    }
  ],
  "defaults": [
    {
      "name": "qwen3-8b",
      "embedding": false,
      "state": "running"
    }
  ],
  "search_providers": [
    {
      "id": "duckduckgo",
      "label": "DuckDuckGo"
    }
  ],
  "default_search_provider": "duckduckgo",
  "accept": ".png,.jpg,.pdf,.txt"
}
```

## conversations

Conversations, branches and message edits.

### `GET /api/v1/conversation-id`

A fresh conversation id. Operation id `newConversationId`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Created`](#created) | A random UUID; nothing is stored. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "id": "5b1f7c52-0c43-4b7e-9d52-2c5a2f6a9e10"
}
```

### `GET /api/v1/conversations`

The caller's conversations. Operation id `listConversations`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`ConversationItem`](#conversationitem) | Newest `updated_at` first. A blank title is a conversation created by attaching a file before the first message. |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
[
  {
    "id": "c1",
    "title": "What is Rust?",
    "project_id": null,
    "updated_at": "2026-10-29T10:00:00.123Z"
  }
]
```

Example `500`:

```text
api_error_internal
```

### `GET /api/v1/conversations/{id}`

The active branch of a conversation. Operation id `getConversation`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Conversation id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`ConversationView`](#conversationview) | Messages from the root to the tip, each with its siblings, sources and attachments. |
| 403 |  |  | Another user's conversation. Empty body. |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_conversation_not_found
```

Example `500`:

```text
api_error_internal
```

### `DELETE /api/v1/conversations/{id}`

Delete a conversation. Operation id `deleteConversation`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Conversation id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted; messages and files cascade. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_conversation_not_found
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/conversations/{id}/switch`

Make another alternative the current branch. Operation id `switchBranch`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Conversation id. |

Request body (`application/json`), required: [`SwitchBody`](#switchbody)

| Field | Type | Required | Description |
|---|---|---|---|
| target_message_id | string | yes |  |

```json
{
  "target_message_id": "m7"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Done: the tip is the newest continuation below the target. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string | Unknown conversation, or the target is not a message of it. |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_message_not_found
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/messages/{id}/edit`

Another version of one of the person's messages. Operation id `editMessage`.

Creates a user message with the same parent and the trimmed content. Sage answers it when the client calls resume.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Message id. |

Request body (`application/json`), required: [`EditBody`](#editbody)

| Field | Type | Required | Description |
|---|---|---|---|
| new_content | string | yes |  |

```json
{
  "new_content": "What is Rust good for?"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Created`](#created) | The new message, a sibling of the old one, now the tip. |
| 400 | text/plain | string | The message is not a user message, or the new content is blank. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string | Unknown message or conversation. |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `400`:

```text
api_error_edit_non_user
```

Example `404`:

```text
api_error_message_not_found
```

Example `500`:

```text
api_error_internal
```

## projects

Projects.

### `GET /api/v1/projects`

The caller's projects. Operation id `listProjects`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`Project`](#project) | Newest `updated_at` first. |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
[
  {
    "id": "p1",
    "name": "Thesis",
    "updated_at": "2026-10-01T09:00:00.000Z"
  }
]
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/projects`

Create a project. Operation id `createProject`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`application/json`), required: [`NewProject`](#newproject)

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |

```json
{
  "name": "Thesis"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`Project`](#project) | The project. |
| 400 | text/plain | string | Blank name or longer than 120 characters. |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |
| 403 |  |  | The token lacks `sage:write`. Empty body. |

Example `400`:

```text
api_error_project_name
```

Example `500`:

```text
api_error_internal
```

## turns

Turns and their event stream.

### `POST /api/v1/conversations/{id}/resume`

Have Sage answer a trailing message of the person's. Operation id `resumeConversation`.

The turn re-answers the tip message: it is stored with `skip_user_message` so no new user message is made. The model is the first chat-capable instance Switchboard lists.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | Conversation id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`Resumed`](#resumed) | `id` is a new turn id to stream, or null when the last message is Sage's, the conversation has no tip, or no chat-capable model exists. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "id": null
}
```

Example `404`:

```text
api_error_conversation_not_found
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/turns`

Start a turn. Operation id `startTurn`.

Stores the request in memory under a new id (nothing is written to the database yet) and returns the id. `skip_user_message` and `parent_id` from the body are overwritten (false, none): a new message always branches from the conversation's tip. The conversation may be new; an existing one must belong to the caller. Pending requests are kept in a process-local map and removed when the turn finishes without being aborted.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`application/json`), required: [`TurnRequest`](#turnrequest)

| Field | Type | Required | Description |
|---|---|---|---|
| instance_id | string | yes | A chat-capable Switchboard instance id (from GET /api/v1/models). |
| message | string | yes |  |
| conversation_id | string | yes | From GET /api/v1/conversation-id for a new chat. |
| project_id | string or null | no |  |
| search_provider | string or null | no | duckduckgo, searxng, brave or serpapi; the configured default when absent. |
| capability_profile | string or null | no | web_assistant, code_assistant or cli_agent; an unknown name falls back to the configured profile. Accepted from the body although the browser client never sends it. |
| tool_confirmations | array of string | no | Names of tools the caller confirms (command, file_ops). Default `[]`. |
| file_ids | string | no | Staged file ids, comma separated. Default `""`. |
| parent_id | any | no | Ignored (reset to null). |
| skip_user_message | any | no | Ignored (reset to false). |

```json
{
  "instance_id": "pid-48213",
  "message": "What is Rust?",
  "conversation_id": "5b1f7c52-0c43-4b7e-9d52-2c5a2f6a9e10",
  "project_id": null,
  "search_provider": null,
  "file_ids": ""
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`TurnStarted`](#turnstarted) | The id of the stream that carries the answer. |
| 400 | text/plain | string | Blank message. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "id": "9e3c1f0a-3e2b-4a58-9a14-6b8b1b1c7d22"
}
```

Example `400`:

```text
api_error_empty_message
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/turns/regenerate`

A new answer next to an old one. Operation id `regenerate`.

Re-asks the parent user message on the first chat-capable instance Switchboard lists, with `skip_user_message`; the new answer becomes a sibling of the old.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`application/json`), required: [`RegenerateBody`](#regeneratebody)

| Field | Type | Required | Description |
|---|---|---|---|
| message_id | string | yes | An assistant message. |

```json
{
  "message_id": "m8"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`TurnStarted`](#turnstarted) | The id of the stream that carries the new answer. |
| 400 | text/plain | string | The message is not an assistant message, or has no parent. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 |  |  | Unknown message (empty body), or the parent is missing (`api_error_parent_not_found`). |
| 503 | text/plain | string | No chat-capable instance. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `400`:

```text
api_error_regenerate_non_assistant
```

Example `503`:

```text
api_error_no_models_available
```

### `GET /api/v1/turns/{id}/stream`

The answer of a turn as server-sent events. Operation id `streamTurn`.

Starts the work: the request is read from the pending map (it stays there until the turn completes; an aborted turn leaves it), the instance is looked up, files and history are loaded and a task runs the turn, feeding the stream. The pending id is not tied to a user: the caller needs only a valid login and the id.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| id | path | yes | string | The id returned by POST /api/v1/turns, /regenerate or /resume. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/event-stream | string | text/event-stream. Events `status`, `delta`, `replace`, `done`, `failure`; see events.md. Errors before the answer starts are sent as a `failure` event on a 200 stream, because EventSource cannot read an error status. |
| 404 |  |  | No pending turn with that id (empty body): never started, already consumed, or lost on a restart. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```text
event: delta
data: {"text":"Rust is"}

event: done
data: {"conversation_id":"5b1f...","user_message_id":"m1","message_id":"m2","title":"What is Rust?"}


```

## chat

Stateless chat and observability.

### `POST /api/v1/chat`

One-shot chat completion against an instance (JSON API). Operation id `chat`.

Stateless: nothing is stored. Used by tools such as the evaluation scripts. Does not check that the instance is chat-capable. `max_tokens` is the instance's `max_model_len` capped at 2048 (none when unknown). Grounded mode is on when SAGE_GROUNDED is true and the configured capability profile enables `web_search`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`application/json`), required: [`ChatRequest`](#chatrequest)

| Field | Type | Required | Description |
|---|---|---|---|
| instance_id | string | yes |  |
| message | string | yes |  |
| history | array of object | no | Earlier turns, oldest first; other roles are dropped. Default `[]`. |
| search_provider | string or null | no |  |
| evidence | array of object | no | Fixed source passages. Present (even empty) means planning, search and fetching are skipped and the answer is grounded on these. |

```json
{
  "instance_id": "pid-48213",
  "message": "What is Rust?",
  "history": [],
  "search_provider": "duckduckgo"
}
```

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/event-stream | string | text/event-stream of `data:` lines (no `event:` names). Plain mode: `{"content": "<piece>"}` per chunk, or `{"error": "api_error_stream_failed", "detail": "<text>"}`. Grounded mode: one `{"content": "<whole answer>"}` then one `{"grounded": true, "sources": [{"index", "url", "domain"}], "search_failed": bool, "unsourced": bool, "timings": {"retrieve_s", "generate_s", "verify_s"}}`. |
| 403 |  |  | The token lacks `sage:write`. Empty body. |
| 404 | text/plain | string | No instance with that id. |
| 500 | text/plain | string | Switchboard unavailable (`api_error_switchboard_unavailable`) or the vLLM stream could not start (`api_error_stream_failed`). |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```text
data: {"content":"Hello"}

data: {"content":" there"}


```

Example `404`:

```text
api_error_instance_not_found
```

Example `500`:

```text
api_error_switchboard_unavailable
```

### `GET /api/v1/chat/capabilities`

The configured capability profile. Operation id `chatCapabilities`.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`CapabilitiesResponse`](#capabilitiesresponse) | Profile name, description, enabled tools (sorted) and available search providers. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "profile": "web_assistant",
  "description": "Web browsing and research capabilities only",
  "available_tools": [
    "calculator",
    "file_list",
    "file_search",
    "web_fetch",
    "web_search"
  ],
  "available_search_providers": [
    "duckduckgo",
    "searxng"
  ]
}
```

### `GET /api/v1/chat/metrics`

Tool usage metrics for every profile. Operation id `chatMetrics`.

Not per user: any authenticated caller sees the totals.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`MetricsResponse`](#metricsresponse) | In-memory counters since the process started. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

### `GET /api/v1/chat/metrics/{profile}`

Tool usage metrics of one profile. Operation id `chatMetricsProfile`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| profile | path | yes | string | web_assistant, code_assistant or cli_agent. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`ProfileMetrics`](#profilemetrics) |  |
| 404 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_metrics_not_found
```

### `GET /api/v1/chat/costs`

Estimated cost of tool use per user and per profile. Operation id `chatCosts`.

Not restricted to the caller: every user's costs are visible to any authenticated caller.

Authentication: bearer token or session cookie.

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`CostsResponse`](#costsresponse) | In-memory. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

### `GET /api/v1/chat/costs/user/{user_id}`

Costs of one user. Operation id `chatCostsUser`.

Any authenticated caller may ask for any user.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| user_id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`UserCosts`](#usercosts) |  |
| 404 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_costs_not_found
```

### `GET /api/v1/chat/context-status/{profile}`

Context window limits of a profile. Operation id `chatContextStatus`.

Limits: 8192 tokens (warning at 75 percent) for code_assistant and cli_agent, 4096 (80 percent) for anything else.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| profile | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`ContextStatus`](#contextstatus) | Computed with 0 current tokens: only the limits are informative. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `200`:

```json
{
  "profile": "web_assistant",
  "current_tokens": 0,
  "max_tokens": 4096,
  "remaining_tokens": 4096,
  "usage_percent": 0.0,
  "is_near_limit": false,
  "is_at_limit": false
}
```

## files

Files and retrieval.

### `GET /api/v1/files`

Files of a conversation or a project. Operation id `listFiles`.

Exactly one of the two query parameters.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| conversation_id | query | no | string |  |
| project_id | query | no | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`StoredFile`](#storedfile) | For a conversation: its own files plus, when it belongs to a project, the project's files and those of the project's conversations; ordered by creation. For a project: its files. |
| 400 | text/plain | string | Both or neither scope given. |
| 403 |  |  | Another user's conversation or project. Empty body. |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `400`:

```text
api_error_file_scope_required
```

Example `404`:

```text
api_error_conversation_not_found
```

Example `500`:

```text
api_error_internal
```

### `POST /api/v1/files`

Upload a file to a conversation or a project. Operation id `uploadFile`.

multipart/form-data with `file` and exactly one of `conversation_id`, `project_id`. The target must exist and belong to the caller. Non-image files are processed in the background (extract, chunk, embed).

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`multipart/form-data`), required: [`UploadForm`](#uploadform)

| Field | Type | Required | Description |
|---|---|---|---|
| file | string `binary` | yes |  |
| conversation_id | string | no |  |
| project_id | string | no |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`FileInfo`](#fileinfo) | The stored file (status `uploaded`, or `ready` for an image). |
| 400 | text/plain | string | Missing/both scopes (`api_error_file_scope_required`), missing name (`api_error_missing_file_name`), unsupported extension (`api_error_unsupported_file_type`), empty (`api_error_file_empty`). |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string | Conversation or project not found. |
| 413 | text/plain | string | Over SAGE_FILE_MAX_SIZE_MB. |
| 422 | text/plain | string | Scope already holds SAGE_MAX_FILES_PER_SCOPE files. |
| 500 | text/plain | string |  |
| 501 | text/plain | string | In-memory database. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `201`:

```json
{
  "id": "f1",
  "file_name": "notes.md",
  "mime_type": "text/markdown",
  "file_size": 1204,
  "conversation_id": "c1",
  "project_id": null,
  "message_id": null,
  "status": "uploaded",
  "error_message": null
}
```

Example `400`:

```text
api_error_unsupported_file_type
```

Example `404`:

```text
api_error_conversation_not_found
```

Example `413`:

```text
api_error_file_too_large
```

Example `422`:

```text
api_error_file_limit_reached
```

Example `500`:

```text
api_error_internal
```

Example `501`:

```text
api_error_postgres_required
```

### `POST /api/v1/files/staged`

Upload a file from the composer. Operation id `uploadStagedFile`.

As POST /api/v1/files, with a conversation scope only (`project_id` is ignored). A conversation that is not stored yet is created, with a blank title, owned by the caller.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

Request body (`multipart/form-data`), required: [`UploadForm`](#uploadform)

| Field | Type | Required | Description |
|---|---|---|---|
| file | string `binary` | yes |  |
| conversation_id | string | no |  |
| project_id | string | no |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 201 | application/json | [`FileInfo`](#fileinfo) | The stored file with `message_id` null until a message is sent with it. |
| 400 | text/plain | string | Missing `conversation_id` (`api_error_missing_conversation_id`) or the errors of POST /api/v1/files. |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 413 | text/plain | string |  |
| 422 | text/plain | string |  |
| 500 | text/plain | string |  |
| 501 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `201`:

```json
{
  "id": "f1",
  "file_name": "notes.md",
  "mime_type": "text/markdown",
  "file_size": 1204,
  "conversation_id": "c1",
  "project_id": null,
  "message_id": null,
  "status": "uploaded",
  "error_message": null
}
```

Example `400`:

```text
api_error_missing_conversation_id
```

Example `413`:

```text
api_error_file_too_large
```

Example `422`:

```text
api_error_file_limit_reached
```

Example `500`:

```text
api_error_internal
```

Example `501`:

```text
api_error_postgres_required
```

### `GET /api/v1/files/{file_id}`

A file's state. Operation id `getFile`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| file_id | path | yes | string | File id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`StoredFile`](#storedfile) |  |
| 403 |  |  | Not the owner. Empty body. |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_file_not_found
```

Example `500`:

```text
api_error_internal
```

### `DELETE /api/v1/files/{file_id}`

Delete a file. Operation id `deleteFile`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| file_id | path | yes | string | File id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 204 |  |  | Deleted; blob, chunks and rag context rows follow (CASCADE / SET NULL). |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_file_not_found
```

Example `500`:

```text
api_error_internal
```

### `GET /api/v1/files/{file_id}/download`

The stored bytes. Operation id `downloadFile`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| file_id | path | yes | string | File id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/octet-stream | string `binary` | The file, `content-type` = stored MIME type, `Content-Disposition` `inline` for images and `attachment` otherwise, with the name stripped of control characters, quotes and backslashes. |
| 403 |  |  | Not the owner. |
| 404 | text/plain | string | Unknown file, or no blob. |
| 501 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_file_content_not_found
```

Example `501`:

```text
api_error_postgres_required
```

### `POST /api/v1/files/{file_id}/reprocess`

Run the extract, chunk and embed pipeline again. Operation id `reprocessFile`.

Authentication: bearer token or session cookie.

Permission: `sage:write (RequireWrite on non-GET methods)`.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| file_id | path | yes | string | File id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 202 | application/json | [`StoredFile`](#storedfile) | The file as stored (the status is read again with GET). |
| 403 |  |  | Someone else's record (empty body), or, for a write method, the token lacks `sage:write` (RequireWrite, empty body). |
| 404 | text/plain | string |  |
| 409 | text/plain | string | Already processing. |
| 422 | text/plain | string | Images have no pipeline. |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_file_not_found
```

Example `409`:

```text
api_error_file_already_processing
```

Example `422`:

```text
api_error_image_not_processable
```

### `GET /api/v1/files/{file_id}/chunks`

The chunks of a file. Operation id `listChunks`.

Authentication: bearer token or session cookie.

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| file_id | path | yes | string | File id. |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | array of [`FileChunk`](#filechunk) | Ordered by `chunk_index`; the embedding vector itself is not returned. |
| 403 |  |  | Not the owner. |
| 404 | text/plain | string |  |
| 500 | text/plain | string |  |
| 401 |  |  | No valid token or session. From `Auth` (empty body, or a redirect for a browser) and from the handler (empty body). |

Example `404`:

```text
api_error_file_not_found
```

Example `500`:

```text
api_error_internal
```

## ui

Browser pages, assets and SSO routes.

### `GET /ui/chat/stats/{conversation_id}`

Token statistics of a conversation. Operation id `chatStats`.

A leftover of the server-rendered UI; the browser client does not call it. It uses the Username extractor (token or session cookie) but does not check that the conversation belongs to the caller, and is outside the `Auth` prefixes. Context window fixed at 4096.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| conversation_id | path | yes | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`ChatStats`](#chatstats) | `success` true with `stats`, `display` and an optional `warning`. |
| 401 |  |  | No valid session. Empty body. |
| 500 | application/json | [`ChatStats`](#chatstats) | `{"success": false, "error": "<text>"}`. |

Example `200`:

```json
{
  "success": true,
  "stats": {
    "total_messages": 4,
    "total_tokens": 310,
    "max_tokens": 4096,
    "system_tokens": 120,
    "utilization_percent": 7.5,
    "near_limit": false,
    "at_limit": false
  },
  "display": "Tokens: 310/4096 (7.6%) | Messages: 4 | System: 120",
  "warning": null
}
```

### `GET /ui`

Redirect to the home page, or to login without a session. Operation id `uiRoot`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/`

Same as /ui. Operation id `uiRootSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home, or {base}/ui/login when not signed in. |

### `GET /ui/home`

Client page, chat screen. Operation id `uiHome`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/home/`

Same as /ui/home. Operation id `uiHomeSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/initializing`

Client page, initializing screen. Operation id `uiInitializing`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/html | string | The HTML page that starts the browser client (cache-control no-store, nonce CSP). |
| 302 |  |  | To {base}/ui/login without a valid session cookie. |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/quench.js`

The quench-ui loader script. Operation id `uiAssetScript`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/app.css`

Shell stylesheet followed by the client stylesheet. Operation id `uiAssetStyle`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/css | string | The file (cache-control: no-cache). |

### `GET /ui/assets/shell.js`

The forge-shell script. Operation id `uiAssetShell`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | text/javascript | string | The file (cache-control: no-cache). |

### `GET /ui/assets/favicon.png`

The service icon. Operation id `uiAssetFavicon`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | image/png | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/assets/app.wasm`

The browser client module. Operation id `uiAssetModule`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/wasm | string `binary` | The file (cache-control: no-cache). |
| 404 |  |  | app.wasm is not in WEB_DIR ("the browser client is not installed on this server"). |

### `GET /ui/login`

Start sign-in (SSO redirect to Gatehouse). Operation id `uiLogin`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home (refresh cookie accepted, new cookies set) or to Gatehouse's /api/v1/authorize; sets the cookie forge_authorize_state. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/login/`

Same as /ui/login. Operation id `uiLoginSlash`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | See /ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/auth/callback`

Complete the authorization-code exchange. Operation id `uiAuthCallback`.

Authentication: none (the route checks the session itself where noted).

| Name | In | Required | Type | Description |
|---|---|---|---|---|
| code | query | no | string |  |
| state | query | no | string |  |

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To {base}/ui/home with session cookies, or to {base}/ui/login on any failure. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/logout`

Realm logout through Gatehouse. Operation id `uiLogout`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 302 |  |  | To Gatehouse's /ui/logout with a redirect back to {base}/ui/login. |
| 503 |  |  | Text `gatehouse is not configured` (GATEHOUSE_URL, GATEHOUSE_CLIENT_ID or GATEHOUSE_CLIENT_SECRET missing). |

### `GET /ui/status`

Session status for the page shell. Operation id `uiStatus`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | Always 200. |

### `POST /ui/refresh`

Exchange the refresh cookie for new cookies. Operation id `uiRefresh`.

Authentication: none (the route checks the session itself where noted).

| Status | Content type | Body | Meaning |
|---|---|---|---|
| 200 | application/json | [`AuthStatus`](#authstatus) | New session and refresh cookies set. |
| 401 |  |  | No refresh cookie, or Gatehouse refused it. Empty body. |

Example `200`:

```json
{
  "authenticated": true,
  "username": null,
  "roles": []
}
```

## Schemas

### Created

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |

### Project

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| name | string | yes |  |
| updated_at | string | yes | RFC 3339. |

### NewProject

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |

### ConversationItem

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| title | string | yes | The first 30 characters of the first message, with `...` when longer; blank until the first message. |
| project_id | string or null | yes |  |
| updated_at | string | yes |  |

### FileInfo

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| file_name | string | yes |  |
| mime_type | string | yes |  |
| file_size | integer `int64` | yes |  |
| conversation_id | string or null | yes |  |
| project_id | string or null | yes |  |
| message_id | string or null | yes |  |
| status | string (`uploaded`, `processing`, `ready`, `failed`) | yes |  |
| error_message | string or null | yes |  |

### StoredFile

A files row as stored (GET /api/v1/files*, reprocess): FileInfo plus owner and timestamps.

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| file_name | string | yes |  |
| mime_type | string | yes |  |
| file_size | integer `int64` | yes |  |
| conversation_id | string or null | yes |  |
| project_id | string or null | yes |  |
| message_id | string or null | yes |  |
| status | string (`uploaded`, `processing`, `ready`, `failed`) | yes |  |
| error_message | string or null | yes |  |
| owner | string | yes |  |
| created_at | string | yes |  |
| updated_at | string | yes |  |

### FileChunk

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| file_id | string | yes |  |
| chunk_index | integer | yes |  |
| content | string | yes |  |
| embedding_model | string or null | yes |  |
| metadata | any | yes | JSON: heading, page, language, depending on the format. |
| created_at | string | yes |  |

### UploadForm

| Field | Type | Required | Description |
|---|---|---|---|
| file | string `binary` | yes |  |
| conversation_id | string | no |  |
| project_id | string | no |  |

### Source

| Field | Type | Required | Description |
|---|---|---|---|
| file_name | string | yes | File name, or the web page label. |
| chunk_index | integer or null | yes |  |
| detail | string or null | yes |  |
| similarity | number or null | yes |  |

### MessageNode

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| parent_id | string or null | yes |  |
| role | string (`user`, `assistant`) | yes |  |
| content | string | yes |  |
| created_at | string | yes |  |
| siblings | array of string | yes | Ids of the alternatives at this point of the tree, this one included, oldest first. |
| sources | array of [`Source`](#source) | yes |  |
| attachments | array of [`FileInfo`](#fileinfo) | yes |  |

### ConversationView

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| title | string | yes |  |
| project_id | string or null | yes |  |
| messages | array of [`MessageNode`](#messagenode) | yes |  |

### ModelOption

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes | Switchboard instance id. |
| model | string | yes |  |

### DefaultModelState

| Field | Type | Required | Description |
|---|---|---|---|
| name | string | yes |  |
| embedding | boolean | yes |  |
| state | string (`running`, `starting`, `pending`, `failed`, `unknown`) | yes |  |

### SearchProviderOption

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |
| label | string | yes |  |

### ModelsView

| Field | Type | Required | Description |
|---|---|---|---|
| switchboard_ok | boolean | yes |  |
| ready | boolean | yes |  |
| instances | array of [`ModelOption`](#modeloption) | yes |  |
| defaults | array of [`DefaultModelState`](#defaultmodelstate) | yes |  |
| search_providers | array of [`SearchProviderOption`](#searchprovideroption) | yes |  |
| default_search_provider | string | yes |  |
| accept | string | yes | The `accept` value of the file picker: every allowed extension with a dot, comma-separated (image extensions only when SAGE_IMAGES_ENABLED). |

### SwitchBody

| Field | Type | Required | Description |
|---|---|---|---|
| target_message_id | string | yes |  |

### EditBody

| Field | Type | Required | Description |
|---|---|---|---|
| new_content | string | yes |  |

### RegenerateBody

| Field | Type | Required | Description |
|---|---|---|---|
| message_id | string | yes | An assistant message. |

### TurnStarted

| Field | Type | Required | Description |
|---|---|---|---|
| id | string | yes |  |

### Resumed

| Field | Type | Required | Description |
|---|---|---|---|
| id | string or null | yes |  |

### TurnRequest

| Field | Type | Required | Description |
|---|---|---|---|
| instance_id | string | yes | A chat-capable Switchboard instance id (from GET /api/v1/models). |
| message | string | yes |  |
| conversation_id | string | yes | From GET /api/v1/conversation-id for a new chat. |
| project_id | string or null | no |  |
| search_provider | string or null | no | duckduckgo, searxng, brave or serpapi; the configured default when absent. |
| capability_profile | string or null | no | web_assistant, code_assistant or cli_agent; an unknown name falls back to the configured profile. Accepted from the body although the browser client never sends it. |
| tool_confirmations | array of string | no | Names of tools the caller confirms (command, file_ops). Default `[]`. |
| file_ids | string | no | Staged file ids, comma separated. Default `""`. |
| parent_id | any | no | Ignored (reset to null). |
| skip_user_message | any | no | Ignored (reset to false). |

### ChatRequest

| Field | Type | Required | Description |
|---|---|---|---|
| instance_id | string | yes |  |
| message | string | yes |  |
| history | array of object | no | Earlier turns, oldest first; other roles are dropped. Default `[]`. |
| search_provider | string or null | no |  |
| evidence | array of object | no | Fixed source passages. Present (even empty) means planning, search and fetching are skipped and the answer is grounded on these. |

### CapabilitiesResponse

| Field | Type | Required | Description |
|---|---|---|---|
| profile | string | yes |  |
| description | string | yes |  |
| available_tools | array of string | yes |  |
| available_search_providers | array of string | yes |  |

### ToolMetrics

| Field | Type | Required | Description |
|---|---|---|---|
| calls | integer | yes |  |
| successes | integer | yes |  |
| failures | integer | yes |  |
| timeouts | integer | yes |  |
| total_duration_ms | integer | yes |  |
| min_duration_ms | integer | yes | u128::MAX until a call was recorded. |
| max_duration_ms | integer | yes |  |

### ProfileMetrics

| Field | Type | Required | Description |
|---|---|---|---|
| profile | string | yes |  |
| total_calls | integer | yes |  |
| total_successes | integer | yes |  |
| total_failures | integer | yes |  |
| tools | object | yes |  |

### MetricsResponse

| Field | Type | Required | Description |
|---|---|---|---|
| profiles | array of [`ProfileMetrics`](#profilemetrics) | yes |  |

### UserCosts

| Field | Type | Required | Description |
|---|---|---|---|
| user_id | string | yes |  |
| request_count | integer | yes |  |
| total_duration_ms | integer | yes |  |
| total_tokens_used | integer | yes |  |
| total_api_calls | integer | yes |  |
| estimated_cost | number | yes |  |
| avg_cost_per_request | number | yes |  |

### ProfileCosts

| Field | Type | Required | Description |
|---|---|---|---|
| profile | string | yes |  |
| request_count | integer | yes |  |
| total_duration_ms | integer | yes |  |
| total_tokens_used | integer | yes |  |
| total_api_calls | integer | yes |  |
| estimated_cost | number | yes |  |
| avg_cost_per_request | number | yes |  |

### CostsResponse

| Field | Type | Required | Description |
|---|---|---|---|
| users | array of [`UserCosts`](#usercosts) | yes |  |
| profiles | array of [`ProfileCosts`](#profilecosts) | yes |  |

### ContextStatus

| Field | Type | Required | Description |
|---|---|---|---|
| profile | string | yes |  |
| current_tokens | integer | yes |  |
| max_tokens | integer | yes |  |
| remaining_tokens | integer | yes |  |
| usage_percent | number | yes |  |
| is_near_limit | boolean | yes |  |
| is_at_limit | boolean | yes |  |

### ChatStats

| Field | Type | Required | Description |
|---|---|---|---|
| success | boolean | yes |  |
| stats | object | no |  |
| display | string | no |  |
| warning | string or null | no |  |
| error | string | no |  |

### Error

| Field | Type | Required | Description |
|---|---|---|---|
| error | string | yes |  |

### AuthStatus

| Field | Type | Required | Description |
|---|---|---|---|
| authenticated | boolean | yes |  |
| username | string or null | yes |  |
| roles | array of string | yes |  |

