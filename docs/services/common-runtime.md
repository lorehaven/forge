---
title: Common service runtime
parent: Services
nav_order: 90
---

What every web service of the estate (sage, switchboard, workbench, gauge, and the others built on `quench-starter` and `quench-auth`) shares: the routes `quench-starter` adds, the SSO routes under `/ui`, the middleware, and the environment variables of the platform crates. The service pages link here instead of repeating it. Sources: `quench-starter` (`src/http/mod.rs`, `src/http/routers/{health,swagger,ui}.rs`, `src/http/middleware`, `src/common`, `src/logging.rs`), `quench-auth` 0.4.x (`domain/{jwt,realm,sso_client,session,jwks}.rs`, `http/middleware/auth.rs`, `http/routers/ui/pages/auth.rs`, `http/domain/sso_client.rs`), `quench-log`, `quench-cache` (Quench repository).

## Routes added by the starter

They are `#[get]` routes inside `quench-starter`, so they are not in a service's `openapi.yaml` (the documentation checker only reads a service's own sources). All are mounted under `BASE_PATH` and sit outside the `/api/v1` authentication wrapper.

| Method and path | Answer |
|---|---|
| `GET /health` | `200` text `OK`. |
| `GET /health/live` | `200` `{"status":"live"}`; `503` `{"status":"not_live"}` (never, `live` is set at start). |
| `GET /health/ready` | `200` `{"status":"ready"}` once the service's init future finished; `503` `{"status":"not_ready"}` before. Workbench and gauge mark ready after Gatehouse's `/health/ready` answers; sage also waits for Switchboard's, switchboard also for its first model scan (`wait_for_services` polls once a second, 2 s timeout, certificate errors accepted). |
| `GET /swagger-ui` | `308` to `{BASE_PATH}/swagger-ui/`. |
| `GET /swagger-ui/` | `308` to `{BASE_PATH}/swagger-ui/index.html`. No route serves that file; the target answers like any unknown path. |
| `GET {BASE_PATH}` and `GET {BASE_PATH}/` | `302` to `{BASE_PATH}/ui`. |
| `GET /` (when `BASE_PATH` is not `/`) | `302` to `{BASE_PATH}/ui`. |

## Middleware on every response

Outermost first: the filtered logger (logs `METHOD path -> status`, skipping success responses whose path starts with a prefix of `LOG_SKIP_PREFIXES`), the correlation id (reads or mints `x-correlation-id` and echoes it on the response), the security headers (`x-content-type-options: nosniff` unless the handler set one), then the router. Inside it, each service wraps `/api/v1` (and for sage and switchboard some sub-prefixes) with `quench_auth::http::middleware::auth::Auth`.

## Authentication of `/api/v1`

`Auth` takes the access token from `Authorization: Bearer <jwt>`, else from the session cookie. The JWT is verified against Gatehouse's JWKS (EdDSA, `kid` required), its audience must include the service (`SERVICE_NAME` or `SERVICE_AUDIENCES`), and when it carries a `sid` the session must still be active in the session store. An expired or missing access token with a refresh cookie present triggers a silent `POST {GATEHOUSE_URL}/api/v1/auth/refresh`; the response then carries two `Set-Cookie` headers. Failure: `401`, with a redirect instead for a browser request. With `SERVICE_AUTH_ENABLED=false` (the default) the middleware lets every request through. See [Authentication and permissions](../guides/authentication.md) for tokens, grants and the clients.

Error bodies built with `quench_starter::http::domain::api_error::json_error(status, message)` are JSON; each service page shows the exact shape it uses.

## SSO routes under `/ui`

Every browser service registers the same six routes (the handlers are `quench_auth::http::routers::ui::pages::auth`). The service page lists them with their `operationId`; semantics:

| Route | Behaviour |
|---|---|
| `GET /ui/login`, `GET /ui/login/` | If a refresh cookie is present and Gatehouse accepts it, `302` to `{BASE_PATH}/ui/home` with fresh session and refresh cookies. Otherwise `302` to `{GATEHOUSE_URL}/api/v1/authorize?client_id&redirect_uri&state&code_challenge&code_challenge_method=S256` (`redirect_uri` = `{scheme}://{host}{BASE_PATH}/ui/auth/callback`, scheme from `X-Forwarded-Proto` else `https`, host from `X-Forwarded-Host` else `Host`) and sets the cookie `forge_authorize_state` (JSON `{state, code_verifier, redirect}`, 5 minutes, `HttpOnly; Secure; SameSite=Lax`). `503` text `gatehouse is not configured` when `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID` or `GATEHOUSE_CLIENT_SECRET` is missing. |
| `GET /ui/auth/callback?code&state` | Checks the state cookie and the `state`, posts the code, `client_id`, `client_secret` and `code_verifier` form-encoded to `{GATEHOUSE_URL}/api/v1/token`, then `302` to the saved redirect (`{BASE_PATH}/ui/home`) with the session and refresh cookies and a cleared state cookie. Any failure logs `sso callback failed: <reason>` and answers `302` to `{BASE_PATH}/ui/login` with the state cookie cleared. |
| `GET /ui/logout` | `302` to `{GATEHOUSE_URL}/ui/logout?redirect=<absolute {BASE_PATH}/ui/login>`. `503` `gatehouse is not configured` without `GATEHOUSE_URL`. Does not clear this service's cookies itself. |
| `GET /ui/status` | Always `200` JSON `{"authenticated": bool, "username": string or null, "roles": [string]}`. With auth disabled: `{"authenticated":true,"username":"dev","roles":["admin"]}`. Reads the session cookie and verifies the signature only; `roles` is the `scope` claim split at `,` (not at spaces). |
| `POST /ui/refresh` | Takes the refresh cookie, calls Gatehouse's refresh, answers `200` `{"authenticated":true,"username":null,"roles":[]}` with two new cookies; `401` (empty body) without a cookie or when Gatehouse refuses. |

Cookies (`quench_auth::domain::realm`): session `forge_session` (env `AUTH_COOKIE_NAME`), refresh `forge_refresh` (`AUTH_REFRESH_COOKIE_NAME`), both `Path=/; HttpOnly; Secure; SameSite=Lax`, the refresh one with `Max-Age` = `REFRESH_TOKEN_TTL_SECS`; `Domain` from `AUTH_COOKIE_DOMAIN` when set.

`?redirect=` targets (used by Gatehouse's own login page) are validated by `validated_redirect`: a rooted path (not `//`, not `/\`) or a prefix listed in `AUTH_REDIRECT_HOSTS`.

## Page and asset routes of a browser client

`forge-web::Client` serves the page for the service's page routes and five asset routes (`/ui/assets/quench.js`, `app.css`, `shell.js`, `favicon.png`, `app.wasm`). Their behaviour, headers and the `WEB_DIR` variable are in [The browser-client pattern](../guides/browser-client.md). A page route redirects to `{BASE_PATH}/ui/login` (`302`) without a valid session cookie; asset routes need no session.

## Environment variables of the platform crates

Read by every service that uses them; a service page lists only what is specific to it and says which of these it uses.

| Variable | Default | Read by | Meaning |
|---|---|---|---|
| `BASE_PATH` | `/` | starter, `realm` | Mount prefix (normalised: leading `/`, no trailing `/`). |
| `SERVER_ADDR` | `0.0.0.0:443` | starter | HTTPS (or plain HTTP) listen address. Invalid value panics at start. |
| `SERVER_HTTP_REDIRECT_ADDR` | `0.0.0.0:80` | starter | Listener that redirects to HTTPS (only when TLS is on). |
| `SERVER_CERT_PATH`, `SERVER_KEY_PATH` | `cert.pem`, `key.pem` | starter | PEM files. If they cannot be loaded the service serves plain HTTP on `SERVER_ADDR` (status line "starting plain HTTP server"). |
| `SERVICE_NAME` | `service` | `JwtConfig`, starter | The audience checked and the service's name. |
| `SERVICE_AUDIENCES` | empty (then `SERVICE_NAME`) | `JwtConfig` | Comma list of accepted audiences. |
| `SERVICE_AUTH_ENABLED` | `false` | `JwtConfig` | `true` turns authentication on; anything unparsable is `false`. |
| `SERVICE_REALM` | `https://localhost:8698/token` | `JwtConfig` | Realm string (not used for verification). |
| `ACCESS_TOKEN_TTL_SECS` | `900` | `JwtConfig` | |
| `REFRESH_TOKEN_TTL_SECS` | `604800` | `JwtConfig`, `realm` | |
| `GATEHOUSE_URL` | empty | `jwks`, `realm`, `wait` | Gatehouse origin including its base path (JWKS at `{GATEHOUSE_URL}/.well-known/jwks.json`; readiness at `/health/ready`). |
| `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET` | empty | `SsoConfig` | The service's OAuth client. Secret: never log or document the value. |
| `GATEHOUSE_TLS_VERIFY` | `true` | `jwks`, `SsoConfig`, refresh | `false` accepts invalid certificates when talking to Gatehouse. |
| `AUTH_DB_SCHEMA` | `auth` | `realm` | Realm schema name. |
| `AUTH_COOKIE_NAME`, `AUTH_REFRESH_COOKIE_NAME` | `forge_session`, `forge_refresh` | `realm` | |
| `AUTH_COOKIE_DOMAIN` | empty | `realm` | Cookie `Domain`. |
| `AUTH_REDIRECT_HOSTS` | empty | `ui/pages/auth` | Allowed absolute redirect prefixes. |
| `REDIS_URL` / `CACHE_URL` | none | `SessionDb::from_env` (`CacheStore::from_env("forge-session")`) | Session store: Redis when set, otherwise in-process. `CACHE_CLUSTER`/`REDIS_CLUSTER=true` forces cluster mode. Session keys: `session:{sid}`, `refresh:{hash}`, `user:{username}`, `reuse:{hash}`. |
| `DATABASE_URL`, `POSTGRES_URL` | empty | `common::db` (and service code) | Postgres URL. |
| `RUST_LOG` | `info` | `quench-log` | Log level. |
| `LOG_CONSOLE_ENABLED`, `LOG_FILE_ENABLED` | see `quench-log` | `quench-log` | `1/true/yes/on` or `0/false/no/off`. |
| `LOG_DIR`, `LOG_FILE_PREFIX`, `LOG_FILE_FORMAT` (`json`/`plain`), `LOG_ROTATION` (`daily`/`hourly`/`never`), `LOG_MAX_FILES` (default 7) | see `quench-log` | `quench-log` | File logging. |
| `LOG_SKIP_PREFIXES` | empty | starter logger | Comma list of path prefixes whose successful requests are not logged. |
| `NO_COLOR` | unset | `quench-log` | Disables colours on a terminal. |

`quench_starter::logging::init()` also loads the first `.env` found walking up from the working directory (variables already in the environment win). Never read or document the contents of those files.
