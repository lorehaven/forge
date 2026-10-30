---
title: Configuration
parent: Gatehouse
grand_parent: Services
nav_order: 2
---

All settings are environment variables read at start (unless noted), plus two TOML files. Secrets are named, never shown.

## Server and process

| Variable | Default | Read in | Meaning |
|---|---|---|---|
| `BASE_PATH` | `/` | `main.rs`, `quench-starter` | path prefix everything is mounted under (normalised) |
| `SERVER_ADDR` | `0.0.0.0:443` | `quench-starter` | listen address; plain HTTP here when no certificate is found |
| `SERVER_HTTP_REDIRECT_ADDR` | `0.0.0.0:80` | `quench-starter` | HTTP to HTTPS redirect listener (only with TLS) |
| `SERVER_CERT_PATH` / `SERVER_KEY_PATH` | `cert.pem` / `key.pem` | `main.rs`, `quench-starter` | TLS certificate and key; used when both load |
| `SERVICE_NAME` | `gatehouse` (set into the environment at start) | `main.rs` | the token audience name of this service |
| `RUST_LOG` | `info` | `quench-starter` logging | log level |
| `LOG_DIR`, `LOG_FILE_PREFIX` | | `quench-starter` logging | where log files go (the cluster sets `/logs`, `gatehouse`) |
| `LOG_SKIP_PREFIXES` | empty | request logger | comma-separated path prefixes not logged |

## Database and cache

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` (or `POSTGRES_URL`) | none | Postgres holding the realm; an empty value stops the process unless `ALLOW_IN_MEMORY_DB=true` (tests only) |
| `DB_POOL_MAX_SIZE` | `5` | connection pool size |
| `DB_MIGRATION_TABLE` | `quench_migrations` | quench-db's own migration bookkeeping table (Gatehouse itself does not run migrations: foundry does) |
| `AUTH_DB_SCHEMA` | `auth` | schema of `users`, `signing_keys`, `clients`, `authorization_codes`, `notification_prefs`, `app_tokens` |
| `REDIS_URL` / `CACHE_URL` | none (in-process) | the cache store for sessions, mail tokens and rate limits. Comma-separated URLs mean cluster seeds; `CACHE_CLUSTER=true` or `REDIS_CLUSTER=true` forces cluster mode |

## Tokens and sessions

| Variable | Default | Meaning |
|---|---|---|
| `SERVICE_AUTH_ENABLED` | `false` | the estate's development switch. When false the user API serves every caller as an anonymous admin and the page API uses the stand-in admin `admin`. **Production sets it true** |
| `ACCESS_TOKEN_TTL_SECS` | `900` | access token lifetime; also how long a rotated-out signing key stays published |
| `REFRESH_TOKEN_TTL_SECS` | `604800` | refresh token and session lifetime; the refresh cookie's Max-Age |
| `SERVICE_REALM` | `https://localhost:8698/token` | stored as `JwtConfig.realm`; nothing in Gatehouse's routes reads it |
| `SERVICE_AUDIENCES` | derived | **ignored** by Gatehouse: its audience list is the permission catalog's service names plus its own name |
| `GATEHOUSE_KEY_ENCRYPTION_KEY` | none, **required** | any string; SHA-256 of it is the ChaCha20-Poly1305 key for stored signing keys and MFA secrets and the HMAC key of the pending-MFA token. Changing it makes stored keys undecryptable and the process panics at start |
| `AUTH_COOKIE_NAME` | `forge_session` | session cookie name |
| `AUTH_REFRESH_COOKIE_NAME` | `forge_refresh` | refresh cookie name |
| `AUTH_COOKIE_DOMAIN` | unset (host-only) | parent domain for single sign-on across subdomains |
| `AUTH_REDIRECT_HOSTS` | empty | comma-separated origins allowed as `?redirect=` targets besides same-origin paths |
| `GATEHOUSE_LOGIN_MAX_ATTEMPTS` | `5` | wrong passwords (or codes) before a lock |
| `GATEHOUSE_LOCKOUT_DURATION_SECS` | `900` | how long the lock lasts |
| `GATEHOUSE_TEST_MODE` | unset | `true` enables `POST /api/v1/test/token`; never set by the binary |

## Bootstrap

| Variable | Default | Meaning |
|---|---|---|
| `AUTH_BOOTSTRAP` | `true` (set into the environment by `main`) | seed accounts at start |
| `SERVICE_USERNAME`, `SERVICE_PASSWORD` | **required when bootstrapping** | the admin account; ignored when the user exists |
| `SERVICE_TECH_USERNAME`, `SERVICE_TECH_PASSWORD` | empty | optional `service` account (a warning is logged when missing) |

## OAuth clients and catalog

| Variable | Default | Meaning |
|---|---|---|
| `CLIENTS_CONFIG` | `config/clients.toml` | client catalog, see [permissions](permissions.md#oauth-clients); a load failure is logged and clients are not seeded |
| `CLIENT_SECRET_<ID>` | unset | one variable per client (the name is the client's `secret_env`); unset means the client is skipped |
| `PERMISSIONS_CONFIG` | `config/permissions.toml` | permission catalog; failure stops the process |
| `<NAME>_UI_URL`, `<NAME>_URL` | unset | where a dashboard service lives (`NAME` = upper-cased service name); also the base of its OAuth redirect URI |
| `FEATURE_<NAME>_ENABLED` | `true` | `1/true/yes/on` or `0/false/no/off` hides a configured service from the dashboard |

## Links and mail

| Variable | Default | Meaning |
|---|---|---|
| `PUBLIC_BASE_URL` | unset | origin (scheme, host, port; no path) emailed links point at; else the origin of `GATEHOUSE_URL`; else `http://localhost:5443`. Must be `http` or `https` without userinfo |
| `GATEHOUSE_URL` | unset | Gatehouse including its base path (`https://host/gatehouse`); here only the fallback for `PUBLIC_BASE_URL` and the origin the notification links must live under |
| `SMTP_HOST` | unset | unset: links are logged, not mailed |
| `SMTP_PORT` | by security mode (*Unverified:* the defaults 465 implicit, 587 starttls, 25 none come from `quench-mail`, which was not read) | a port number |
| `SMTP_TLS` | `implicit` | `implicit`, `starttls` or `none` (case-insensitive); anything else stops the process |
| `SMTP_TLS_SERVER_NAME` | `SMTP_HOST` | certificate name to validate |
| `SMTP_USERNAME`, `SMTP_PASSWORD` | unset | set both or neither |
| `MAIL_FROM` | **required with `SMTP_HOST`** | a valid address |
| `MAIL_FROM_NAME` | `Forge` | display name; set blank for none |
| `SMTP_TIMEOUT_SECS` | `15` | per-step timeout, a positive number; one delivery is capped at three times this |
| `MAIL_DAILY_LIMIT` | `250` | emails per UTC day for the whole estate, positive; over it sends fail (transient) |

Invalid mail settings (including `MAIL_DAILY_LIMIT`) are a start-up failure.

## Files

| File | Purpose |
|---|---|
| `config/permissions.toml` | permission catalog and templates ([permissions](permissions.md)) |
| `config/clients.toml` | OAuth clients ([permissions](permissions.md#oauth-clients)) |
| `i18n/<locale>.ftl` | translations of the five locales (`en-US`, `pl-PL`, `es-ES`, `de-DE`, `fr-FR`; 252 lines each), compiled into the browser module; mail texts are in the source |
| `web/app.wasm` | the browser client, built apart and placed in `WEB_DIR` (default `web`) |

`WEB_DIR` (default `web`) is read by `forge-web` for `app.wasm`.

## Secrets (names only)

`GATEHOUSE_KEY_ENCRYPTION_KEY`, `SERVICE_PASSWORD`, `SERVICE_TECH_PASSWORD`, `SMTP_PASSWORD`, every `CLIENT_SECRET_*`. In the cluster they come from the `gatehouse-secret` Secret; locally foreman reads them from `.env` files or defaults.

## Feature flags

Only `FEATURE_<NAME>_ENABLED` (dashboard cards). The crate has no Cargo features.
