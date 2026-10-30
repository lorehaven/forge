---
title: Events
parent: Sage
grand_parent: Services
nav_order: 7
---

The answer of a turn is delivered on `GET /api/v1/turns/{id}/stream` as server-sent events. The names and JSON are defined once in `sage-core` (`stream.rs`: `StreamEvent`, `EVENT_NAMES`) and used by both the service and the browser client. Nothing on the stream is markup. The same function frames each event: `event: <name>\ndata: <json on one line>\n\n`.

## Flow

```text
POST /api/v1/turns            -> {"id": "<turn id>"}      (request kept in memory)
GET  /api/v1/turns/<id>/stream                           (starts the work)
   [status ...]  [delta ...]  [status checking]  [replace]  done
   or: failure
```

`POST /api/v1/turns/regenerate` and `POST /api/v1/conversations/{id}/resume` return the same kind of id. The stream answers `404` (empty) for an unknown id. Everything that goes wrong after the stream starts, including a missing or non-chat instance, is a `failure` event on a `200` stream, because `EventSource` cannot read an error status.

## Events

| Event | Data | When |
|---|---|---|
| `status` | `{"stage": "planning" \| "searching" \| "reading" \| "checking", "detail": string or null}` | Grounded mode only. `searching`: `detail` is the queries joined with ` · `. `reading` and `planning`: `detail` null. `checking`: sent before the fact-check when verification is on and there is evidence; `detail` null. |
| `delta` | `{"text": "<piece>"}` | Each chunk the model produces, in order. |
| `replace` | `{"text": "<answer>"}` | Sent once after the model finished and the checks ran, only when the stored answer (trimmed) differs from what was streamed: after grounded cleanup (invalid citations removed, links removed, warning appended, unsourced notice) or after tool results were embedded. The client replaces its whole text. |
| `done` | `{"conversation_id": string, "user_message_id": string or null, "message_id": string, "title": string}` | The turn is stored. `user_message_id` is null for a regeneration or resume (no new user message). `title` is the conversation title. Last event. |
| `failure` | `{"message": "<text>"}` | Last event on error. `message` is an `api_error_*` code (`api_error_switchboard_unavailable`, `api_error_instance_not_found`, `api_error_embedding_model_chat`) or the text of the underlying error when the vLLM stream fails. |

Example:

```text
event: status
data: {"stage":"searching","detail":"rust ownership · rust borrow checker"}

event: delta
data: {"text":"Ownership means"}

event: done
data: {"conversation_id":"5b1f7c52-0c43-4b7e-9d52-2c5a2f6a9e10","user_message_id":"a1","message_id":"a2","title":"What is Rust ownership?"}

```

The browser opens `EventSource` with the five names, parses each with `StreamEvent::parse`, ignores events it cannot parse, and closes the source on `done` or `failure`. If the connection ends without either, the app receives `StreamEnded` (the text `ui_chat_stream_lost`).

## What the turn does between the events

1. Resolve the capability profile (request, else configured) and a per-request tool registry bound to the user and conversation; find the instance (`instance_id`) via Switchboard; it must be chat-capable (task not `embed`/`embedding`).
2. Pre-create the conversation if new (blank title, owner, project) so retrieval and file tools can resolve it.
3. Augment the system prompt with the uploaded files and the best excerpts ([Files](files.md)).
4. Build the user message (with staged images for vision models) and load the history of the active branch.
5. If grounding is on (SAGE_GROUNDED and the profile enables `web_search`): plan, search, fetch, rank ([Grounding](grounding.md)), emitting `status` events, and append the numbered source block to the system prompt.
6. Prompt budget: `max_model_len` of the instance (default 2048 when unknown); generation reserve 2048 tokens when the window is above 4096, 1024 when above 2048, else 512; the prompt budget is the rest. Token estimates are `chars / 3 + 4` per message (rounded up), plus `SAGE_IMAGE_TOKEN_ESTIMATE` per image. The newest history that fits is kept. Images are capped at `SAGE_MAX_IMAGES_PER_REQUEST`, newest first.
7. Stream the model's answer as `delta` events (`max_tokens` = the generation reserve). Grounded mode sends no native `tools`.
8. If grounded: clean and fact-check the answer (`status checking`), see [Grounding](grounding.md).
9. Parse `<toolcall>` blocks from the answer, run them through the registry ([Tools](tools.md)) and embed the results as HTML blocks into the stored answer; a question about the tools themselves ("what tools do you have") runs no tool.
10. Store the user message (unless regeneration/resume), the assistant message (child of the user message), the RAG sources and web sources of the answer, and move the conversation's tip; the title is the first 30 characters of the first message (with `...`) when the conversation had none.
11. Send `replace` if the stored text differs, then `done`.

A turn whose client went away (the channel closed) is abandoned: nothing is stored and the pending request stays in the map.

## The stateless chat stream

`POST /api/v1/chat` uses a different framing: bare `data:` lines without event names, see [API](api.md).
