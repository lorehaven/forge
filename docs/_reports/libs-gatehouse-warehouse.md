# Report: libs (conveyor-pipeline, forge-shell, forge-ui, forge-web, gatehouse-auth, gatehouse-client, rivet-package) and services gatehouse, warehouse

> **Fixed 2026-10-30** (the pages of the book describe the fixed behaviour; the text below is the original finding):
> * Security 1: `GET /token` narrows the requested scope to the user's `warehouse:read` / `warehouse:write` grants (`/v2/_catalog` is still not scope-checked).
> * Security 7: `quench-auth` `/ui/status` splits the scope on spaces as well as commas.


Scope: `docs/libs/<lib>/`, `docs/services/gatehouse/`, `docs/services/warehouse/`, and `scripts/docs-check.py` (two changes, below). Route coverage: gatehouse 85 operations documented, 85 route declarations in `services/gatehouse/service/src`; warehouse 119 documented, 119 declared (115 macro routes plus 4 hand-registered `HEAD` routes). `python3 scripts/docs-check.py gatehouse warehouse` passes.

## Changes to `scripts/docs-check.py` (please review)

1. **Hand-registered routes.** Four warehouse `HEAD` routes have no macro (no `#[head]` exists); they are registered by hand as `quench_http::route::RouteRegistration { method: Method::HEAD, pattern: "..." }` in `routers/docker/registry/check.rs` (`/v2/`), `routers/docker/blob/check_exists.rs`, `routers/docker/manifest/check_exists.rs` and `routers/files/ops/download.rs`. The checker now also matches the regex `method:\s*Method::(GET|...|OPTIONS)\s*,\s*pattern:\s*"..."`. This adds routes to check, it does not remove any.
2. **Patterned path parameters.** Docker routes are declared `{name:.+}`, `{name:.*}`, `{path:.*}`, `{repository:.+}`. `norm()` now reduces `{x:regex}` to `{x}` (as it already did for `:x`), so the OpenAPI paths are valid templates (`/v2/{name}/blobs/{digest}`) and the description of the regex is in the parameter text.

## (1) Stale or wrong statements in the old pages

Old `docs/docker/gatehouse-service.md` (deleted):

* "The admin pages check the `admin` role themselves and answer with a plain 403": the pages are served to any signed-in person; the data calls and each form post check a catalog action (`gatehouse:read-users`, `create-user`, `edit-user`, `delete-user`, `manage-permissions`); a refused form post is a plain 403 (`ui/pages/admin.rs`).
* "`POST /ui/admin/users` takes only a username, password and role": it also takes `email` (which turns it into an invitation).
* "Redis is a hard dependency; gatehouse exits at startup if it is unreachable": with neither `REDIS_URL` nor `CACHE_URL` set the session, token and rate-limit stores are in process (`quench-cache`); startup fails only when a URL is set and unusable.
* The UI paragraph (`AppShellBuilder`, `Theme::DefaultDark`, "dist/assets is generated at startup"): the pages are the wasm client served by `forge-web`; `service/dist/assets` is unused leftover.
* "Inline `#[cfg(test)]` modules cover the catalog ... (`src/catalog.rs`, `src/ui/pages/admin.rs`)": tests are in `tests/unit/*.rs` and `tests/pg_models.rs`; the catalog is `src/catalog/mod.rs`, the realm rules `src/realm/mod.rs`.
* Config table: omitted `REFRESH_TOKEN_TTL_SECS`, `GATEHOUSE_LOGIN_MAX_ATTEMPTS`, `GATEHOUSE_LOCKOUT_DURATION_SECS`, `SERVICE_TECH_USERNAME/PASSWORD`, `GATEHOUSE_TEST_MODE`, `SERVER_*`, `BASE_PATH`, `GATEHOUSE_URL` (used for the public origin), `DB_POOL_MAX_SIZE`, `LOG_*`, `WEB_DIR`.
* Route table omitted the whole page API (`/api/v1/ui/*`), `POST /api/v1/test/token`, `/ui/status`, `/ui/refresh`, `/ui/account/{avatar,password,email,mfa/*}`, `/ui/admin/users/{u}/{invite,disable,enable,unlock,mfa/disable,template,delete}` and `/ui/assets/*`.

Old `docs/docker/warehouse-service.md` (deleted):

* "A small server-rendered UI ... any signed-in identity may view; every mutation on these pages is held to `can_manage`": the UI is a wasm client over `/ui/api`; crate yank/unyank and Docker image delete need only a session (no `can_manage`).
* "`MAX_FILE_BYTES` / `MAX_REQUEST_BODY_BYTES` per-file / per-request cap": `MAX_REQUEST_BODY_BYTES` is only the default of `MAX_FILE_BYTES`; the same limit also caps artifact publishes.
* The feature-flag list implies `FEATURE_DOCKER_ENABLED`/`FEATURE_CRATES_ENABLED` gate those registries: they only change `/ui/api/overview`; the routes are always served.
* UI test file names (`routers_ui_pages_*`) refer to the server-rendered pages; only `routers_ui_*` files that still exist apply.
* "Redis/REDIS_URL ... required": optional (in-process fallback).
* It did not mention: the `/ui/api` management API, the Docker HEAD routes, `ENABLE_REDIRECT`/`BLOB_REDIRECT_BASE`, `REGISTRY_BASE_URL`, `MAX_CONCURRENT_UPLOADS`, `GATEHOUSE_URL` being mandatory, readiness waiting for Gatehouse, the Docker blob GC semantics (blobs only), and that blobs are global across repositories.

Old `docs/libs/conveyor-pipeline.md`: errors "`thiserror`-derived" and step splitting "with `shlex`": the crate uses `quench_core::Error` and `quench_core::shell::split` and has no third-party dependency. `Triggers::allows` and `PipelineSpec::allows` are right; `plan` described correctly.

Old `docs/libs/rivet-package.md`: "`values.yaml` (or `values.toml`)" while `VALUES_FILE` is `values.toml` and the crate never reads it; `Manifest::version()` "gives the parsed `semver::Version`": it is `quench_core::version::Version`; "Unknown keys under `[package]`, `[requires]` and `[[deployment]]` are errors": also under `[[deployment.also_stops]]`, and unknown top-level keys are accepted.

Crate READMEs outside `docs/` still link to the deleted pages (not changed: outside the allowed scope): `libs/conveyor-pipeline/README.md`, `libs/rivet-package/README.md`, `services/gatehouse/service/README.md`, `services/warehouse/service/README.md` (links to `docs/libs/*.md`, `docs/docker/*.md`). `libs/forge-shell`, `forge-ui`, `forge-web`, `gatehouse-client` have no README; `forge-ui/Cargo.toml` and `forge-shell/Cargo.toml` have `publish = false` and no `readme`.

## (2) Existing but undocumented before

* Gatehouse: the page API, the application-token API, `POST /api/v1/notify` and the notification catalog (documented briefly in the old page), `GET /api/v1/ui/*`, avatar upload rules, rate-limit policy table, MFA pending token format, `GATEHOUSE_TEST_MODE`, OAuth client file format and the redirect-URI derivation, signing-key retirement, session/cache key layout, the foundry migration list, the cluster overlay objects.
* Warehouse: the Docker HEAD routes, upload limits, scope check format, `/ui/api`, the dynamic storage layout and sync feed, artifact formats (`?format=`), `latest` resolution, the Android manifest fields, rivet publish rules, GC behaviour, the leftover files (`service/migrations`, `dist`, `.env`, `key.pem`).
* forge-shell/forge-ui: every `fui-*` class (generated indexes of all class selectors and the stylesheets verbatim), the 28 icon masks, tokens, every `data-*` hook of `shell.js`, the inline scripts and global functions of forge-ui.
* forge-web, gatehouse-client, gatehouse-auth, conveyor-pipeline (all keys, steps, cron, errors) and rivet-package (byte layout, manifest keys) had no or only partial reference.

## (3) Suspected bugs and open questions

Security-relevant (warehouse):

1. **Docker tokens are not tied to grants.** `GET /token` (`routers/docker/token.rs:37-70`) authenticates the realm user and signs a token whose `scope` is whatever the query asked for; `middleware/auth.rs` then trusts that scope. Any realm user, with or without a `warehouse` grant, can obtain `repository:*:pull,push` and read, push or delete any image (`DELETE /v2/.../manifests/<digest>` too). `GET /v2/_catalog` is not scope-checked at all. The tokens are accepted without consulting `warehouse:write`.
2. **UI management API checks are inconsistent.** `POST /ui/api/crates/yank|unyank` (`ui/api/crates.rs:62-84`) and `POST /ui/api/docker/delete-image` (`ui/api/docker.rs:49`) require only a session; the comment says "as before, any session may". `GET /ui/api/files/storages` (`ui/api/files.rs:35`) returns every dynamic storage (owner, quota, usage) and the root path of every static storage to any session, unlike the public API which filters by `can_on_storage`.
3. **Registry auth-failure throttle keys on the first `X-Forwarded-For` entry** (`middleware/auth.rs`, `client_key`), which a client controls; a different forged value per request gives a fresh bucket. Gatehouse's `ClientIp` uses the last entry for exactly this reason.
4. **`warehouse:read` is never enforced** (see permissions page): "read" is "has any grant on warehouse" through the token audience.
5. Crates: owners are informational; any holder of `warehouse:write` can publish any crate name and change owners.
6. `POST /admin/crates/gc` deletes tarballs of yanked versions, so yank plus GC cannot be undone by unyank.

Behavioural/suspected defects:

7. `auth_status` in `quench-auth` (`http/routers/ui/pages/auth.rs:59-68`) builds `roles` by splitting the token `scope` on `,`, but scopes are space separated: for a non-wildcard user `GET /ui/status` returns one element such as `"user sage:read"`. (Affects every service's `/ui/status`; code in the Quench repository.)
8. Crates with upper-case letters: publish stores `<root>/<Name>/...` and the index file under `index_file_path(<Name>)` (original case in the file name, lower-case prefix) while `GET /index/...` and the owners/search routes lower-case the name, so such a crate cannot be found by the index route on a case-sensitive filesystem (`routers/crates/ops/publish.rs`, `index/mod.rs:52`, `owners/mod.rs`). Cargo itself requires lower-case registry names, so this may be unreachable in practice; unverified by a test.
9. `gatehouse-client` hard-codes the base path `/gatehouse` in `login` and `refresh` (`lib.rs`) while `gatehouse-auth` takes the full Gatehouse URL; a Gatehouse mounted elsewhere cannot be used by the former.
10. `gatehouse-auth::login` cannot sign in an account with MFA (the JSON login answers `401 mfa_required`; the library reports a generic refusal and offers no code step). An admin with MFA therefore cannot run `anvil login`.
11. `PATCH /api/v1/users/{u}` and `POST /ui/account/password` by the owner do not end the owner's own sessions (by design per `realm::update`: `revoke = changes_access || username != actor`), although a password change would normally revoke other sessions.
12. `docs-check` compares only `service/src`; `app`/`web` crates have no routes. The framework routes (`/health*`, `/swagger-ui*`, `<base>`, `/`) are documented in the API page conventions, not in `openapi.yaml`, because they come from `quench-starter`.
13. `SERVICE_NAME` defaults to `service`; Warehouse's `RequireWrite` and audience checks use it, so a deployment that forgets `SERVICE_NAME=warehouse` rejects all realm tokens. Gatehouse ignores `SERVICE_AUDIENCES` (its audiences come from the catalog); the old page said so, and the code agrees.
14. `forge-ui` is not used by any crate of the workspace (only a `[workspace.dependencies]` entry and a comment in `gatehouse-service/src/ui/locale.rs`).
15. `service/dist/assets/...` (gatehouse and warehouse), `warehouse/service/migrations/*.toml` (old per-service `users`/`sessions`) look unused by the current services (the `i18n/*.ftl` files are used, via `include_str!` in the apps).
16. Unverified in the documentation: the cache key prefix join (`forge-session:` etc.), the default SMTP ports per mode (`quench-mail`), and every snippet in the `usage.md` pages (written from the sources, not compiled).
