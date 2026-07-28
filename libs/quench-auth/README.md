# Quench Auth

The relying-party half of Forge authentication: everything a service needs to
*verify* a realm session and to hand a browser to gatehouse. It deliberately
does not contain the auth API.

## What is here

| Module | Purpose |
|---|---|
| `domain::jwt` | `JwtConfig`, `Claims`, audience matching (`Claims::allows`) |
| `domain::realm` | the realm's cookie names, cookie domain and gatehouse URLs — the only place these are constructed |
| `domain::auth` | `User`, `Role`, and **read-only** `UserDb` (`get_user`, `validate`) for Basic-auth service-to-service calls |
| `domain::session` | `SessionDb`, backed by the cache store (Redis); relying parties use `is_active` to honour revocation |
| `middleware::auth::Auth` | verifies the token on every request: signature, expiry, audience, session |
| `routers::ui` | `is_ui_authenticated`, `get_user_from_req` |
| `routers::ui::pages::auth` | `login_delegation`, `logout_delegation`, `redirect_target` — handing the browser to gatehouse and validating its return address; `auth_status`, what an open page polls to learn its session ended |

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
`sessions`), the login form and user seeding live in **gatehouse**
(`docker/gatehouse-service/src/api/`, `src/ui/pages/auth.rs`, `src/bootstrap.rs`).
A relying party verifies tokens; it never issues them, never renders a login
form, and never creates a user. Keeping that code in the library is what made
every service look like it could authenticate on its own.

`GATEHOUSE_URL` is therefore mandatory for any service using this crate with
auth enabled — see the gatehouse README.

## Still here only until SSO Phase 2

Two read paths keep relying parties talking to the database directly:

- `UserDb::validate` — the Basic-auth machine-to-machine path (sage → switchboard,
  and warehouse's registry token endpoint). Goes away with the
  `client_credentials` grant.
- `SessionDb::is_active` — per-request revocation, now a Redis read rather than
  a database one. Goes away with token introspection, or with access tokens
  short enough not to need it.

Both are noted in `docs/SSO_PLAN.md` §2.4/§2.6.
