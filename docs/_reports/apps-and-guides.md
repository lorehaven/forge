# Apps and guides: gap report

Merged from the per-slice reports below.

---
## Source: _reports/apps-and-guides-daybook.md
# Gap report: Daybook

## 1. Stale or wrong statements in the old docs

* `docs/docker/daybook-service.md` (deleted): "API-only task manager ... no web UI". The service serves a browser client (`/ui`, `/ui/assets/*`) and forwards sign-in (`/auth/login`, `/auth/refresh`). Six `/ui` routes and two `/auth` routes were undocumented.
* Same page: "Redis ... every request is rejected unless the session is active". True only when `REDIS_URL`/`CACHE_URL` is set; unset falls back to an in-process store with a warning (`quench-auth` `SessionDb::from_env`). If set but unreachable, `main.rs` panics (`expect("session store")`).
* Same page: the create body example omits `description` and `sort_order`; `TaskInput` accepts both. The update description list omitted `is_completed` default (false when omitted).
* Same page: "`foreman start daybook` ... starts the service on port 12443" correct; "installs the Daybook schema" correct (foundry task).
* `homecloud/overlays/forge/daybook/config.yaml.j2` comment says "API only: no UI and no login of its own" (stale; also deployment.yaml.j2 header "API only - no UI"). Not edited (outside forge/docs).
* `services/daybook/README.md` says the service's documentation is `docs/docker/daybook-service.md` (now gone) and points to `quench/docs/migration/daybook.md`. Not edited (outside forge/docs).
* Foundry module `daybook/module.toml` version is `0.3.0` while the service crate is `0.3.2` (harmless; migrations have not changed since 0.3.0).

## 2. In the code but previously undocumented

* `/auth/login`, `/auth/refresh` forwarders (`GATEHOUSE_TLS_VERIFY`, 502 body).
* `/ui*` routes, `WEB_DIR`.
* Roll-forward algorithm details: row lock `FOR UPDATE`, anchor rules, 20 000-step limit after which the task is completed instead of rolled.
* Client side entirely: crates, protocol (`Request`/`Msg`/`Op`/`Reply`), `Store` files, widget snapshot format, launch file, Android job/plugins, browser localStorage keys.
* Error texts table; `DB_SCHEMA`, `DB_POOL_MAX_SIZE`.
* Label colour normalised to lower case; `description` not trimmed while `content` is.

## 3. Open questions / suspected issues

* `daybook_net::api::API = "/daybook/api/v1"` and `LOGIN = "/gatehouse/api/v1/auth/login"` are hard-coded (`net/src/api.rs:8,11`): a deployment with another base path, or foreman's per-port layout, cannot be used by the native programs. Suspected limitation, not verified against a running estate.
* `Controller::set_completed` sends `sent.to_update()` including `sort_order` of the cached task; if the service rolled a task meanwhile the cached copy is stale (probably benign).
* `tasks::update` reads `recurrence_anchor` and compares `stored_rule == changes.recurrence` before validation of the new rule order; rule `weekdays` normalisation happens in `validated()` before the comparison (`check_recurrence` runs first), so equal rules compare equal. Fine; noted only.
* `ListRef::Label` view in the client loads all open tasks and filters locally; `GET /tasks?label_id=` is not used by the clients.
* `service/src/routers/mod.rs`: `actor()` returns the literal `dev` when claims are missing (only possible with `SERVICE_AUTH_ENABLED=false`); with auth disabled all data belongs to the user `dev`, which does not exist in `auth.users`, so the owner foreign key would fail on insert (`23503` -> 400). Suspected; not run.
* `docs/guides/authentication.md` is referenced from `configuration.md`; the guide must use that file name.
* Unverified: one-origin local setup for native programs against foreman; behaviour of an already-running service when gatehouse's JWKS is unreachable.

---
## Source: _reports/apps-and-guides-guides1.md
# Gap report: guides (architecture, browser client, testing, i18n)

## 1. Stale or wrong statements in the old docs

* ARCHITECTURE.md: "eight long-running services" and a diagram of six; the workspace has eleven server crates (conveyor, daybook, foundry, gantry, gatehouse, gauge, hoist, sage, switchboard, warehouse, workbench). It omitted gantry, gauge, hoist and the native apps (forklift).
* ARCHITECTURE.md: Quench crates described as "quench-web ... Element/PageBuilder" renders every service's UI and quench-web-components "has no consumers"; the UIs are now `quench-ui` wasm clients (`<name>-web`) served by `forge-web`. `quench-web` is no longer a dependency of the service crates' UI path (not re-verified for every service).
* ARCHITECTURE.md/README.md linked to Quench pages under github `docs/quench-*.md`; no longer valid (Quench book).
* README.md "Project Structure" listed libs as conveyor-pipeline, rivet-package only; there are seven libs. It listed the docs tree `docs/docker/`, which is replaced by `docs/services/`.
* forge-bdd.md: "with neither [--service nor tags], it runs all six"; `Target::ALL` has seven. The sample summary lacks daybook. "the same ports" statement fine. The doc says `@workbench`/`@daybook` plain HTTP: in code workbench uses `missing.pem` certs (plain HTTP), as stated.
* forge-bdd.md said `.env` supplies the `*_API_URL` variables; in code they are optional overrides with in-code defaults for all seven services (not only conveyor/workbench).
* forge-bdd.md "Seven suites ... `@daybook` (12443)": correct; no stale mismatch there. Scenario totals in its sample (285) are not current: 304 `Scenario` lines now.
* docs/README.md smoke tests (`cargo run -p riveter -- repl`, welder sample path) were not re-verified here (CLI authors own them).
* tests/forge-bdd/README.md (outside docs, not edited) still points to `docs/tests/forge-bdd.md`, now deleted. Needs editing by the owner of that file.
* Old browser-client plan (quench/docs/migration/PATTERN.md) mentions `forge-ui` + `quench-web` removal and `.anvil/docker.toml`; the book follows the code.

## 2. In the code, previously undocumented

* `forge-bdd --no-spawn`; readiness polling (60 x 500 ms, warning only); the `Graceful shutdown` feature name used to run sage's shutdown pass last.
* `forge-sectest` gate cases and `FORGE_SECTEST_INSECURE` / `_ALLOW_REMOTE` / `_QUENCH_DIR`; `public-routes.txt` baseline.
* `.conveyor.toml` stages (check/build/test/security) and the coverage job's sidecar service.
* `HOIST_TEST_DATABASE_URL`, `GAUGE_TEST_DATABASE_URL` and `FOUNDRY_TEST_DATABASE_URL`: tests read them.
* `forge-shell` shell keys, `Bar` fields, `shell.js` behaviour (1 s theme poll).
* `forge-web` page headers (CSP with nonce, `no-store`).
* Profile overrides (`opt-level = 3` for renderer crates, `profile.wasm`).

## 3. Open questions / suspected problems

* `.conveyor.toml` coverage job sets `*_TEST_DATABASE_URL` for conveyor, daybook, foundry, gantry, gatehouse, sage, warehouse, workbench only; `GAUGE_TEST_DATABASE_URL` and `HOIST_TEST_DATABASE_URL` are read by tests but not set, so those integration tests skip in CI coverage. Also the foundry `--install` list omits `gauge`/`hoist` modules (if they have any).
* `daybook-app` screens are English only; `services/daybook/service/i18n/` is empty (`.gitkeep`) while the old `daybook` docs/Android may expect translations. Daybook web ships 17 shell keys only (`services/daybook/web/i18n`).
* `forge-ui` is a workspace member with no dependents; `docs` call it legacy. Confirm it can be removed.
* `switchboard-service` has no `switchboard-core` edge in `cargo metadata` (the wasm client uses it, the server does not); `gantry-service` and `daybook-service` likewise lack a core edge. Intentional?
* forge-bdd: `selected_targets` matches service names by substring of the raw `--tags` text (`tag_expression.contains`), so a tag such as `@sagely` would select sage. Suspected minor bug (`tests/forge-bdd/src/main.rs`, `selected_targets`).
* forge-bdd `wait_until_ready` only warns when a service never answers; the run then proceeds and fails scenario by scenario.
* The security gate's `live` case is skipped silently by `scripts/security.fish` when `FORGE_SECTEST_TARGETS` is unset (it prints a notice).
* i18n key-set equality between locales is not enforced by any test (checked once by script for this page).
* Not verified by running: `scripts/web.sh`, `foreman test`, `forge-bdd` (they start processes/need builds); the text is from reading the sources.
* docs/README.md was deleted; its "Smoke Tests" and prerequisites list were folded into the Architecture page (prerequisites only). Pages of other authors (docs/cli/*, docs/docker/*, docs/libs/*) still link to the deleted `../README.md`, `../ARCHITECTURE.md` and `../tests/forge-bdd.md`; they are being replaced and the links must go with them.

---
## Source: _reports/apps-and-guides-guides2.md
# Gap report: authentication, running locally, deployment guides

## 1. Stale or wrong statements found
* `docs/ARCHITECTURE.md` (not touched by this slice) says "eight long-running services"; `foreman.toml` runs ten (adds daybook, hoist) and lists gauge, gantry.
* `foreman.toml` header and `.anvil/release.toml`: `anvil release --help` says "packages configured in `[publish].packages`", but the config uses `[release]`.
* `homecloud/overlays/forge/daybook/deployment.yaml.j2` comment says "API only - no UI"; the service serves a browser client at `/daybook/ui` (`WASM_PACKAGE=daybook-web`).
* `homecloud/overlays/forge/values.yaml`: `GANTRY_RUNNER_IMAGE` is `.../gantry/runner:0.1.0` while gantry-service is 0.2.2 and `scripts/image.sh` tags the runner with Gantry's version.
* `images/Dockerfile.alpine` default cargo index is `https://ennor.ddns.net/index/`; `images/gantry-runner/Dockerfile` uses `https://ennor.ddns.net/warehouse/index/`. One of them is probably stale.
* `permissions.toml` header says gatehouse "needs no rebuild"; `clients.toml` is also read at startup (not verified whether it hot-reloads).
* `foreman.toml` comments say "no shared secret anywhere now", yet every client still has a `GATEHOUSE_CLIENT_SECRET` (client secrets, not JWT keys; consistent but misleading).
* REFRESH_TOKEN_TTL_SECS default in code is 604800; overlay sets 2592000.

## 2. In code, previously undocumented
* Grant types `app_token` and `client_credentials`, `lht_` application tokens, resource-scoped actions (`project:<id>:<action>`, `target:`, `storage:`).
* Gantry local run modes (`GANTRY_CLUSTER`/`GANTRY_RUNNER` combinations).
* `FORGE_SKIP_MODELS`, `foreman run <task>`, `seed` task, `web` task, per-service `pre_stop`.
* `anvil artifact`, `keystore-password`, `registry-token`, `docker` subcommands.

## 3. Open questions
* Warehouse Cargo/Docker registry auth was not traced; the authentication guide says so.
* gauge has `Auth` only, no `gauge:read` check in the service despite the catalog entry (`services/gauge/service/src/routers/api/mod.rs`): suspected gap.
* hoist has `hoist:read` in the catalog but the service checks only the audience.
* The exact image-release command sequence for services (docker.toml vs release.toml's package list does not contain service packages) was not run.
* Android notes: how apps are pointed at a local Gatehouse is left to the app pages.

---
## Source: _reports/apps-and-guides-hoist-forklift.md
# Gap report: Hoist and Forklift

## 1. Stale or wrong statements in the old docs

* `docs/docker/hoist-service.md` (deleted, content covered by `docs/services/hoist/`):
  * said the single-app route was `GET /v1/apps/{package}`; the path parameter is `{package_name}` (`service/src/routers/apps.rs`).
  * did not mention the `400 invalid sort: <value>` error, the `503 the catalogue needs Postgres` case or that `offset`/`limit` are clamped and echoed.
  * "`anvil release` sees the service as a docker package, and the windowed desktop host, the Android surface and the browser server are left out of the images": not checked; there is no browser server any more.
* `services/hoist/README.md` (outside the docs, not touched) still starts with the web template text ("`app/` ... runs in the browser and is rendered on the server", `quench dev`, `server/`); there is no `server/` crate, and the app has no browser client. Same for `services/hoist/app/src/lib.rs` (module doc says "The server renders `app()`..."), `crate-type = ["cdylib","rlib"]`, `quench_ui_web::app!(app)` and the `sample_catalog()` demo `app()`.
* `services/hoist/service/src/models.rs` doc comment on `MergedPackage` says "stored in SQLite"; the store is Postgres.
* No stale statement found in `services/forklift/README.md` itself (not touched).
* `forklift_core::remote_path` doc comment: "Every segment is cleaned to what warehouse accepts: letters, digits, `.`, `_` and `-`". Only `local_id` and `filename` are cleaned; the **category is used raw**, so a folder category produces `custom:<slug>/...` with a colon (confirmed by `core/tests/unit.rs:19-20`).

## 2. Code that was undocumented and now is

* Hoist: the install-state join, `offered()` filtering by arch and package format, the format choice from `/etc/os-release`, the `pkexec pacman -U` install path, the JSON line printed by `--check` and the Android `--check` stdin/stdout contract, announcement key, session sharing between window and background check, the `packages` Kotlin plugin methods, the update job (id 4100, 6 h), the Public tab and `download_public` rules (https only, no token, SHA-256 only when present).
* Forklift: the whole manifest, settings and snapshot formats, the engine's status table and back-off, reconciliation by path then by identity, restore planning and targets, the run lock and the `hidden` marker protocol between the Kotlin host and the program, the `--backup` JSON-line protocol, the three jobs and the foreground service, MediaStore versus SAF differences, all Linux specifics (512 MiB limit, ignored names, folder choosers, restore directory).

## 3. Open questions and suspected bugs

* `forklift-core/src/restore.rs` (`reconcile`) and `host/src/lib.rs:reconciled`: the change feed is read only *since* the stored cursor, and reconciliation only considers files never sent. After the cursor has advanced, a re-installed app that kept the cursor file (it will not) is fine, but a new storage uses a new `sync-<storage>` cursor so that is fine too; the remaining risk is a storage whose feed is disabled (`409`): then nothing is recognised and every file is uploaded again (overwriting the same paths when the paths match). Not a bug, but worth knowing.
* `host/src/lib.rs:list_storages` keeps only storages with an `owner`. A statically configured storage can therefore never be chosen from the UI even if the account may write to it.
* `forklift-surface/src/main.rs` `found_of` and `desktop`: `size` of a SAF document that reports no size is sent as `-1` by Kotlin (`FilesPlugin.list`, `Backup.folder`); `as_u64()` of `-1` is `None`, so the size becomes 0 and the file is recorded with size 0 (changes of such a file would be seen only through `modified_ms`). Suspected edge case.
* `FilesPlugin.list` reads `COLUMN_SIZE` at index 3 but the `Backup.folder` (headless) variant does the same; both return the size, but the `modified_ms` uses `COLUMN_LAST_MODIFIED` in milliseconds while `media` uses `DATE_MODIFIED * 1000`: consistent.
* Linux host: the default settings enable `photos` and `videos`, but the toggles are hidden when `phone == false`; the dashboard then shows `Photos` and `Videos` rows with zero files on the desktop. Cosmetic.
* `forklift_surface::headless::run` ends with `{"finished":true}` for its own skip reasons, while `Backup.run` treats only a `skipped` field as "busy" and returns `!busy`; so a run that skipped because automatic backup is off returns `true` ("did something") and the foreground service stops looping. Probably intended.
* `hoist-app`: `Setup.public` is `false` on the desktop, so the Public tab is Android only; `Request::Launch` and `Request::InstallPublic` are ignored by the desktop worker (`continue`). Not an error, but `DownloadPublic` is also answered on desktop although the tab does not exist.
* `hoist-service` `routers/mod.rs` wraps `Auth` over `{base_path}/v1` only; there is no per-user permission check (`x-permission`). Documented as such. Health probe paths are registered by `quench-starter`; not verified here.
* Hoist `PackagesPlugin.install_from_url` is unused by `hoist-surface` (it installs from a file the Rust side downloaded).
* Not run: emulator, `quench build`, the apps and the service (no Postgres/Gatehouse); `python3 scripts/docs-check.py hoist` passes (3 routes).
