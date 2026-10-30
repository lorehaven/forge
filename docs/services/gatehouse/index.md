---
title: Gatehouse
parent: Services
has_children: true
permalink: /services/gatehouse/
nav_order: 6
---

Gatehouse is the Forge estate's authentication service: one identity store (`auth.users`), one login, one session. It issues Ed25519-signed access tokens; every other service verifies them locally against Gatehouse's JWKS and never sees a password. It also hosts the account pages (profile, password, address, MFA, notification choices), the user administration, and the mail the estate sends to people.

## Crates

| Crate | Path | Version | Role |
|---|---|---|---|
| `gatehouse-service` | `services/gatehouse/service` | 0.4.2 | the server (binary `gatehouse-service`, library `gatehouse_service`) |
| `gatehouse-core` | `services/gatehouse/core` | 0.1.0 | the wire model of the page API, notices and form drafts; shared by service and client |
| `gatehouse-app` | `services/gatehouse/app` | 0.1.0 | the screens (no I/O): sign-in, dashboard, account, administration |
| `gatehouse-web` | `services/gatehouse/web` | 0.1.0 | the browser module (`cdylib`) that runs the app over `fetch` |

## Where it runs

| | |
|---|---|
| Local (foreman) | package `gatehouse-service`, port `5443`, `base_path = "/gatehouse"`, `needs = []`; HTTP redirect listener on `5080`; borrows warehouse's dev certificate. Foreman unsets `GATEHOUSE_URL` and `GATEHOUSE_CLIENT_ID` for it |
| Cluster | `forge` overlay of `homecloud`: Deployment `gatehouse`, image `ossiriand.arda:8080/forge/gatehouse/gatehouse:<version>`, Service port 80, ingress path `/gatehouse` on `ennor.ddns.net`; see [operations](operations.md) |
| Base path | `BASE_PATH` (default `/`; `/gatehouse` in both setups above) |
| Listeners | `SERVER_ADDR` (default `0.0.0.0:443`) with TLS when `SERVER_CERT_PATH`/`SERVER_KEY_PATH` (default `cert.pem`/`key.pem`) exist, plus an HTTP to HTTPS redirect on `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`); otherwise plain HTTP on `SERVER_ADDR` |

## Dependencies

* **Postgres**: schema `auth` (`AUTH_DB_SCHEMA`), installed by foundry modules `auth` and `gatehouse`; see [data](data.md). Required (`DATABASE_URL`).
* **Cache store** (`REDIS_URL` or `CACHE_URL`): sessions, mail/verification tokens and rate-limit counters. Without a URL an in-process store is used (single instance only, lost on restart).
* **SMTP** (optional): without `SMTP_HOST` mail links are written to the log.
* No other service is called. Other services call Gatehouse: JWKS, the token endpoint, `/ui/login`, `/api/v1/notify`.

## Authentication model

Described in [authentication](authentication.md): password login with lockout, optional TOTP second factor, access tokens (EdDSA JWT, 15 minutes by default) plus rotating refresh tokens kept in the cache store, OAuth authorization code with PKCE for relying parties, `client_credentials` for machine identities, and application tokens (`lht_...`) for command-line tools. Permissions are `service:action` grants from a catalog; see [permissions](permissions.md).

## Pages of this section

* [API](api.md) and [openapi.yaml](openapi.yaml): all 85 routes.
* [Authentication](authentication.md): tokens, cookies, sessions, flows, MFA, application tokens.
* [Permissions](permissions.md): roles, catalog, templates, resource-scoped grants, OAuth clients.
* [Notifications](notifications.md): the notification catalog and mail.
* [Configuration](configuration.md): every environment variable and config file.
* [Data](data.md): tables, migrations, cache keys.
* [Operations](operations.md): running, probes, limits, failure modes.
* [Client](client.md): the browser client.

Libraries used by tools to talk to Gatehouse: `gatehouse-auth` (CLI tokens) and `gatehouse-client` (native app sessions) in the [Libraries](../../libs/index.md) section.
