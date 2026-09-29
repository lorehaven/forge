# Gatehouse Service

Gatehouse is the Forge estate's authentication service: one identity store, one login page, one session. Sage, switchboard, warehouse, conveyor and workbench verify the Ed25519-signed tokens gatehouse issues and never see a password or hold a user table of their own. It replaced a per-service `sage.users`/`switchboard.users`/`warehouse.users` split with a single `auth.users`, a shared Redis session store, and one login form (`gatehouse/ui/login`) every other service redirects a browser to. Binary and crate name: `gatehouse-service` (`docker/gatehouse-service`).

## Features

- **Login, refresh, logout** over both a JSON API and OAuth2 authorization-code + PKCE, for relying parties that want their own scoped token rather than trusting a realm-wide cookie directly.
- **A permission catalog** (`config/permissions.toml`) — the estate's one place a service's grantable actions live: which services exist, what actions each supports, and named grant templates an admin can assign in one step. Replaces both an old `SERVICE_AUDIENCES` env var and a hardcoded two-value read/write enum.
- **Self-service registration and password reset**, gated behind an email-verification link, delivered by SMTP (`quench-mail`) or, when no mail server is configured, written to the log — see [Configuration](#configuration).
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
| `GET`/`POST` | `/ui/unsubscribe` | end one kind of notification from the link in its email (public; opening only asks, pressing - or a mail provider's one-click POST - does it) |
| `POST` | `/ui/account/notifications` | choose which kinds of service notification you get |
| `POST` | `/api/v1/notify` | a service asks gatehouse to notify a person (service token or `gatehouse:notify`) |
| `GET`/`POST` | `/ui/accept-invite` | the page an administrator's invitation opens: choose a password, which also confirms the address |
| `GET`/`POST` | `/ui/confirm-email` | confirm a new email address (GET shows the change, POST makes it) |
| `POST` | `/ui/account/email` | ask to change your own address (needs the current password) |
| `GET`/`POST` | `/ui/resend-verification` | a fresh verification link for an account whose address is still unconfirmed |
| `GET` | `/ui/home` | every enabled service, front door after login |
| `GET`/`POST` | `/ui/admin/users[/{username}]` | user list, editor, permission matrix, template apply, delete (admin) |

`/ui/login` and `/ui/logout` accept `?redirect=`, which is where a relying party sends the browser back to after auth. Targets are validated to prevent an open redirect: a rooted same-origin path (starts with `/`, not `//` or `/\`) is always allowed, plus any prefix listed in `AUTH_REDIRECT_HOSTS`; protocol-relative `//host` targets are rejected outright. The check lives in `quench-auth` (`validated_redirect`), shared by every relying party that redirects to its own `/ui/login`.

## Architecture

### Permission catalog and audiences

`config/permissions.toml` declares `[services.*]` (label, grantable `actions`, any `resource_types` for scoped grants like `conveyor:project:<id>:<action>`, and `home` / `description` for the home page) and `[templates.*]` (named grant bundles). Gatehouse builds its JWT audience ceiling from the catalog's service list at startup — not from a `SERVICE_AUDIENCES` env var, which every *other* service in the estate still reads for its own single-service default. A token's audience is narrowed to only the services its holder was granted (`admin`/`service` roles are wildcards and get every audience); gatehouse itself is always included, since it serves the login page and refresh even for a user with no other grants.

A grant is `service:action` (e.g. `switchboard:launch`), enforced by whatever action a relying party's own routes declare via `Claims::can` — not a blanket read/write ladder, though a service that only needs a coarse split just lists `actions = ["read", "write"]` and gets the old two-level behaviour for free (`RequireWrite` in `quench-auth` checks specifically for `"write"`).

### Roles vs. permissions

Three roles: `admin`, `user`, `service`. `admin` and `service` are wildcards — every permission on every service, without it being written down — but only `admin` can manage users; `service` is a wildcard for *service access* only. Assigning `admin` or `service` is gated on already holding the literal `admin` role in gatehouse's own code, never on a catalog action, so no permission grant can hand out the power to grant admin. Two invariants enforced in `src/realm.rs` regardless of which surface (API or UI) is used: the realm must always keep at least one admin, and nobody can delete or demote themselves. Changing what someone may do ends their sessions immediately, so a permission change applies at once instead of waiting for the access token to expire — the exception is an admin changing only their own password.

The admin pages check the `admin` role themselves and answer with a plain 403 page rather than redirecting to login — bouncing an already-signed-in session to the login form would look like it had expired. Creating a user and granting it access are two separate steps (`POST /ui/admin/users` takes only a username, password and role and lands on that user's editor) rather than one combined submission, so a single mistake can't hand out estate-wide access to an account nobody has reviewed yet.

### Self-service registration and password reset

Both flows send a link through `email::Sender`. With `SMTP_HOST` set, `SmtpSender` delivers a text + HTML email through [`quench-mail`](https://github.com/lorehaven/quench/blob/master/docs/quench-mail.md) (verified TLS, authenticated submission); without it, `LoggingSender` writes the link to the process log instead — fine for dev and BDD, and explicitly not safe for a deployment real users reach, since anyone reading the logs can read the credential-equivalent link. Startup logs which one is active, and with SMTP configured it also checks in the background that the server accepts the login (a failure warns but does not stop gatehouse). A send failure is logged and never changes what the page answers: registration still succeeds, and a reset request still gets the same redirect whether or not the account exists or mail works. Emails are written in the recipient's saved `preferred_locale`, else the language the browser was showing (`qlocale` cookie), else English, in the UI's five languages. Links point at `PUBLIC_BASE_URL`, never at the request's `Host`. Registration refuses an address `quench-mail` could not deliver to (ASCII only, no quoted local parts).

Beyond verification and reset, gatehouse sends four more kinds of mail (all in the five UI languages, all from one `email::Mail` type so a new kind is one variant plus one template):

- **Security notices** - password changed, two-factor turned on or off (only if it was on), and an email address changed - go only to the **confirmed** address on file, never an unconfirmed one (which may belong to someone else). They fire for self-service changes, password resets, and an administrator changing a password or forcing MFA off. A notice that cannot be sent is logged and never fails the request. Following a password-reset link also confirms an unconfirmed address, since the link reached it.
- **Invitations.** Giving an administrator-created account an email address (the "Email address" field on the admin create form, or `"invite": true` with `email` on `POST /api/v1/users`) creates it with a random password nobody is told and emails a link (7 days, single use) to `/ui/accept-invite`, where the person chooses their own password - which confirms the address. A password typed alongside an address is ignored. Until then the unconfirmed address keeps the account from logging in. The admin editor shows whether the address is confirmed and offers "Resend invitation"; the API response reports `invite_sent`. A failed send still creates the account and says so.
- **Changing your own address** (account page): asking needs the current password, is rate limited, and emails a link (24 hours) to the *new* address; opening it only shows the change, and a button makes it (so a mail scanner that fetches links changes nothing). The address becomes confirmed, and the *old* address, if it was confirmed, is told.

### Notifications for other services

A service does not send mail itself and holds no SMTP credentials: it calls `POST /api/v1/notify` with `{"username", "template", "vars", "dedupe_key"?, "requested"?}` and a token that is a machine identity (the `service` role) or holds `gatehouse:notify` (the token's audience must include `gatehouse`, so a calling service's OAuth client needs `gatehouse` among its `allowed_scopes`). An ordinary user's token gets 403.

- **A fixed catalog** (`notify::catalog`, currently `conveyor.run.failed` - on by default - and `conveyor.run.succeeded` and `conveyor.run.recovered` (the first pass after a failure) - both off by default) owns the wording, the five translations and the variables each kind takes. A request naming an unknown kind, missing a variable, adding one, sending a value with a line break or over 300 characters, or a `url` that is not under gatehouse's own public origin is refused with `400` and the reason; validation happens before anything about the person is looked up. A compromised service therefore cannot put text or foreign links under this domain.
- **Whether to send** is gatehouse's call, in this order: the account exists (else `skipped: no_such_user`), it has a confirmed address (`no_verified_email`), the person is subscribed to this kind (`not_subscribed`; a request with `"requested": true` - the service holds a subscription the person made, e.g. following a repository in conveyor - counts as subscribed to a kind that is off by default, but the person's own explicit opt-out of the kind here still wins), the `dedupe_key` was not already used in the last day (`duplicate`), and neither the per-person (20/hour) nor per-person-per-kind (10/hour) limit is spent (`rate_limited`). Being unsubscribed spends no dedupe key or limit. The response is `202 {"status":"accepted"}`, `200 {"status":"skipped","reason":...}`, or `502 {"status":"failed","reason":...,"retryable":bool}` when the mail server did not take it. The global daily mail budget applies as to all mail.
- **Subscriptions** are per kind, and only departures from a kind's default are stored (`auth.notification_prefs`, one row per `username|template`, foundry migration `0011-notification-prefs`; the `auth` module is `0.2.2`). People manage them under "Notifications" on the account page. Account mail (verification, reset, invitation, address and security notices) is not a subscription and cannot be turned off.
- **Unsubscribing** needs no login: every notification carries a link (valid 90 days, reusable - unsubscribing is idempotent) in its footer *and* the standard `List-Unsubscribe` / `List-Unsubscribe-Post: List-Unsubscribe=One-Click` headers (RFC 8058) that mail providers surface as their own "Unsubscribe" button.

Deploy order: the foundry migration Job must finish before a gatehouse that reads the table rolls out (without the table the account page falls back to the defaults and notify returns `failed`/`retryable`). The permission catalog (`config/permissions.toml` and the homecloud `permissions-configmap`) lists `notify` under `gatehouse`.

**Usernames** are limited to ASCII letters, digits and `. _ - @ +`, 1 to 64 characters (`realm::valid_username`), enforced on every path that creates an account. They appear on admin pages and in URLs, mail and logs and anyone can register one, so the set contains nothing that is markup or a path. (Accounts created before the rule are unaffected on login.)

Anything that makes gatehouse send an email is **rate limited**, because each one costs real mail-relay allowance and can flood someone's inbox. Fixed windows, counted in the shared Redis (so every replica agrees): registration 20/hour per client and 3/day per email address; password reset 20/hour per client and 3/hour per submitted username; resending a verification link 20/hour per client, 3/hour per username and one a minute per username; and a global cap of `MAIL_DAILY_LIMIT` emails a day (default 250, under the relay's 300) that stops sending for everyone once spent. Counting is by what was *asked*, never by whether the account exists, so a refusal (`ui_login_rate_limited` / `ui_register_error_rate_limited`) reveals nothing about accounts; the limiter counts with an atomic set-per-window, so parallel requests cannot slip extra sends through, and it fails closed if the store errors. The client address is `X-Real-IP` (else the last `X-Forwarded-For` entry) as set by the ingress - trustworthy only behind a proxy that overwrites it (ingress-nginx with `externalTrafficPolicy: Local` does) - so every per-client limit is backed by one that does not depend on it. The unverified-login error and the registration notice on the login page link to `/ui/resend-verification`; like reset, that page answers identically whether or not the account exists, needs verification, or is disabled (admin and service accounts never need it).

An account with an email address on file **cannot log in until that address is verified** (`/ui/verify`): the login is refused after the password check (`email_unverified` from the API, `ui_login_email_unverified` on the page), so the refusal doesn't reveal which addresses are unconfirmed to someone without the password. Accounts with no address, and `admin` / `service` accounts, are exempt - the seeded admin needs no verification, and an admin adding an address to their profile cannot lock the realm out. A newly self-registered user starts with `registration.default_template` from the permission catalog (`viewer` by default); an admin-created user starts with nothing, since an admin is right there to grant it.

### Sessions

Sessions live in Redis, not Postgres:

```
session:{sid}    -> { username, refresh_hash }   TTL = refresh lifetime
refresh:{hash}   -> sid                          TTL = refresh lifetime
user:{username}  -> set of sids                  TTL = refresh lifetime
```

Expiry is the TTL, so nothing sweeps them; revocation is a `DEL`, so it takes effect at every service on its next request. Refresh-token rotation uses `GETDEL`, atomic on the server, so a token presented twice succeeds at most once. Redis is a hard dependency — gatehouse exits at startup if it is unreachable, and a relying party that cannot read a session treats it as invalid rather than assuming the best.

### The home page

After signing in, `/ui/home` lists the services this deployment offers. Which services exist comes from the permission catalog: an entry in `config/permissions.toml` with `home = true` (and an optional English `description`) asks for a card, titled by its `label`. Where it lives and whether it is switched on comes from the environment, keyed by the upper-cased service name: the card appears when `{NAME}_UI_URL` (or `{NAME}_URL`) is set and `FEATURE_{NAME}_ENABLED` is not turned off (default `true`), so the page reflects what actually runs — a deployment that runs only warehouse needs no flags at all. Gatehouse's own entry has no `home`, so it never gets a card. The card's text is `ui_service_<name>_title` / `ui_service_<name>_desc` when a locale defines them, and the catalog's `label` / `description` otherwise. Adding a service to the estate is therefore a stanza in `permissions.toml` plus its URL in the environment — no gatehouse code change, and the translations are optional. The UI itself is built from the same shell, theme and generated stylesheet as every other service: `quench-starter`'s shared CSS rule sets, `AppShellBuilder` with `Theme::DefaultDark`, and the same five locales; `dist/assets` is generated at startup, so there is nothing to build or commit.

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
| `PUBLIC_BASE_URL` | origin (scheme + host + port, no path) that emailed links point at; falls back to the origin of `GATEHOUSE_URL`, then `http://localhost:5443`. Never taken from the request, so a forged `Host` header cannot redirect a reset link |
| `SMTP_HOST` | mail server for verification and reset emails; **unset means links are logged, not emailed** |
| `SMTP_PORT` | default by `SMTP_TLS`: 465 (`implicit`), 587 (`starttls`), 25 (`none`) |
| `SMTP_TLS` | `implicit` (default), `starttls` or `none`; login is refused over `none` |
| `SMTP_TLS_SERVER_NAME` | certificate name to validate when it differs from `SMTP_HOST` (e.g. connecting to an in-cluster service name) |
| `SMTP_USERNAME` / `SMTP_PASSWORD` | login; set both or neither |
| `MAIL_FROM` / `MAIL_FROM_NAME` | sender address (required with `SMTP_HOST`) and display name (default `Forge`, blank for none) |
| `MAIL_DAILY_LIMIT` | most emails the whole gatehouse may send per day, default 250; over it, sends fail (logged) until the next UTC day |
| `SMTP_TIMEOUT_SECS` | per-step timeout, default 15; one whole delivery is capped at three times this |
| `AUTH_REDIRECT_HOSTS` | comma-separated relying-party origins allowed as `?redirect=` |
| `ACCESS_TOKEN_TTL_SECS` | access token lifetime (default 900) |
| `AUTH_DB_SCHEMA` | realm schema name, default `auth` |
| `AUTH_COOKIE_NAME` / `AUTH_REFRESH_COOKIE_NAME` | session/refresh cookie names, default `forge_session` / `forge_refresh` |
| `AUTH_COOKIE_DOMAIN` | parent domain for cross-subdomain SSO; unset means host-only |
| `{NAME}_UI_URL` / `..._URL` | per-service links shown on `/ui/home` |
| `FEATURE_{NAME}_ENABLED` | hide a configured service from `/ui/home` (default `true`) |

In local dev (`foreman.toml`) gatehouse runs on port 5443 under base path `/gatehouse` with no `needs` (it is the destination, not a client of anything). It is the one service `foreman` deliberately unsets `GATEHOUSE_URL`/`GATEHOUSE_CLIENT_ID` for.

## Testing

Inline `#[cfg(test)]` modules cover the permission catalog (`src/catalog.rs` — known services/actions, resource-scoped grants, template validation), audience narrowing (`src/api/auth.rs`), and the admin permission-matrix fold-in logic (`src/ui/pages/admin.rs`). Run with `cargo test -p gatehouse-service` or `foreman test gatehouse`.

[Home](../README.md)
