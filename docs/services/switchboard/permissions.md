---
title: Permissions
parent: Switchboard
grand_parent: Services
nav_order: 6
---

Source: `service/src/routers/models/mod_impl.rs`. Gatehouse catalog entry `switchboard` has the actions `read`, `launch`, `stop`, `delete-model` (see [Authentication and permissions](../../guides/authentication.md)). The `editor` template holds `read`, `launch`, `stop`; `viewer` holds `read`. No template grants `delete-model`.

`can(claims, config, action)` is true when authentication is off, or when the claims hold `<SERVICE_NAME>:<action>` (`Claims::can`, so a wildcard role also passes). `is_admin` is true when authentication is off or the claims have a wildcard role. `OptionalClaims` takes the claims from the `Auth` middleware, or, when absent, decodes the session cookie.

| Route | Check |
|---|---|
| `GET /api/v1/gpu/status`, `GET /api/v1/vllm/list`, `GET /api/v1/vllm/instances`, `POST /api/v1/models/list` | `Auth` only. The action `read` is not checked by the service. |
| `GET /api/v1/vllm/capabilities` | `Auth` only; the answer reports `can_launch`, `can_stop`, `can_delete_model`. |
| `POST /api/v1/vllm/instances`, `POST /api/v1/vllm/launch` | `launch`; else `403` with an empty body. |
| `DELETE /api/v1/vllm/instances/{id}` | `stop`. |
| `POST /api/v1/models/delete`, `POST /api/v1/models/delete-form` | `delete-model`, and the path must lie under `HF_ROOTS` or `GGUF_ROOTS`. |
| `GET /api/v1/models/running` | wildcard role (`admin`, `service`). A user with only `switchboard:read` gets `403`. Sage reads it with the `sage-switchboard` client-credentials token (role `service`). |
| `/ui/*` pages | a valid session cookie. |

There is no `RequireWrite` on these prefixes. The client hides the buttons when the capability is false, but the service enforces the grants.
