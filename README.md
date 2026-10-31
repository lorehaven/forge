# Forge

Forge is a Cargo workspace of the services, libraries and command-line tools that run a self-hosted estate, all built on the [Quench](https://github.com/lorehaven/quench) framework: authentication, storage and registries, CI/CD, cluster deployment, an LLM workspace, host metrics, and the personal apps that sit on them.

**Documentation: [docs/](./docs/index.md)** (guides, per-service and per-library reference, every command)

## Purpose

* Run the whole estate from one workspace: one login (Gatehouse), one storage (Warehouse), one CI (Conveyor), one deployer (Gantry).
* Give every service the same shape: a Quench HTTP service, a `core` crate with no I/O, an `app` crate of screens, and a `web` crate that runs them in the browser.
* Ship the tools that build, release and operate it (`anvil`, `riveter`, `foreman`, `conveyor`, `warehouse`).

## General use cases

* **Build and release**: `anvil` for builds, tests and releases in dependency order; `conveyor` runs the same on every push.
* **Deploy**: `riveter` renders and applies Kubernetes overlays and packs them as `.rivet` packages; Gantry installs and swaps them from a UI.
* **Store**: Warehouse holds container images, crates, files and packages.
* **Sign in**: Gatehouse issues the tokens the services verify; `gatehouse-auth` and `gatehouse-client` are the matching clients.
* **Run models**: Switchboard launches vLLM instances, Sage chats over them, `welder` routes prompts through agents.
* **Personal apps**: Daybook, Forklift and Hoist share one codebase across the browser, Linux and Android.
* **Develop locally**: `foreman` starts the estate from `foreman.toml` (see [running locally](./docs/guides/running-locally.md)).

New here? Start with the [architecture guide](./docs/guides/architecture.md).

## Members

Every member has a README in its directory. Services also hold `core`, `app` and `web` crates (and `net`, `host`, `desktop`, `surface` for the native apps) beside the `service` crate.

### Command-line tools

| Member | Description | Docs |
| --- | --- | --- |
| [`anvil`](./cli/anvil/) | Workspace build, test and release tool | [📖](./docs/cli/anvil/index.md) |
| [`conveyor`](./cli/conveyor-cli/) | Terminal client of the Conveyor CI service | [📖](./docs/cli/conveyor-cli/index.md) |
| [`foreman`](./cli/foreman/) | Runs a project's local estate from one TOML file | [📖](./docs/cli/foreman/index.md) |
| [`pulley`](./cli/pulley/) | Backup and sync jobs powered by rsync | [📖](./docs/cli/pulley/index.md) |
| [`riveter`](./cli/riveter/) | Kubernetes manifest renderer and applier, with `.rivet` packages | [📖](./docs/cli/riveter/index.md) |
| [`forge-toolbox`](./cli/toolbox/) | Monitor and install the Forge tools | [📖](./docs/cli/toolbox/index.md) |
| [`warehouse`](./cli/warehouse-cli/) | Command-line client of Warehouse | [📖](./docs/cli/warehouse-cli/index.md) |
| [`welder`](./cli/welder/) | Multi-agent LLM workflow runner | [📖](./docs/cli/welder/index.md) |

### Libraries

| Member | Description | Docs |
| --- | --- | --- |
| [`conveyor-pipeline`](./libs/conveyor-pipeline/) | The Conveyor pipeline language: parsing, conditions and run planning | [📖](./docs/libs/conveyor-pipeline/index.md) |
| [`forge-shell`](./libs/forge-shell/) | The frame the browser clients share: top bar, tokens, styles | [📖](./docs/libs/forge-shell/index.md) |
| [`forge-ui`](./libs/forge-ui/) | Top bar pieces shared by the server-rendered pages | [📖](./docs/libs/forge-ui/index.md) |
| [`forge-web`](./libs/forge-web/) | Serves a service's wasm client with a strict CSP | [📖](./docs/libs/forge-web/index.md) |
| [`gatehouse-auth`](./libs/gatehouse-auth/) | Application-token credentials for the CLI tools | [📖](./docs/libs/gatehouse-auth/index.md) |
| [`gatehouse-client`](./libs/gatehouse-client/) | A blocking Gatehouse session for native clients | [📖](./docs/libs/gatehouse-client/index.md) |
| [`rivet-package`](./libs/rivet-package/) | The `.rivet` package format | [📖](./docs/libs/rivet-package/index.md) |

### Services

| Member | Description | Docs |
| --- | --- | --- |
| [`gatehouse-service`](./services/gatehouse/service/) | One login, one identity store | [📖](./docs/services/gatehouse/index.md) |
| [`warehouse-service`](./services/warehouse/service/) | Docker and Cargo registries, files, artifacts, `.rivet` packages | [📖](./docs/services/warehouse/index.md) |
| [`conveyor-service`](./services/conveyor/service/) | CI/CD: webhooks, pipelines and runs | [📖](./docs/services/conveyor/index.md) |
| [`gantry-service`](./services/gantry/service/) | Installs, upgrades and swaps what runs in the cluster | [📖](./docs/services/gantry/index.md) |
| [`foundry-service`](./services/foundry/service/) | Database initialisation from the migration catalog | [📖](./docs/services/foundry/index.md) |
| [`sage-service`](./services/sage/service/) | AI workspace | [📖](./docs/services/sage/index.md) |
| [`switchboard-service`](./services/switchboard/service/) | Models registry and vLLM instances | [📖](./docs/services/switchboard/index.md) |
| [`workbench-service`](./services/workbench/service/) | Projects, boards and issues | [📖](./docs/services/workbench/index.md) |
| [`gauge-service`](./services/gauge/service/) | Host metrics dashboard | [📖](./docs/services/gauge/index.md) |
| [`lectern-service`](./services/lectern/service/) | Public documentation viewer | [📖](./docs/services/lectern/index.md) |
| [`daybook`](./services/daybook/) | Tasks and lists: service, Android, desktop and browser | [📖](./docs/services/daybook/index.md) |
| [`forklift`](./services/forklift/) | Photo, video and folder backup to Warehouse | [📖](./docs/services/forklift/index.md) |
| [`hoist`](./services/hoist/) | App installer and updater | [📖](./docs/services/hoist/index.md) |
| [`hoist-service`](./services/hoist/service/) | Hoist's public app catalogue | [📖](./docs/services/hoist/index.md) |

### Testing

| Member | Description | Docs |
| --- | --- | --- |
| [`forge-bdd`](./tests/forge-bdd/) | One BDD runner for every service | [📖](./docs/guides/testing.md) |
| [`forge-sectest`](./tests/forge-sectest/) | Black-box security scanner for the services | [📖](./docs/guides/testing.md) |

## Build

```bash
cargo build --release
```

## Tests

```bash
cargo test --workspace
```

Behaviour suites run with `forge-bdd` and the security scan with `scripts/security.fish`; see the [testing guide](./docs/guides/testing.md).

## License
MIT
