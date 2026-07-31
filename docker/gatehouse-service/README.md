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
| `GET` | `/api/v1/auth/userinfo` | subject, roles, permissions and audiences of a token |
| `GET` | `/api/v1/users` | list the realm's users (admin) |
| `POST` | `/api/v1/users` | create a user (admin) |
| `GET` | `/api/v1/users/{username}` | one user (admin) |
| `PATCH` | `/api/v1/users/{username}` | change password, roles or permissions (admin) |
| `DELETE` | `/api/v1/users/{username}` | remove a user and end their sessions (admin) |
| `PUT` | `/api/v1/users/{username}/permissions` | replace a user's grants (admin) |
| `GET` | `/api/v1/me` | what the caller may do, per service |
| `GET` | `/ui/home` | the estate's front door: every enabled service |
| `GET`/`POST` | `/ui/login` | the only login form in the estate |
| `GET` | `/ui/logout` | global logout |
| `GET`/`POST` | `/ui/admin/users` | the user list, and the form that adds one (admin) |
| `GET`/`POST` | `/ui/admin/users/{username}` | roles, the access matrix and a new password (admin) |
| `POST` | `/ui/admin/users/{username}/delete` | remove a user (admin) |

The user routes accept a bearer token or the realm session cookie, and answer 403
to anyone without the `admin` role. `/api/v1/me` is the exception: any
authenticated caller may ask what they themselves may do.

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

## Users, roles and permissions

Three roles — `admin`, `user`, `service` — and, for an ordinary `user`, a grant
per service at one of two levels:

```json
{ "sage": "write", "warehouse": "read" }
```

`write` implies `read`. `admin` and `service` are **wildcards**: they hold every
permission on every service without any of it being written down, so adding a
service to the estate never means re-granting anything. Their `permissions` column
stays empty and their token's scope is just `admin` or `service`.

Roles administer; permissions grant access. `service` is a wildcard for *service
access* only — the machine-to-machine account has to reach whatever the estate
runs — but it cannot manage users. That is `admin` alone.

**Access to a service enforces itself.** A token's audience list is narrowed to
the services its holder was granted, so the audience check every relying party
already performs is what refuses a user with no grant. No relying party needs to
know permissions exist:

```
admin        aud = [sage, switchboard, warehouse, gatehouse, conveyor]  scope = "admin"
sage:read    aud = [sage, gatehouse]                                    scope = "user sage:read"
no grants    aud = [gatehouse]                                          scope = "user"
```

Gatehouse is always in the list — it serves the login page, the home page and
refresh, so a token that excluded it would leave the holder unable to reach the one
service that could grant them anything. `SERVICE_AUDIENCES` remains the ceiling: a
grant naming a service this deployment does not run is ignored at issue time and
rejected at grant time.

The `read`/`write` distinction is carried in the token but not yet enforced by the
relying parties — that is `docs/PERMISSIONS_PLAN.md` Phase D. Today a grant of
either level means the service is reachable.

Changing what someone may do ends their sessions, so the new answer applies at
once rather than whenever their access token expires. The exception is an admin
changing only their own password.

Two rules that cannot be broken from either surface: the realm must keep at least
one admin, and you cannot delete or demote yourself. Without them one mistaken
edit locks everybody out of the estate, recoverable only by SQL.

Both the API and the pages go through `src/realm.rs`, so the rules are enforced
once. A change to what an edit is allowed to do belongs there and nowhere else.

## The admin pages

`/ui/admin/users` lists the realm and adds to it;
`/ui/admin/users/{username}` is where roles, access and passwords are set.
Plain server-rendered forms with a POST and a redirect, like the login page — the
one surface that gets an administrator back into a locked-out estate does not
depend on JavaScript.

The access matrix is built from `SERVICE_AUDIENCES`, one three-way control per
service (no access / read / read and write). For a wildcard role it renders as
full write, disabled, with a note saying the role already grants it — an admin
whose matrix showed no access would read as a bug.

Creating and granting are two steps: the create form takes a username, a password
and a role, then lands on that user's editor. A form that created *and* granted in
one submission would be one mistake away from handing out the estate to an account
nobody has looked at yet.

The pages check the `admin` role themselves. No session redirects to the login
form; a session without the role gets a 403 page, because bouncing a signed-in
user to a login form looks like their session expired. The link on `/ui/home` is
only rendered for admins, which is cosmetic — the check that matters is on the
page and on the API behind it.

## Sessions

Sessions live in the cache store, not the database:

```
session:{sid}    -> { username, refresh_hash }   TTL = refresh lifetime
refresh:{hash}   -> sid                          TTL = refresh lifetime
user:{username}  -> set of sids                  TTL = refresh lifetime
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

The third key is an index, and the only one not reachable from a token. It exists
so "end every session this user holds" is expressible, which is what makes a
permission change take effect immediately. It may hold ids whose sessions have
already expired — revocation skips what is gone — but it must never miss one,
which is why it is a set rather than a JSON array read and written back.

There is still deliberately **no session listing UI and no audit trail**. The
index would support "sign out my other devices"; "who granted this, and when"
would not — that needs durable per-grant records in Postgres, sketched in
`docs/PERMISSIONS_PLAN.md` §5.

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
