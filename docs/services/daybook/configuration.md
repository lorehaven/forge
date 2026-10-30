---
title: Configuration
parent: Daybook
grand_parent: Services
nav_order: 2
---

The service reads its environment through `quench_core::env` (and, for the shared parts, `quench-starter`, `quench-auth`, `quench-db`). The variables below are those read by the code paths `daybook-service` runs; values for the cluster are in the `homecloud` overlay `overlays/forge/daybook/config.yaml.j2` (names only here).

## Environment

| Variable | Default | Read by | Meaning |
|---|---|---|---|
| `BASE_PATH` | `/` | `main.rs`, `quench-starter` | URL prefix of everything (`/daybook` in foreman and the cluster). Normalised by `normalize_base_path`. |
| `DATABASE_URL` (or `POSTGRES_URL`) | none; startup fails without | `DbWrapper::init_env` | Postgres connection URL. |
| `DB_SCHEMA` | `daybook` | `domain::db::schema()`, on every query | Schema the tables live in. The foundry module must have been installed into the same schema. |
| `DB_POOL_MAX_SIZE` | quench-db default | `quench-db` | Pool size. |
| `GATEHOUSE_URL` | empty | `JwtConfig::init` (JWKS), `gatehouse_health_url`, `signin::forward`, `forge_web` | Gatehouse's address including its base path (`https://host/gatehouse`). Needed for JWKS, readiness and sign-in forwarding. |
| `GATEHOUSE_TLS_VERIFY` | `true` | `signin.rs` | `false` accepts a self-signed gatehouse certificate for the forwarded `/auth/*` calls. |
| `SERVICE_AUTH_ENABLED` | `false` | `JwtConfig` | When false `Auth` does nothing and `authz::can` returns true; the actor is the literal `dev`. Set `true` anywhere real. |
| `SERVICE_NAME` | `service` | `JwtConfig` | The service's name (audience check). `daybook` in the cluster. |
| `SERVICE_REALM` | `https://localhost:8698/token` | `JwtConfig` | Token issuer realm. |
| `SERVICE_AUDIENCES` | empty | `JwtConfig` | Audiences accepted. |
| `AUTH_DB_SCHEMA` | `auth` | `quench-auth` | Gatehouse's schema name (shared setting). |
| `AUTH_COOKIE_NAME`, `AUTH_REFRESH_COOKIE_NAME`, `AUTH_COOKIE_DOMAIN`, `ACCESS_TOKEN_TTL_SECS` (900), `REFRESH_TOKEN_TTL_SECS` (604800) | see `quench-auth` | `quench-auth` | Cookie names and lifetimes shared with gatehouse. |
| `REDIS_URL` (or `CACHE_URL`) | unset | `SessionDb::from_env` | Session store. When unset the store is in-process (a warning is logged; sessions are lost on restart and are not shared). `Auth` rejects a token whose session id (`sid`) is not active there. |
| `SERVER_ADDR` | `0.0.0.0:443` | `quench-starter` | Listen address. Foreman sets the port 12443. |
| `SERVER_HTTP_REDIRECT_ADDR` | `0.0.0.0:80` | `quench-starter` | Plain-HTTP redirect listener (foreman: `0.0.0.0:12080`). |
| `SERVER_CERT_PATH`, `SERVER_KEY_PATH` | `cert.pem`, `key.pem` | `quench-starter` | TLS files. |
| `WEB_DIR` | `web` | `forge_web::Client::module` | Directory holding `app.wasm`, relative to the working directory. |
| `LOG_SKIP_PREFIXES` | empty | `quench-starter` | Request-log filter. |
| `LOG_DIR`, `LOG_FILE_PREFIX`, `RUST_LOG` | see `quench-log` | `quench-log` | Logging (the overlay sets `/logs`, `daybook`). |

The client programs read `DAYBOOK_TRACE` (set to trace gatehouse calls, via `gatehouse_client::trace_var`) and `XDG_DATA_HOME` / `HOME` (data directory), see [Data](data.md).

Feature flags (`FEATURE_*`): none.

## Gatehouse registration

Gatehouse must know the service: its permission catalog (`services/gatehouse/service/config/permissions.toml`) lists the `daybook` actions `read` and `write`, and the realm audience list is derived from it (see [Authentication and permissions](../../guides/authentication.md)). Gatehouse's `DAYBOOK_UI_URL` (foreman: `https://localhost:12443/daybook/ui`) is the dashboard link.

## Secrets

* `DATABASE_URL` (the overlay keeps it in `daybook-secret`; it is the only key in that secret).

The service has no OAuth client of its own.

## Files

* `service/config/` and `service/i18n/` contain only `.gitkeep`: the service has no config files and no translations.
* `service/web/app.wasm` (git-ignored build output of `scripts/web.sh`).
