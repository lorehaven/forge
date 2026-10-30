---
title: Architecture
parent: Guides
nav_order: 1
---

Forge is one Cargo workspace (`/Cargo.toml`, edition 2024, `resolver = "2"`, `unsafe_code = "forbid"`) that holds the services, the shared libraries, the command-line tools and the test suites of the estate. Everything is built on the crates of the sibling Quench repository, pulled from the `ennor` registry. This page is checked against the workspace members and `cargo metadata`.

## Workspace members

```toml
members = [
  "cli/*", "libs/*", "tests/*",
  "services/<name>/{app,core,service,web}"   # per service, see below
]
```

| Group | Members |
|---|---|
| `cli/*` | `anvil`, `conveyor-cli`, `foreman`, `pulley`, `riveter`, `toolbox` (package `forge-toolbox`), `warehouse-cli`, `welder` |
| `libs/*` | `conveyor-pipeline`, `forge-shell`, `forge-ui`, `forge-web`, `gatehouse-auth`, `gatehouse-client`, `rivet-package` |
| `tests/*` | `forge-bdd`, `forge-sectest` |
| services with a server and a browser client | `conveyor`, `gantry`, `gatehouse`, `gauge`, `sage`, `switchboard`, `warehouse`, `workbench`: `app`, `core`, `service`, `web` |
| `daybook` | `app`, `core`, `desktop`, `host`, `net`, `service`, `surface`, `web` |
| `hoist` | `app`, `core`, `desktop`, `net`, `service`, `surface` (no `web`) |
| `forklift` | `app`, `core`, `desktop`, `host`, `net`, `surface` (no server, no `web`) |
| `foundry` | `service` only |

`forge-ui` is a workspace member but no other member depends on it (`cargo metadata` shows no edge); it only carries a gallery example. The directory layout is:

```text
cli/        binaries
libs/       shared crates
services/   one directory per service or app; <name>/service is the server crate
tests/      forge-bdd (cross-service BDD), forge-sectest (security scanner and gate)
images/     Dockerfile.alpine (every service image), Dockerfile.vllm.rocm, gantry-runner/, rust-builder/ (CI images)
scripts/    image.sh, security.fish, unsafe-allowlist.txt, web.sh
plans/      internal plans
foreman.toml   the local estate
.conveyor.toml the CI pipeline
.anvil/     anvil configuration (docker.toml, release.toml, install.toml, ...)
```

## Crate layers of a service

Every service that has a UI follows one layout (see [The browser-client pattern](browser-client.md)). The edges below are the real non-dev dependencies between workspace crates.

```text
<name>-service  ->  <name>-web  ->  <name>-app  ->  <name>-core
      |                  |               |
      +-> forge-web      +-> (wasm)      +-> forge-shell
          +-> forge-shell
```

| Layer | Role | Depends on (workspace) |
|---|---|---|
| `<name>-core` | wire model, form rules, logic; no I/O; builds for `wasm32` | none |
| `<name>-app` | screens over `Request`/`Msg`, no I/O; also the stylesheet | `<name>-core`, `forge-shell` |
| `<name>-web` | `cdylib` + `rlib`: the wasm module over `fetch` | `<name>-app`, `<name>-core` |
| `<name>-service` | the HTTP API, SSO routes, the page that starts the client | `<name>-web`, `forge-web`, plus `<name>-core` for most |

Exceptions found in `cargo metadata`:

* `conveyor-service` also depends on `conveyor-pipeline`; `gantry-service` and `warehouse-service` on `rivet-package`.
* `switchboard-service` has no direct edge to `switchboard-core`.
* `daybook-service` depends on `daybook-web` and `forge-web` only; `daybook-app` depends on `daybook-core` only (the frame comes from `daybook-web`, which depends on `forge-shell`).
* `hoist-service` and `foundry-service` depend on no workspace crate.
* `gatehouse-service` has `gatehouse-core` and `gatehouse-web`.

## Native apps

Daybook, Hoist and Forklift run on Android and Linux with the Quench native renderer, with no WebView.

| Crate | Role |
|---|---|
| `*-core` | model, no I/O |
| `*-app` | screens (Daybook, Hoist and Forklift: `Request`/`Msg` between app and host) |
| `*-net` | the API over `gatehouse-client` (blocking client; keeps the session) |
| `*-host` | (Daybook, Forklift) the controller both hosts share, over a `Platform` |
| `*-desktop` | Wayland window, binary `daybook` / `hoist` / `forklift` |
| `*-surface` | Android program built as `libquench_surface.so` |
| `daybook-web` | Daybook's browser build |

Edges: `daybook-host -> daybook-core, daybook-net`; `forklift-host -> forklift-app, forklift-core, forklift-net`; `hoist-surface -> hoist-app, hoist-core, hoist-desktop`; `daybook-surface` and `forklift-surface` -> app, core, host, net. `daybook-net`, `forklift-net` and `hoist-net` depend on `gatehouse-client`. See the app pages in the Services section.

## Libraries

| Lib | Version | Purpose (Cargo `description`) |
|---|---|---|
| `forge-shell` | 0.1.0 | the frame of the browser clients: top bar, tokens, `shell.js` |
| `forge-web` | 0.1.0 | serves a service's client: the page that starts a `quench-ui` module and the files it loads |
| `forge-ui` | 0.1.0 | top bar pieces and Untitled UI tokens for the older server-rendered pages (no dependents now) |
| `gatehouse-auth` | 0.1.7 | application-token credentials for the CLI tools (used by `anvil`, `riveter`) |
| `gatehouse-client` | 0.1.0 | blocking client session for gatehouse (used by the native apps) |
| `conveyor-pipeline` | 0.2.1 | the pipeline language; shared by `conveyor-service` and `conveyor-cli` |
| `rivet-package` | 0.1.6 | the `.rivet` package format; shared by `warehouse-service`, `gantry-service`, `riveter` |

## The estate

Twelve server crates exist: `conveyor`, `daybook`, `foundry`, `gantry`, `gatehouse`, `gauge`, `hoist`, `lectern`, `sage`, `switchboard`, `warehouse`, `workbench` (each `services/<name>/service`). They are quench-http servers started by `quench-starter`; most own a Postgres schema installed by Foundry.

```text
                    Gatehouse (identity, tokens, JWKS)
                         | every service verifies tokens locally
  Conveyor  Warehouse  Switchboard  Sage  Workbench  Daybook  Gantry  Gauge  Hoist
  (CI/CD)   (storage)  (vLLM)       (chat) (issues)  (tasks)  (deploy)(metrics)(catalogue)
  Foundry: run-to-completion job that installs every service's schema
```

* **Gatehouse** holds the user table and the login page and issues Ed25519-signed JWTs. Services verify them against its JWKS; there is no per-request call back. See [Authentication and permissions](authentication.md).
* **Foundry** applies the migration catalogue (`services/foundry/service/migrations`) and exits; it runs before a rollout or as a foreman task.
* **Switchboard** is the only service that starts vLLM; **Sage** asks it for models.
* **Conveyor** runs pipelines from `.conveyor.toml`, using `anvil`, `riveter` and Warehouse as the registry.
* **Warehouse** serves a Cargo registry, a Docker registry (v2), plain file storage and `.rivet` packages.
* **Gantry** deploys `.rivet` packages with runner Jobs.
* **Gauge** shows host metrics; no database.
* **Lectern** serves the documentation books held in a Warehouse files storage to anyone: no login, no database, no schema. See [Lectern](../services/lectern/index.md).
* **Daybook**, **Hoist** and **Forklift** are the apps; Daybook and Hoist have servers, Forklift talks to Warehouse.

Per-service facts (ports, env, schemas) are on each service's page.

## Command-line tools

`foreman` runs the local estate from `foreman.toml` ([Running the estate locally](running-locally.md)). `anvil` builds, lints, tests and releases. `riveter` renders and applies Kubernetes manifests. `conveyor-cli` and `warehouse-cli` are clients. `welder` runs LLM workflows. `pulley` is an rsync tool. `forge-toolbox` is a TUI for installed Forge tools.

## Shared plumbing

All services use Quench crates: `quench-http` (server), `quench-starter` (TLS/plain setup, base path, health, DB bootstrap), `quench-auth` (token verification, `Claims::can`), `quench-db`/`quench-pg`, `quench-cache`, `quench-client`, `quench-config`. The browser stack is `quench-ui`, `quench-ui-web`, `quench-style`, `quench-fullstack`. Their documentation is in the Quench book.

## Dependency policy

Workspace lints: `unsafe_code = "forbid"`, `missing_debug_implementations = "deny"`, clippy groups `all`, `cargo`, `complexity`, `correctness`, `nursery`, `pedantic`, `perf`, `style`, `suspicious` denied (with `missing_errors_doc`, `missing_panics_doc`, `multiple_crate_versions`, `fn_params_excessive_bools` allowed). `deny.toml` configures `cargo deny`; the CI `deps` job runs `machete`, `audit` and `deny`. `scripts/unsafe-allowlist.txt` lists the files the security gate allows to contain `unsafe`.

## Build profiles

`[profile.release]` strips debug info. `[profile.wasm]` inherits release with `opt-level = "z"`, `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`; `[profile.wasm-dev]` inherits dev. The renderer-heavy crates (`hoist-app`, `forklift-*`, `daybook-app`, `quench-css`, `quench-layout`, `quench-paint`, `quench-text`, `quench-image`, `quench-raster`, `quench-svg`, `quench-ui`, `quench-ui-native`, `quench-style`, `quench-desktop-native`, `quench-android-surface`) are built with `opt-level = 3` and no debug assertions even in dev builds, because the native renderer lays out and paints on every change.

Prerequisites for working in the repository: a Rust toolchain with edition 2024, `docker` (anvil image builds), `kubectl` (riveter), `rsync` (pulley), `ollama` or `vllm` (welder), the `wasm32-unknown-unknown` target for the browser clients.
