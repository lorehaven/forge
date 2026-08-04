# Quench Auth

`quench-auth` (crate `quench_auth`) is the relying-party half of Forge's realm authentication. It gives every service the pieces needed to *verify* a token or session that gatehouse issued — JWT decode/verify, permission checks, session-revocation lookups, and the actix middleware and UI helpers that wire those into a service's request pipeline. It deliberately does not include a login form, a token-issuing API, or user administration: those live in gatehouse (`docker/gatehouse-service`), the one service in the realm allowed to mint tokens and manage accounts. Relying parties that depend on it include gatehouse itself (which also uses it to verify its own tokens), sage-service, switchboard-service, warehouse-service, and conveyor-service.

## Public API / Key Types

Reachable via `quench_auth::prelude` (see `libs/quench-auth/src/prelude.rs`) and `quench_auth::actix::*`:

| Module | Purpose |
|---|---|
| `domain::jwt` | `JwtConfig` (construct with `init()` for a relying party, `init_signing()` for gatehouse, `for_tests()`/`for_tests_with_signing()` in tests) and `Claims` (`sub`, `aud`, `scope`, `exp`, `iat`, `sid`), with `Claims::allows`, `has_role`, `can`, `has_wildcard`, `roles`, `permissions` |
| `domain::auth` | `User`, `Role` (`Admin`, `User`, `Service`), `Actions`, `Permissions`, and read-only `UserDb` (`get_user`, `validate`) for HTTP Basic service-to-service auth |
| `domain::realm` | the realm's cookie names, cookie domain, and gatehouse URLs — the only place these are constructed |
| `domain::session` | `Session`, `SessionDb` (backed by `quench-cache`'s `CacheStore`, i.e. Redis in a deployment); `is_active` for per-request revocation checks, `revoke_all` to end every session a user holds |
| `domain::jwks` | `JwksVerifier`, which fetches and caches gatehouse's published JWKS over HTTP so a relying party can verify a token's signature without a shared secret |
| `domain::sso_client` | the relying-party half of the authorization-code + PKCE flow against gatehouse's `/authorize` and `/auth/callback` |
| `middleware::auth::Auth` | actix middleware that verifies the token on every request: signature, expiry, audience, session |
| `middleware::require_write::RequireWrite` | actix middleware enforcing the `"write"` action on any non-GET/HEAD/OPTIONS request, reading the `Claims` `Auth` already placed in request extensions |
| `routers::ui` | `is_ui_authenticated`, `get_user_from_req` |
| `routers::ui::pages::auth` | `login_delegation`, `auth_callback`, `logout_delegation`, `redirect_target`/`validated_redirect` (open-redirect-safe `?redirect=` handling), `auth_status` |

## Checking a permission

A permission is one action on one service. Actions are plain strings a service defines for itself (`"read"`, `"write"`, or something more specific like switchboard's `"launch"`), with no built-in ordering between them:

```rust
use quench_auth::prelude::Claims;

if !claims.can("warehouse", "write") {
    return HttpResponse::Forbidden().finish();
}
```

A wildcard role (`admin` or `service`) satisfies everything, which is why an admin's token carries no permission entries at all. `has_wildcard()` asks "may act on this service's administration"; `has_role("admin")` asks "may administer the realm" — different questions, and only gatehouse should be asking the second.

Never test the scope claim with `contains`. With permissions packed into the same claim, `scope.contains("admin")` also matches a grant naming a service called `admin`; `has_role` splits the claim into entries and ignores anything with a colon.

Service *access* needs no check at all: gatehouse narrows a token's audience list to the services its holder was granted, so the audience check in `middleware::auth` already refuses a user with no grant. What needs a check is telling `read` from `write`.

## Mounting the middleware

`Auth` establishes identity (signature, expiry, audience, session); `RequireWrite` layers a blanket write check on top of it. Actix runs the *last*-registered `.wrap()` first, so `Auth` must be the outer layer:

```rust
web::scope("/api/v1/things")
    .wrap(RequireWrite::new(config.clone()))
    .wrap(Auth::new(config))
```

## Noticing a session that ended

A page is authenticated once, when it is rendered, and a tab left open then goes on looking signed in however long ago the session expired or was revoked — the first anyone learns of it is a click that lands on a login page.

`auth_status` closes that gap. Mounted at `/ui/status`, it always answers `200` (this is a question about the session, not a request that needs one) with `{ authenticated, username, roles }`. The page shell's session watcher — in `quench-web`, so every service gets it for free — polls it once a minute and on returning to the tab, and navigates to login only when the body parses and says `authenticated: false`; a 404, a network blip, or a body that won't parse are all ignored, since throwing somebody out of a working page because one request failed is worse than the bug it would catch.

It reads the cookie and its signature, not the session store — whether the session is still live is `is_ui_authenticated`'s job on the requests that matter. Doing that here would put a store round trip on every open tab every minute to catch a revocation moments before the next real request would anyway. Services mount it themselves, so one that doesn't is simply not watched — gatehouse deliberately doesn't, since it owns the login page the watcher would send people to.

## What is not here, and why

The token API (`/api/v1/auth/login`, `refresh`, `logout`, `userinfo`, `sessions`), the login form, user seeding, and the user administration API (`/api/v1/users`) live in gatehouse (`docker/gatehouse-service/src/api/`, `src/ui/pages/auth.rs`, `src/bootstrap.rs`). A relying party verifies tokens; it never issues them, never renders a login form, and never creates or edits a user — keeping that code in the library is what made every service look like it could authenticate on its own.

`Access` and `Claims::can` are here because *checking* a permission is a relying party's job. Deciding one is not: there is no way to write a grant through this crate, and `UserDb` remains read-only.

## Still here

`UserDb::validate` and `SessionDb::is_active` are the two read paths that keep a relying party talking to a store directly rather than trusting the token alone:

- `UserDb::validate` — no longer used for sage → switchboard (that moved to the `client_credentials` grant, verified via JWKS like everything else), but still backs warehouse's Docker Registry v2 token endpoint (`docker/warehouse-service/src/routers/docker/token.rs`), which authenticates with HTTP Basic because the registry protocol requires it, not by the estate's own choice.
- `SessionDb::is_active` — per-request revocation, a Redis read rather than a database one. Would go away with token introspection, or with access tokens short enough not to need it — neither is planned.

## Configuration

Read from the environment via `envmnt` (see `JwtConfig::from_parts` and `domain::realm`):

- `SERVICE_NAME`, `SERVICE_REALM`, `SERVICE_AUDIENCES` (comma-separated), `SERVICE_AUTH_ENABLED`
- `ACCESS_TOKEN_TTL_SECS` (default 900), `REFRESH_TOKEN_TTL_SECS` (default 604800)
- `GATEHOUSE_URL` — mandatory for any service using this crate with auth enabled; also gates the JWKS verifier and the SSO delegation endpoints
- `AUTH_DB_SCHEMA`, `AUTH_COOKIE_NAME`, `AUTH_REFRESH_COOKIE_NAME`, `AUTH_COOKIE_DOMAIN`, `AUTH_REDIRECT_HOSTS`, `BASE_PATH`
- `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, `GATEHOUSE_TLS_VERIFY` for the SSO client
- `REDIS_URL`/`CACHE_URL` (via `quench-cache`) for a shared `SessionDb`; without one, sessions are held in-process (lost on restart, invisible to other replicas)

## Testing

Unit tests live under `libs/quench-auth/tests/unit/`, covering `domain::auth`, `domain::jwt`, `domain::realm`, `domain::session`, `middleware::require_write`, and `routers::ui::pages::auth`. `JwtConfig::for_tests()` and `JwtConfig::for_tests_with_signing()` are exposed publicly (not `#[cfg(test)]`) specifically so downstream crates' own test binaries can build a config without a real database or gatehouse.

[Home](../README.md)
