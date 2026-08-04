# Gatehouse Service

Gatehouse is the Forge estate's authentication service: one identity store, one login page, one session. Sage, switchboard, warehouse and conveyor verify the Ed25519-signed tokens gatehouse issues and never see a password or hold a user table of their own. It replaced a per-service `sage.users`/`switchboard.users`/`warehouse.users` split with a single `auth.users`, a shared Redis session store, and one login form (`gatehouse/ui/login`) every other service redirects a browser to. Binary and crate name: `gatehouse-service` (`docker/gatehouse-service`).

## Features

- **Login, refresh, logout** over both a JSON API and OAuth2 authorization-code + PKCE, for relying parties that want their own scoped token rather than trusting a realm-wide cookie directly.
- **A permission catalog** (`config/permissions.toml`) — the estate's one place a service's grantable actions live: which services exist, what actions each supports, and named grant templates an admin can assign in one step. Replaces both an old `SERVICE_AUDIENCES` env var and a hardcoded two-value read/write enum.
- **Self-service registration and password reset**, gated behind an email-verification link (dev-only `LoggingSender` today — see [Configuration](#configuration)).
- **Admin UI** for creating/editing/deleting users, and a permission checkbox matrix driven entirely by the catalog above.
- **Ed25519 (EdDSA) token signing** with key rotation that retires rather than deletes an outgoing key, so tokens it already signed keep verifying until they expire.
- **Shared Redis sessions**, so a logout or a permission change ends a session at every service on its next request, not whenever an access token happens to expire.

## API routes

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/v1/auth/login` | credentials → access + refresh token |
| `POST` | `/api/v1/auth/refresh` | rotate a refresh token |
| `POST` | `/api/v1/auth/logout` | revoke a refresh token, realm-wide |
| `GET` | `/api/v1/auth/userinfo` | subject, roles, permissions and audiences of a token |
| `GET`/`POST`/`PATCH`/`DELETE` | `/api/v1/users[/{username}]` | user CRUD (admin) |
| `PUT` | `/api/v1/users/{username}/permissions` | replace a user's grants (admin) |
| `POST` | `/api/v1/users/{username}/template` | apply a named grant template (admin) |
| `GET` | `/api/v1/me` | what the caller may do, per service |
| `GET` | `/api/v1/authorize`, `POST /api/v1/token` | OAuth2 authorization-code + PKCE and `client_credentials` |
| `GET` | `/.well-known/jwks.json` | the realm's public signing keys |
| `POST` | `/api/v1/admin/keys/rotate` | rotate the signing key (`gatehouse:manage-signing-keys`) |
| `GET`/`POST` | `/ui/login`, `/ui/logout` | the estate's only login form |
| `GET`/`POST` | `/ui/register`, `GET /ui/verify` | self-service account creation + email verification |
| `GET`/`POST` | `/ui/forgot-password`, `/ui/reset-password` | password reset by emailed link |
| `GET` | `/ui/home` | every enabled service, front door after login |
| `GET`/`POST` | `/ui/admin/users[/{username}]` | user list, editor, permission matrix, template apply, delete (admin) |

`/ui/login` and `/ui/logout` accept `?redirect=`, which is where a relying party sends the browser back to after auth. Targets are validated to prevent an open redirect: a rooted same-origin path (starts with `/`, not `//` or `/\`) is always allowed, plus any prefix listed in `AUTH_REDIRECT_HOSTS`; protocol-relative `//host` targets are rejected outright. The check lives in `quench-auth` (`validated_redirect`), shared by every relying party that redirects to its own `/ui/login`.

## Architecture

### Permission catalog and audiences

`config/permissions.toml` declares `[services.*]` (label, grantable `actions`, and any `resource_types` for scoped grants like `conveyor:project:<id>:<action>`) and `[templates.*]` (named grant bundles). Gatehouse builds its JWT audience ceiling from the catalog's service list at startup — not from a `SERVICE_AUDIENCES` env var, which every *other* service in the estate still reads for its own single-service default. A token's audience is narrowed to only the services its holder was granted (`admin`/`service` roles are wildcards and get every audience); gatehouse itself is always included, since it serves the login page and refresh even for a user with no other grants.

A grant is `service:action` (e.g. `switchboard:launch`), enforced by whatever action a relying party's own routes declare via `Claims::can` — not a blanket read/write ladder, though a service that only needs a coarse split just lists `actions = ["read", "write"]` and gets the old two-level behaviour for free (`RequireWrite` in `quench-auth` checks specifically for `"write"`).

### Roles vs. permissions

Three roles: `admin`, `user`, `service`. `admin` and `service` are wildcards — every permission on every service, without it being written down — but only `admin` can manage users; `service` is a wildcard for *service access* only. Assigning `admin` or `service` is gated on already holding the literal `admin` role in gatehouse's own code, never on a catalog action, so no permission grant can hand out the power to grant admin. Two invariants enforced in `src/realm.rs` regardless of which surface (API or UI) is used: the realm must always keep at least one admin, and nobody can delete or demote themselves. Changing what someone may do ends their sessions immediately, so a permission change applies at once instead of waiting for the access token to expire — the exception is an admin changing only their own password.

The admin pages check the `admin` role themselves and answer with a plain 403 page rather than redirecting to login — bouncing an already-signed-in session to the login form would look like it had expired. Creating a user and granting it access are two separate steps (`POST /ui/admin/users` takes only a username, password and role and lands on that user's editor) rather than one combined submission, so a single mistake can't hand out estate-wide access to an account nobody has reviewed yet.

### Self-service registration and password reset

Both flows send a link through `email::Sender`; the only implementation wired in is `LoggingSender`, which writes the link to the process log instead of an inbox. That makes both flows testable without an SMTP relay, and is explicitly not safe for a deployment real users reach — anyone reading the logs can read the credential-equivalent link. A newly self-registered user starts with `registration.default_template` from the permission catalog (`viewer` by default); an admin-created user starts with nothing, since an admin is right there to grant it.

### Sessions

Sessions live in Redis, not Postgres:

```
session:{sid}    -> { username, refresh_hash }   TTL = refresh lifetime
refresh:{hash}   -> sid                          TTL = refresh lifetime
user:{username}  -> set of sids                  TTL = refresh lifetime
```

Expiry is the TTL, so nothing sweeps them; revocation is a `DEL`, so it takes effect at every service on its next request. Refresh-token rotation uses `GETDEL`, atomic on the server, so a token presented twice succeeds at most once. Redis is a hard dependency — gatehouse exits at startup if it is unreachable, and a relying party that cannot read a session treats it as invalid rather than assuming the best.

### The home page

After signing in, `/ui/home` lists the services this deployment offers, built from a small fixed table (`src/services.rs`) of four entries — `CONVEYOR`, `SAGE`, `SWITCHBOARD`, `WAREHOUSE` — each keyed by an environment-variable prefix. A service appears when its URL is configured (`{PREFIX}_UI_URL` or `{PREFIX}_URL`) and its feature flag is not turned off (`FEATURE_{PREFIX}_ENABLED`, default `true`), so the page reflects what actually runs rather than a hardcoded list; a deployment that runs only warehouse needs no flags at all. Adding a service to the estate is a new `ServiceDefinition` entry plus its i18n strings — no other code change. The UI itself is built from the same shell, theme and generated stylesheet as every other service: `quench-starter`'s shared CSS rule sets, `AppShellBuilder` with `Theme::DefaultDark`, and the same five locales; `dist/assets` is generated at startup, so there is nothing to build or commit.

### Signing and the redirect flow

Gatehouse holds its Ed25519 private key (`auth.signing_keys`, encrypted at rest with `GATEHOUSE_KEY_ENCRYPTION_KEY`) and publishes the public half at `/.well-known/jwks.json`; a relying party fetches and caches that JWKS to verify locally, so there is no call to gatehouse on the hot path. A relying party's `/ui/login` runs a real authorization-code + PKCE round trip rather than trusting a realm cookie directly, so each service ends up with a token scoped to itself that it fetched itself.

## Requirements

- Postgres for the `auth` schema (users, signing keys, OAuth clients, authorization codes) — installed via foundry's `auth`/`gatehouse` catalog modules, not by gatehouse itself.
- Redis/`REDIS_URL` (or `CACHE_URL`) for sessions — required on gatehouse *and* every relying party.
- `GATEHOUSE_KEY_ENCRYPTION_KEY` to encrypt signing keys at rest.
- `config/permissions.toml` must load successfully at startup, or the process refuses to start — a realm with no grantable services is treated as broken, not a smaller estate.

Every other service in the estate needs `GATEHOUSE_URL` in turn: there is no per-service login form any more, so sage, switchboard and warehouse serve `/ui/login`/`/ui/logout` as redirects here, and a service started without `GATEHOUSE_URL` answers those routes with `503 gatehouse is not configured` instead. Nobody can sign in until gatehouse itself is reachable.

## Configuration

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres holding the `auth` schema |
| `REDIS_URL` / `CACHE_URL` | shared session store |
| `GATEHOUSE_KEY_ENCRYPTION_KEY` | encrypts `auth.signing_keys` at rest |
| `CLIENTS_CONFIG` | OAuth client catalog, default `config/clients.toml` |
| `PERMISSIONS_CONFIG` | permission catalog, default `config/permissions.toml` |
| `AUTH_BOOTSTRAP` | defaults to `true` here, unset everywhere else |
| `SERVICE_USERNAME` / `SERVICE_PASSWORD` | the admin seeded on first boot |
| `AUTH_REDIRECT_HOSTS` | comma-separated relying-party origins allowed as `?redirect=` |
| `ACCESS_TOKEN_TTL_SECS` | access token lifetime (default 900) |
| `AUTH_DB_SCHEMA` | realm schema name, default `auth` |
| `AUTH_COOKIE_NAME` / `AUTH_REFRESH_COOKIE_NAME` | session/refresh cookie names, default `forge_session` / `forge_refresh` |
| `AUTH_COOKIE_DOMAIN` | parent domain for cross-subdomain SSO; unset means host-only |
| `{SAGE,SWITCHBOARD,WAREHOUSE,CONVEYOR}_UI_URL` / `..._URL` | per-service links shown on `/ui/home` |
| `FEATURE_{SAGE,SWITCHBOARD,WAREHOUSE,CONVEYOR}_ENABLED` | hide a configured service from `/ui/home` (default `true`) |

In local dev (`foreman.toml`) gatehouse runs on port 5443 under base path `/gatehouse` with no `needs` (it is the destination, not a client of anything). It is the one service `foreman` deliberately unsets `GATEHOUSE_URL`/`GATEHOUSE_CLIENT_ID` for.

## Testing

Inline `#[cfg(test)]` modules cover the permission catalog (`src/catalog.rs` — known services/actions, resource-scoped grants, template validation), audience narrowing (`src/api/auth.rs`), and the admin permission-matrix fold-in logic (`src/ui/pages/admin.rs`). Run with `cargo test -p gatehouse-service` or `foreman test gatehouse`.

[Home](../README.md)
