---
title: Permissions
parent: Sage
grand_parent: Services
nav_order: 6
---

Gatehouse catalog entry `sage`: actions `read` and `write` (see [Authentication and permissions](../../guides/authentication.md)). Templates: `viewer` holds `sage:read`, `editor` holds `sage:read` and `sage:write`.

* `Auth` (bearer or session cookie, audience `sage`) wraps eight prefixes: `/api/v1/files`, `/api/v1/chat`, `/api/v1/models`, `/api/v1/conversation-id`, `/api/v1/conversations`, `/api/v1/projects`, `/api/v1/turns`, `/api/v1/messages`. The prefix match is textual: `/api/v1/chat` also covers `/api/v1/chat/metrics`.
* `RequireWrite` inside `Auth` on the same prefixes: methods other than GET, HEAD and OPTIONS need `Claims::can("sage", "write")`; else `403`, empty body. The `sage:read` grant is never checked: a token with the audience `sage` and no grant can use every GET.
* Handlers take the user from `Username` (`quench_auth::http::routers::ui::get_user_from_req`): the claims `Auth` put in the request, else the session cookie verified against the realm and the session store. A request with no valid identity gets `401` with an empty body. With `SERVICE_AUTH_ENABLED=false` the identity is a synthetic claim for the user `admin` (role `admin`), so every record belongs to `admin`.
* Ownership: conversations, files and projects carry an `owner`. A record that exists and belongs to someone else answers `403` (empty body); a missing one `404` with a key. `GET /api/v1/conversations` and `/projects` list only the caller's. Messages are reached through their conversation; `POST /api/v1/messages/{id}/edit` checks the conversation owner after checking that the message is a user message.
* Not restricted by user: `GET /api/v1/chat/metrics`, `/chat/metrics/{profile}`, `/chat/costs`, `/chat/costs/user/{user_id}`, `/chat/context-status/{profile}` (process-wide counters; any authenticated caller may read every user's cost record), `GET /api/v1/models`, `GET /api/v1/chat/capabilities`.
* `GET /api/v1/turns/{id}/stream` needs a valid identity but does not compare it with the turn's owner; the turn id is a random UUID returned to the starter.
* `POST /api/v1/turns` accepts `capability_profile` and `tool_confirmations` from the body, so any caller with `sage:write` can select `cli_agent` and confirm `command` and `file_ops` for that turn. See [Tools and profiles](tools.md).
* Sage calls Switchboard as the client `sage-switchboard` (client credentials; audience `switchboard`), which holds a wildcard role: `GET /api/v1/vllm/instances`, `POST /api/v1/vllm/instances`, `DELETE /api/v1/vllm/instances/{id}`.
