# Quench Auth

The relying-party half of Forge authentication: everything a service needs to
*verify* a realm session and to hand a browser to gatehouse. It deliberately
does not contain the auth API.

## What is here

| Module | Purpose |
|---|---|
| `domain::jwt` | `JwtConfig`, `Claims`, audience matching (`Claims::allows`), and the permission checks `Claims::{has_role, can, has_wildcard}` |
| `domain::realm` | the realm's cookie names, cookie domain and gatehouse URLs — the only place these are constructed |
| `domain::auth` | `User`, `Role`, `Access`, `Permissions`, and **read-only** `UserDb` (`get_user`, `validate`) for Basic-auth service-to-service calls |
| `domain::session` | `SessionDb`, backed by the cache store (Redis); relying parties use `is_active` to honour revocation, and `revoke_all` ends every session one user holds |
| `middleware::auth::Auth` | verifies the token on every request: signature, expiry, audience, session |
| `routers::ui` | `is_ui_authenticated`, `get_user_from_req` |
| `routers::ui::pages::auth` | `login_delegation`, `logout_delegation`, `redirect_target` — handing the browser to gatehouse and validating its return address; `auth_status`, what an open page polls to learn its session ended |

## Checking a permission

A permission is access to one service at one level. Ask the verified claims:

```rust
use quench_auth::prelude::Access;

if !claims.can("warehouse", Access::Write) {
    return HttpResponse::Forbidden().finish();
}
```

`Access::Write` satisfies a route asking for `Read`, and a wildcard role (`admin`
or `service`) satisfies everything — which is why an admin's token carries no
permission entries at all. Use `has_wildcard()` for "may act on this service's
administration", `has_role("admin")` for "may administer the realm"; those are
different questions, and only gatehouse should be asking the second.

Never test the scope claim with `contains`. With permissions in the same claim,
`scope.contains("admin")` also matches a grant naming a service called `admin`;
`has_role` splits the claim into entries and ignores anything with a colon.

Service *access* needs no check at all: gatehouse narrows a token's audience list
to the services its holder was granted, so the audience check in `middleware::auth`
already refuses a user with no grant. What needs a check is telling `read` from
`write`.

## Noticing a session that ended

A page is authenticated once, when it is rendered, and a tab left open then goes
on looking signed in however long ago the session expired or was revoked. The
first anyone learns of it is a click that lands on a login page.

`auth_status` closes that. Mount it at `/ui/status` and the page shell's session
watcher — in quench-web, so every service gets it — polls it once a minute and
on returning to the tab, and navigates to login when the answer says
`authenticated: false`. Only that answer counts: a 404, a network blip or a body
that will not parse are all ignored, since throwing somebody out of a working
page because one request failed is worse than the bug being fixed.

It reads the cookie and its signature, not the session store. Whether the
session is still live is `is_ui_authenticated`'s job on the requests that matter;
doing it here would put a store round trip on every open tab every minute to
catch a revocation moments before the next real request does.

Services mount it themselves, so one that does not is simply not watched —
gatehouse deliberately does not, since it owns the login page the watcher would
send people to.

## What is not here, and why

The token API (`/api/v1/auth/login`, `refresh`, `logout`, `userinfo`,
`sessions`), the login form, user seeding and the user administration API
(`/api/v1/users`) live in **gatehouse** (`docker/gatehouse-service/src/api/`,
`src/ui/pages/auth.rs`, `src/bootstrap.rs`). A relying party verifies tokens; it
never issues them, never renders a login form, and never creates or edits a user.
Keeping that code in the library is what made every service look like it could
authenticate on its own.

`Access` and `Claims::can` are here because *checking* a permission is a relying
party's job. Deciding one is not: there is no way to write a grant through this
crate, and `UserDb` remains read-only.

`GATEHOUSE_URL` is therefore mandatory for any service using this crate with
auth enabled — see the gatehouse README.

## Still here

`UserDb::validate` and `SessionDb::is_active` are the two read paths that keep
a relying party talking to a store directly rather than trusting the token
alone:

- `UserDb::validate` — no longer sage → switchboard (that moved to the
  `client_credentials` grant, verified via JWKS like everything else), but
  still warehouse's Docker Registry v2 token endpoint, which authenticates
  with HTTP Basic because the registry protocol requires it, not the estate's
  own choice.
- `SessionDb::is_active` — per-request revocation, a Redis read rather than a
  database one. Would go away with token introspection, or with access tokens
  short enough not to need it - neither is planned.
