---
title: Sage
parent: Services
nav_order: 10
has_children: true
---

Sage is the estate's LLM chat service. A conversation is a tree of messages; a turn sends the person's message to a vLLM instance that Switchboard manages and streams the answer back as server-sent events. Sage can ground answers in web search (a code-driven pipeline for small models), retrieve from uploaded files (extract, chunk, embed with pgvector), call a small set of tools, and keeps its configured default models running by asking Switchboard to launch them. The browser client is a `quench-ui` program compiled to WebAssembly.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `sage-core` | `services/sage/core` | 0.1.0 | Wire model (`ConversationView`, `MessageNode`, `FileInfo`, `ModelsView`, bodies), the stream events (`StreamEvent`, `EVENT_NAMES`), form drafts (`MessageDraft`, `ProjectDraft`), the Markdown renderer. No I/O. |
| `sage-app` | `services/sage/app` | 0.1.0 | Screens and the `Request`/`Msg` protocol. |
| `sage-web` | `services/sage/web` | 0.1.0 | `wasm32` module: `Client::execute`, the `fetch` transport, `EventSource` for turns. |
| `sage` | `services/sage/service` | 0.6.3 | The server. Binary `sage-service`. |

The foundry module `sage` 0.2.0 (schema `sage`) owns the tables.

## Facts

* Foreman: HTTPS 8443, `BASE_PATH=/sage`, `needs = ["gatehouse", "switchboard"]`. The service is ready when Switchboard's and Gatehouse's `/health/ready` both answer.
* Dependencies: Postgres with the `vector` extension (required: `DATABASE_URL`), Gatehouse (JWKS, SSO), Switchboard (OAuth client `sage-switchboard`, client credentials), the vLLM instances themselves (called directly at `http://<host>:<port>/v1/...`), Redis for sessions (optional), the internet for web search and page fetching.
* Authentication: `Auth` and `RequireWrite` over `/api/v1/{files,chat,models,conversation-id,conversations,projects,turns,messages}`. Handlers read the user name from the token and scope records to it. See [Permissions](permissions.md).
* Records are private to their owner: foreign records answer `403` (empty body). There is no sharing and no admin view.

## Pages

| Page | Content |
|---|---|
| [API](api.md), [openapi.yaml](openapi.yaml) | 28 API operations and 17 browser routes (one is the leftover `/ui/chat/stats/{conversation_id}`). |
| [Events](events.md) | The turn stream: event names, JSON, ordering, failures. |
| [Models](models.md) | `SAGE_DEFAULT_MODELS`, `SAGE_SUPPORTED_MODELS`, the monitor, the picker, readiness. |
| [Grounding](grounding.md) | The web-grounding pipeline and its answer checks. |
| [Files and retrieval](files.md) | Upload, extraction, chunks, embeddings, RAG, images. |
| [Tools and profiles](tools.md) | Tools, capability profiles, confirmations, search providers. |
| [Observability](observability.md) | Metrics, costs, rate limiting, audit log. |
| [Permissions](permissions.md) | Token requirements and ownership rules. |
| [Configuration](configuration.md) | Every environment variable. |
| [Data](data.md) | Tables. |
| [Operations](operations.md) | Running, health, failure modes. |
| [Client](client.md) | Screens, requests, i18n keys. |

Shared behaviour: [Common service runtime](../common-runtime.md).
