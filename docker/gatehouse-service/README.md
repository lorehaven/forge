# gatehouse-service

The authentication service for the Forge estate. One identity store, one login
page, one session — sage, switchboard and warehouse verify the tokens it issues
and never see a password.

## What moved here

| | before | now |
|---|---|---|
| Users | `sage.users`, `switchboard.users`, `warehouse.users` | `auth.users` |
| Sessions | one table per service schema | Redis, keyed by session id |
| Login form | one per service | `gatehouse/ui/login` |
| Admin bootstrap | every service, from its own `.env` | gatehouse only (`AUTH_BOOTSTRAP=true`) |
| Cookie | `{service}_ui_session`, `SameSite=Strict` | `forge_session`, `SameSite=Lax`, realm-wide |
| Token audience | `service` claim, one service | `aud` list covering the realm |

Relying parties still verify locally — signature, expiry and audience — so there
is no call to gatehouse on the hot path. They redirect a browser here only when
there is no valid session.

## Gatehouse is required

Every service in the estate needs `GATEHOUSE_URL`. There is no per-service login
form any more: sage, switchboard and warehouse serve `/ui/login` and
`/ui/logout` as redirects here, and a service started without `GATEHOUSE_URL`
answers those routes with `503 gatehouse is not configured` and logs why. Nobody
can sign in until gatehouse is reachable.

That is also why gatehouse is the only service that seeds accounts
(`AUTH_BOOTSTRAP`): a relying party can read the realm's users — it needs to for
Basic-auth service-to-service calls — but it can never create one.

## API

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/api/v1/auth/login` | credentials → access + refresh token |
| `POST` | `/api/v1/auth/refresh` | rotate a refresh token (body or cookie) |
| `POST` | `/api/v1/auth/logout` | revoke a refresh token — realm-wide |
| `GET` | `/api/v1/auth/userinfo` | subject, roles and audiences of a token |
| `GET` | `/ui/home` | the estate's front door: every enabled service |
| `GET`/`POST` | `/ui/login` | the only login form in the estate |
| `GET` | `/ui/logout` | global logout |

`/ui/login` and `/ui/logout` accept `?redirect=`, which is where a relying party
sends the browser back to. Targets are validated: rooted same-origin paths
always, plus any prefix listed in `AUTH_REDIRECT_HOSTS`. Protocol-relative
`//host` targets are rejected.

## The home page

After signing in you land on `/ui/home`, which lists the services this
deployment offers. A service appears when its URL is set and its feature flag is
not turned off, so the page reflects what actually runs rather than a hardcoded
list:

| Variable | Purpose |
|---|---|
| `SAGE_UI_URL` / `SAGE_URL` | where to send someone choosing sage |
| `SWITCHBOARD_UI_URL` / `SWITCHBOARD_URL` | same for switchboard |
| `WAREHOUSE_UI_URL` / `WAREHOUSE_URL` | same for warehouse |
| `FEATURE_SAGE_ENABLED` | defaults to `true`; set `false` to hide a configured service |
| `FEATURE_SWITCHBOARD_ENABLED` | as above |
| `FEATURE_WAREHOUSE_ENABLED` | as above |

A service with no URL configured is never listed, so a deployment that runs only
warehouse needs no flags at all. Adding a service to the estate is a
`ServiceDefinition` entry in `src/services.rs` plus its i18n strings.

The UI is built from the same shell, theme and generated stylesheet as every
other service: `quench-starter`'s shared CSS rule sets, `AppShellBuilder` with
`Theme::DefaultDark`, and the same five locales. `dist/assets` is generated at
startup, so there is nothing to build or commit.

## Configuration

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | Postgres holding the `auth` schema (users) |
| `REDIS_URL` / `CACHE_URL` | shared session store; required on every service |
| `JWT_SECRET` | HS256 signing key — **the same value in every service** |
| `SERVICE_AUDIENCES` | services a gatehouse token is valid for, e.g. `sage,switchboard,warehouse` |
| `AUTH_BOOTSTRAP` | defaults to `true` here, unset everywhere else |
| `SERVICE_USERNAME` / `SERVICE_PASSWORD` | the admin created on first boot |
| `SERVICE_TECH_USERNAME` / `SERVICE_TECH_PASSWORD` | machine-to-machine account (was switchboard's) |
| `AUTH_DB_SCHEMA` | realm schema, default `auth` |
| `AUTH_COOKIE_NAME` / `AUTH_REFRESH_COOKIE_NAME` | default `forge_session` / `forge_refresh` |
| `AUTH_COOKIE_DOMAIN` | parent domain for cross-subdomain SSO; unset = host-only |
| `AUTH_REDIRECT_HOSTS` | comma-separated relying-party origins allowed as `?redirect=` |

Relying parties set `GATEHOUSE_URL` (this service's base URL), `REDIS_URL`, and
the same `JWT_SECRET`, `AUTH_COOKIE_*` and `SERVICE_NAME`.

## Sessions

Sessions live in the cache store, not the database:

```
session:{sid}    -> { username, refresh_hash }   TTL = refresh lifetime
refresh:{hash}   -> sid                          TTL = refresh lifetime
```

Expiry is the TTL, so nothing sweeps them. Revocation is a delete, so a logout
takes effect at every service on its next request rather than whenever the
access token happens to expire. Rotation consumes the old refresh token with
`GETDEL`, which is atomic on the server - a token presented twice succeeds at
most once, whether that is a racing client or a replayed steal.

Set `REDIS_URL` (or `CACHE_URL`) on **every** service, not just gatehouse: the
relying parties read the same store to decide whether a session is still alive.
Without it each process keeps sessions in its own memory, which is fine for a
single-process dev run and wrong for anything else - services log a warning
saying exactly that.

A cluster works too: pass a comma-separated list of seed nodes, or set
`CACHE_CLUSTER=true` if you only have one address to reach it by. Every session
operation is single-key - `GET`, `SET EX`, `GETDEL`, `DEL` - so keys are free to
land wherever their slot puts them, and `GETDEL` stays atomic on the node that
owns the key. Each service logs which topology it connected to at startup.

Redis is a hard dependency, and deliberately so: gatehouse exits at startup if
the store is unreachable, and a relying party that cannot read a session treats
it as invalid rather than assuming the best. An outage means nobody can sign in;
it does not mean everybody is trusted.

There is deliberately **no session listing and no audit trail**. If you later
want "sign out my other devices", or "when was this access revoked", both need
durable per-user records and belong back in Postgres.

## Database

Schema comes from the foundry catalog's `auth` module, not from this service:

```bash
foundry-service --install gatehouse apply
```

That creates `auth.users`, the realm's only table. Bootstrap then seeds
`SERVICE_USERNAME` / `SERVICE_PASSWORD` and the machine identity
`SERVICE_TECH_USERNAME` / `SERVICE_TECH_PASSWORD`, and never overwrites a user
that already exists — so on a second boot the passwords in the environment are
ignored.

There are no sessions in Postgres, so there is nothing to migrate: everyone
signs in once against the new realm.

## Not yet here

Deliberately staged (see `docs/SSO_PLAN.md` Phase 2): asymmetric EdDSA signing
with JWKS, the authorization-code + PKCE redirect flow, and the
`client_credentials` grant for machine-to-machine. Today the realm shares one
HS256 secret and one cookie, which requires all services to sit under a common
parent domain.
