---
title: Authentication and permissions
parent: Guides
nav_order: 4
---

Gatehouse is the only service that knows users and passwords. It issues Ed25519-signed JWTs; every other service verifies them locally against Gatehouse's JWKS and decides what the caller may do from the token's `scope` claim. This page describes the token, the grants, the grant types Gatehouse accepts and what each service enforces. Sources: `services/gatehouse/service/src`, `services/gatehouse/service/config/{permissions,clients}.toml` and the `quench-auth` 0.4.14 crate (in the Quench book).

## The token

Claims (`quench_auth::domain::jwt::Claims`):

| Claim | Type | Meaning |
|---|---|---|
| `sub` | string | The user name, or the `client_id` for a client-credentials token. |
| `aud` | array of strings | Services the token may be presented to. A service rejects a token whose `aud` does not contain its `SERVICE_NAME`. Defaults to `[]`. |
| `scope` | string | Space- or comma-separated entries: roles (`admin`, `user`, `service`; never contain `:`) and grants `service:action`. |
| `exp`, `iat` | number | Expiry and issue time, seconds since the epoch. |
| `sid` | string, optional | Session id; present for browser sessions and refresh-backed tokens, absent for client-credentials and app-token grants. |

Lifetimes (read from the environment by `JwtConfig`): `ACCESS_TOKEN_TTL_SECS` (default `900`), `REFRESH_TOKEN_TTL_SECS` (default `604800`; the `forge` overlay sets `2592000`). An authorization code lives 60 seconds.

Transport. `Auth` (a `quench-http` middleware) takes the token from `Authorization: Bearer <jwt>` first, else from the session cookie. Cookie names: `forge_session` (override `AUTH_COOKIE_NAME`) and `forge_refresh` (override `AUTH_REFRESH_COOKIE_NAME`). Cookies are `Path=/`, `HttpOnly`, `Secure`, `SameSite=Lax`. When the access token is expired but a refresh cookie is present, `Auth` renews both cookies transparently. No token: `401` with `WWW-Authenticate: Bearer`.

## Grants: `service:action`

A user holds roles and a permission set `{service: [action, ...]}`. The token flattens them into `scope`: `admin`, or `sage:read sage:write warehouse:read`. `Claims::can(service, action)` is true when

1. any role on the token is a wildcard (`admin` and `service` both are), or
2. `scope` contains the entry `service:action`. The entry is split at the first `:`, so a resource-scoped grant `conveyor:project:42:write` means service `conveyor`, action `project:42:write`.

There is no ordering between actions: `write` does not imply `read`. A wildcard role carries no `service:action` entries (`user_scope` writes none), and its `aud` is every audience of the realm.

The catalog of services and actions is `services/gatehouse/service/config/permissions.toml` (override path `PERMISSIONS_CONFIG`). Adding a service or an action means editing that file and restarting Gatehouse. A service that is not listed is in no token's audience, so it answers every request `401`.

| Service | Actions | Resource types |
|---|---|---|
| `sage` | `read`, `write` | - |
| `warehouse` | `read`, `write` | `storage` |
| `conveyor` | `read`, `write` | `project` |
| `switchboard` | `read`, `launch`, `stop`, `delete-model` | - |
| `workbench` | `read`, `write` | `project` |
| `daybook` | `read`, `write` | - |
| `gantry` | `read`, `deploy`, `scale`, `activate`, `rollback` | `target` |
| `gauge` | `read` | - |
| `hoist` | `read` | - |
| `gatehouse` | `read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions`, `manage-signing-keys`, `notify` | - |

Templates (named bundles assignable in one step; every action must exist in the catalog or Gatehouse fails at startup):

| Template | Grants |
|---|---|
| `viewer` | `read` on sage, warehouse, switchboard, conveyor, workbench, daybook, gantry, gauge, hoist |
| `editor` | `read`+`write` on sage, warehouse, conveyor, workbench, daybook; switchboard `read`,`launch`,`stop`; `read` on gantry, gauge, hoist |
| `user-manager` | gatehouse `read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions` |

`[registration] default_template = "viewer"`: what a self-registered account starts with. Assigning the `admin` or `service` role stays gated on holding the `admin` role; no catalog action grants it.

Resource-scoped grants, as the services check them:

| Service | Form | Source |
|---|---|---|
| warehouse | `warehouse:storage:<name>:<action>`; a storage's owner needs no grant | `routers/files/authz.rs` |
| conveyor | `conveyor:project:<id>:<action>` (an ancestor node covers what is nested under it) | `routers/api/authz.rs` |
| workbench | `workbench:project:<id>:<action>` (flat, no ancestor walk) | `routers/api/authz.rs` |
| gantry | `gantry:target:<package>:<action>` | `routers/api/authz.rs` |

## How a caller gets a token

All through Gatehouse, under its `BASE_PATH` (`/gatehouse`).

| Flow | Request | Result |
|---|---|---|
| Browser SSO (authorization code with PKCE) | `GET /api/v1/authorize?client_id&redirect_uri&state&code_challenge[&code_challenge_method=S256&scope]` then `POST /api/v1/token` with `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`, `client_secret`, `code_verifier` | Access + refresh token pair. Only `S256` is accepted. `redirect_uri` must match the client's registration. With no session the browser is sent to `/ui/login?redirect=...`. |
| Refresh | `POST /api/v1/token` `grant_type=refresh_token`, `refresh_token` | New pair (the refresh token rotates). |
| Machine to machine | `POST /api/v1/token` `grant_type=client_credentials`, `client_id`, `client_secret` | Access token only (empty `refresh_token`), `scope` = `service` (wildcard), `aud` = the client's `allowed_scopes`. |
| Application token | `POST /api/v1/token` `grant_type=app_token`, `app_token=lht_...` | Access token only. |
| Password login (apps, CLIs) | `POST /api/v1/auth/login`, `POST /api/v1/auth/refresh`, `POST /api/v1/auth/logout`, `GET /api/v1/auth/userinfo` | Used by `gatehouse-client` (the native apps). A second factor is not supported there: the client reports `MfaRequired`. |
| Browser login | `GET/POST /ui/login`, `/ui/login/mfa`, `POST /ui/refresh` | Sets the two cookies. |

The token endpoint's body is form-encoded. Error texts include `unsupported grant_type '<x>'`, `unknown client`, `invalid client credentials`, `PKCE verification failed`, `invalid, expired or already-used code`.

Application tokens (`lht_` prefix) are created by a logged-in user with `POST /api/v1/me/tokens` (a login session is required; an app token cannot mint another), listed with `GET /api/v1/me/tokens`, revoked by id. Fields: `name`, `scope` (list of `service:action` entries; empty means the owner's whole current set), `expires_in_days` (minimum 1, maximum and default 365). On exchange the token carries the entries that the owner still holds, so a later demotion applies. The CLI tools keep an app token in `.<tool>/secret.toml` (`gatehouse-auth` crate; see the Libraries section) and exchange it on every run.

### Clients

`config/clients.toml` lists the OAuth clients. A secret is never in the file: `secret_env` names the environment variable Gatehouse reads it from, and the owning service sets the same value as `GATEHOUSE_CLIENT_SECRET`.

| `client_id` | Kind | Audience (`allowed_scopes`) | `secret_env` |
|---|---|---|---|
| `sage`, `switchboard`, `warehouse`, `conveyor`, `workbench`, `gantry`, `gauge` | authorization code, `redirect_path = /ui/auth/callback` | the service itself | `CLIENT_SECRET_<NAME>` |
| `sage-switchboard` | client credentials | `switchboard` | `CLIENT_SECRET_SAGE_SWITCHBOARD` |
| `conveyor-gatehouse` | client credentials | `gatehouse` | `CLIENT_SECRET_CONVEYOR_GATEHOUSE` |
| `conveyor-publish` | client credentials | `warehouse` | `CLIENT_SECRET_CONVEYOR_PUBLISH` |
| `gantry-warehouse` | client credentials | `warehouse` | `CLIENT_SECRET_GANTRY_WAREHOUSE` |

Daybook and Hoist have no client: Daybook's browser client signs in through the API like the apps do, and Hoist's service only verifies tokens.

## A relying party: what a service needs

Environment read by `JwtConfig` and the SSO helpers: `GATEHOUSE_URL` (including the base path; the JWKS is `{GATEHOUSE_URL}/.well-known/jwks.json`), `SERVICE_NAME` (the audience it checks), `SERVICE_AUDIENCES` (comma list; default `SERVICE_NAME`), `SERVICE_AUTH_ENABLED` (default `false`: the dev bypass, every request passes and `can` helpers return true), `SERVICE_REALM`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, `GATEHOUSE_TLS_VERIFY`. Gatehouse does not use `SERVICE_AUDIENCES`: it derives the realm's audience list from the permission catalog.

Middleware (`quench_auth::http::middleware`): `Auth` verifies and puts `Claims` into the request extensions. `RequireWrite` (placed inside `Auth`) answers `403` to any method other than `GET`, `HEAD`, `OPTIONS` unless the token `can(<this service>, "write")`.

## What each service enforces

Prefixes are relative to the service's `BASE_PATH`.

| Service | `Auth` over | Further checks |
|---|---|---|
| gatehouse | not wrapped by `wrap_auth`: it mints the tokens; admin routes use an `AdminClaims`/`can("gatehouse", <action>)` extractor | `read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions`, `manage-signing-keys`, `notify` per route |
| warehouse | files, artifacts/apk, rivets, admin | `RequireWrite` over `/admin`, `/api/v1/artifacts`, `/api/v1/apk` and rivets writes; files check `warehouse:write` or `warehouse:storage:<name>:<action>` per request; the Cargo and Docker registry routes are not covered here (*Unverified:* see the warehouse page) |
| switchboard | `/api/v1/vllm`, models, gpu | no `RequireWrite`; `can(.., "launch")`, `"stop"`, `"delete-model"` per route; `read` on listing |
| sage | `/api/v1/{files,chat,models,conversation-id,conversations,projects,turns,messages}` | `RequireWrite` on the same prefixes |
| conveyor | `/api/v1` except `/api/v1/webhooks` | no `RequireWrite`: `authz.rs` checks `conveyor:<action>` or `conveyor:project:<id>:<action>` per route |
| workbench | `/api/v1` | per route `workbench:<action>` or `workbench:project:<id>:<action>` |
| gantry | `/api/v1` | `gantry:<action>` or `gantry:target:<package>:<action>` per route |
| gauge | `/api/v1` | `Auth` only; no per-action check in the service |
| daybook | `/api/v1` | `daybook:read` / `daybook:write` per route (`routers/authz.rs`); rows are always scoped to the caller's `sub` |
| hoist | `/v1` (catalogue) | `Auth` only: any valid identity with `hoist` in `aud`; health probes stay open |

The browser pages (`/ui/...`) check the session on their own and redirect to Gatehouse's `/api/v1/authorize` when there is none. `forge-sectest` keeps the list of routes that are public on purpose in `tests/forge-sectest/public-routes.txt`.

## Local settings

Under `foreman` every service gets `SERVICE_AUTH_ENABLED=true`, `GATEHOUSE_URL=https://localhost:5443/gatehouse`, `GATEHOUSE_CLIENT_ID=${name}` and `GATEHOUSE_TLS_VERIFY=false` (the dev certificate is issued for `registry.local`). See [Running the estate locally](running-locally.md). The Gantry local compose file sets `SERVICE_AUTH_ENABLED=false`, so every caller is "dev" and may do everything.
