---
title: Deployment
parent: Guides
nav_order: 6
---

How Forge goes from source to the cluster: crates to the `ennor` registry, service images to the Docker registry in Warehouse, apps to Warehouse as APK/deb/rpm/arch/source artifacts, and the estate itself as a `riveter` package built from the sibling `homecloud` repository. Sources: `.anvil/*.toml`, `images/`, `scripts/image.sh`, `.conveyor.toml`, `/mnt/extdrive/projects/lorehaven/homecloud/overlays/forge`. Only manifests were read; no secret and no live system.

## Anvil configuration

Anvil reads `.anvil.toml` and every `.anvil/*.toml` in the workspace root (`anvil config` inspects and validates the merge). Files in `forge/.anvil/`:

| File | Section | Used by |
|---|---|---|
| `release.toml` | `[release]` | `anvil release`: library and tool packages published as crates. |
| `install.toml` | `[install]` | `anvil install`: binaries installed with `cargo install --path`. |
| `docker.toml` | `[docker]` | `anvil docker ...`: service images. |
| `artifacts.toml` | `[artifacts]` | `anvil artifact ...`: `warehouse_url`. |
| `daybook.toml`, `forklift.toml`, `hoist.toml` | `[artifacts.programs.<name>]` | `anvil artifact release <name>`. |
| `secret.toml` | credentials | written by `anvil login` (not documented, never read here). |

### Package release: `anvil release`

`anvil release [--all | -p <package>] [--dry-run]` bumps the patch version, commits, then publishes a crate (and installs it) or releases the Docker image, depending on what the package is configured for. `[release]` in `release.toml`:

```toml
[release]
registry = "ennor"
gatehouse_url = "https://ennor.ddns.net/gatehouse"
packages = [ "anvil", "conveyor-cli", "conveyor-pipeline", "foreman", "forge-toolbox",
             "gatehouse-auth", "pulley", "rivet-package", "riveter", "warehouse-cli", "welder" ]
```

`[install].packages`: anvil, conveyor-cli, foreman, forge-toolbox, pulley, riveter, switchboard-cli, warehouse-cli, welder. Commit subjects such as `release: bump package versions (...)` in the history are this command's output.

### Image release: `anvil docker`

Subcommands: `build`, `tag`, `push`, `release` (build + tag + push), `build-all`, `release-all`. Configuration (`docker.toml`):

```toml
[docker]
registry = "ennor.ddns.net/forge"
cargo_registry = "ennor"

[docker.modules.docker]
dockerfile = "images/Dockerfile.alpine"
packages = [ "warehouse-service", "switchboard-service", "sage-service", "foundry-service", "gatehouse-service",
             "conveyor-service", "workbench-service", "gantry-service", "daybook-service", "gauge-service", "hoist-service" ]
```

Every service has `[docker.modules.docker.<package>]` with `module_name`, `image_name`, `registries = ["ossiriand.arda:8080/forge", "ennor.ddns.net/forge"]` and `build_args`:

| Package | Image (`<registry>/<module>/<image>`) | `build_args` |
|---|---|---|
| conveyor-service | `conveyor/conveyor` | `RUNTIME_PACKAGES=git`, `RESOURCE_DIR=web`, `WASM_PACKAGE=conveyor-web` |
| gatehouse-service | `gatehouse/gatehouse` | `RESOURCE_DIR=web`, `WASM_PACKAGE=gatehouse-web` |
| foundry-service | `foundry/foundry` | `RESOURCE_DIR=migrations`, `RUN_AS=999:999` |
| sage-service | `sage/sage` | `RESOURCE_DIR=web`, `WASM_PACKAGE=sage-web` |
| switchboard-service | `switchboard/switchboard` | `RUN_AS=999:999`, `RESOURCE_DIR=web`, `WASM_PACKAGE=switchboard-web` |
| warehouse-service | `warehouse/warehouse` | `RESOURCE_DIR=web`, `WASM_PACKAGE=warehouse-web` |
| gantry-service | `gantry/gantry` | `RESOURCE_DIR=web`, `WASM_PACKAGE=gantry-web` |
| gauge-service | `gauge/gauge` | `RESOURCE_DIR=web`, `WASM_PACKAGE=gauge-web` |
| daybook-service | `daybook/daybook` | `RESOURCE_DIR=web`, `WASM_PACKAGE=daybook-web` |
| hoist-service | `hoist/hoist` | `RESOURCE_DIR=config` |
| workbench-service | `workbench/workbench` | `RESOURCE_DIR=web`, `WASM_PACKAGE=workbench-web` |

## `images/Dockerfile.alpine`

One Dockerfile for every service.

1. `deps-builder` (`rust:1.98-alpine3.22`): installs `musl-dev pkgconfig build-base ca-certificates git curl openssl`, adds target `x86_64-unknown-linux-musl`, writes `/usr/local/cargo/config.toml` with `[registries.ennor]` from build args `CARGO_REGISTRIES_ENNOR_INDEX` (default `sparse+https://ennor.ddns.net/index/`) and optional `CARGO_REGISTRIES_ENNOR_TOKEN`. Copies `Cargo.toml`, `Cargo.lock`, `cli`, `libs`, `services`, `tests` and builds the whole workspace in release mode for the musl target, excluding the apps (`--exclude hoist-desktop hoist-surface forklift-desktop forklift-surface daybook-desktop daybook-surface`) so that the dependency layer is cached.
2. `builder`: copies the full tree and builds again. If `WASM_PACKAGE` is set it adds `wasm32-unknown-unknown`, builds `-p $WASM_PACKAGE --profile wasm` and copies `target/wasm32-unknown-unknown/wasm/<package>.wasm` to `$RESOURCES_PATH/web/app.wasm`.
3. Runtime (`alpine:3.22`): `ca-certificates` plus `RUNTIME_PACKAGES`. `WORKDIR /app`. Copies, owned by `999:999`, `${RESOURCES_PATH}/config` to `./config`, `${RESOURCES_PATH}/${RESOURCE_DIR}` to `./${RESOURCE_DIR}` and the release binary to `./service`. `USER ${RUN_AS}` (default `root`), `EXPOSE 80`, `ENTRYPOINT ["./service"]`.

Build args: `PROJECT_NAME` and `RESOURCES_PATH` (set by Anvil per package), `RUNTIME_PACKAGES` (default empty), `RESOURCE_DIR` (default `i18n`), `RUN_AS` (default `root`), `WASM_PACKAGE` (default empty).

`images/rust-builder/Dockerfile` (`rust:1.98-bookworm`, glibc, clippy, rustfmt, nightly rustfmt, and the cargo tools the pipeline calls) is the image of every `.conveyor.toml` job. `images/gantry-runner/Dockerfile` builds `riveter` and `gantry-runner` from this workspace plus `kubectl` into an alpine image for Gantry's runner Jobs. `images/Dockerfile.vllm.rocm` is the vLLM ROCm image.

Tool images are built with `scripts/image.sh <rust-builder|gantry-runner> <build|publish>` (also `foreman run build-...`/`publish-...`): tags `ossiriand.arda:8080/forge/ci/rust-builder:latest` and `ossiriand.arda:8080/forge/gantry/runner:<gantry-service version>`. The registry token goes in as build arg `CARGO_REGISTRIES_ENNOR_TOKEN` when `ENNOR_TOKEN` is set; pushing needs `docker login ossiriand.arda:8080`.

## Artifact release: apps and desktop packages

`anvil artifact list` shows what can be released. `anvil artifact release <name> [--target <t>] [--dry-run] [--allow-dirty] [--skip-existing]` builds every target of a program and uploads it to Warehouse (`[artifacts] warehouse_url = "https://ennor.ddns.net/warehouse"`). Programs: `daybook` (`dev.lorehaven.daybook`), `forklift` (`dev.lorehaven.forklift`), `hoist` (`dev.lorehaven.hoist`), each with `dir = services/<name>` and `version_file = "desktop/Cargo.toml"`: one Cargo version drives every build. The version code is `major * 1_000_000 + minor * 1_000 + patch`, the same number `quench` puts in the APK.

Targets (identical for the three; `${version_name}` and `${version_code}` are substituted in `file`):

| Target | Platform / format | Build command | Output |
|---|---|---|---|
| (android) | `android` / `apk` | `quench build apk --release` | `dist/<name>-${version_name}-${version_code}.apk` |
| `linux-deb` | `linux` / `deb`, `x86_64` | `quench build deb` | `dist/<name>_${version_name}_amd64.deb` |
| `linux-rpm` | `rpm` | `quench build rpm` | `dist/<name>-${version_name}-1.x86_64.rpm` |
| `linux-arch` | `pkg.tar.zst` | `quench build arch` | `dist/<name>-${version_name}-1-x86_64.pkg.tar.zst` |
| `linux-source` | `tar.gz`, `universal` | `quench build source` | `dist/<name>-${version_name}.tar.gz` |

Signing the APK uses the project's `release.keystore`; `anvil keystore-password` stores its password in the git-ignored secret file after `anvil login`, otherwise `QUENCH_KEYSTORE_PASSWORD` is read from the environment. Warehouse keeps one build per format of a version.

## Pipeline

`.conveyor.toml` (`on` push to master, any pull request, and a nightly schedule `0 0 * * *` Europe/Warsaw) runs in `rust-builder:latest`, with `ANVIL_APP_TOKEN` and `ANVIL_GATEHOUSE_URL` as secrets:

| Stage | Jobs |
|---|---|
| `check` | `format` (`anvil format --check`), `lint` (`anvil lint --all-targets --deny-warnings --json`), `deps` (`machete`, `audit`, `deny`) |
| `build` | `workspace` (`anvil build --all`) |
| `test` | `unit` (`anvil test --all`), `coverage` (Foundry applies the schemas to a `pgvector/pgvector:pg18` sidecar, then `cargo llvm-cov`; artifact `lcov.info`) |
| `security` (schedule only) | `nightly`: `forge-sectest` scanners, gate and live tests against the in-cluster services; artifact `target/security-findings.json` |

The pipeline has no release stage: releases are run by hand with Anvil.

## The cluster: homecloud

The estate is declared in the sibling repository `homecloud` as `riveter` overlays (one directory per package under `overlays/`: `forge`, `media`, `observability`, `paper-finder`, `palantir`, `stalwart`, `survey-sender`, `vllm`, `vpn`). `riveter pack` builds a `.rivet` package, the pipeline publishes it to Warehouse, Gantry (or `riveter install`) applies it. Layout of `overlays/forge`:

| Path | Content |
|---|---|
| `rivet.toml` | `[package]` name `forge`, version, and `[[deployment]] inference` (sage and switchboard, plus vLLM pods selected by label `app=vllm,app.kubernetes.io/managed-by=switchboard`) |
| `overlay.yaml` | `env: forge`, `namespace_name: forge`, `host: ennor.ddns.net`, and the `resources:` list of `{% include %}`s in dependency order |
| `values.yaml` | non-secret `${VAR}`s (for example `ACCESS_TOKEN_TTL_SECS: "900"`, `REFRESH_TOKEN_TTL_SECS: "2592000"`, `FOUNDRY_INSTALL`, every `*_UI_URL`, `GANTRY_RUNNER_IMAGE`) |
| `secrets.yaml` | `NAME: ENC[age,...]` lines, encrypted with age; names readable, values not |
| `<service>/` | `config.yaml.j2` (ConfigMap), `secret.yaml.j2`, `deployment.yaml.j2`, `service.yaml.j2`, `ingress.yaml.j2`; extra: `role.yaml.j2` (conveyor, gantry, switchboard), `serviceaccount.yaml.j2`, `runner-credentials.yaml.j2` (gantry), `clients-configmap.yaml.j2` and `permissions-configmap.yaml.j2` (gatehouse) |
| `postgres/`, `redis/`, `searxng/`, `foundry/` | the database, the cache, the search sidecar of Sage, and the migration Job (`job.yaml.j2`, `job-config`, `job-secret`) |

Apply order: postgres, redis, the `foundry` Job (completes), gatehouse, then warehouse, switchboard, searxng, sage, conveyor, workbench, daybook, hoist, gantry, gauge; each resource declares `depends_on`. Deployments pull `ossiriand.arda:8080/forge/<module>/<image>:<version>` (`pull_policy: Always`, `replicas: 1`, `strategy: Recreate`, port 80), read their environment from `<name>-config` and `<name>-secret` by reference (`env_from_config_maps`, `env_from_secrets`; riveter stamps a hash of both on the workload so a changed value rolls it out) and probe `GET /<name>/health/ready` (readiness) and `/<name>/health/live` (liveness). Logs go to a host path volume `/mnt/dev/forge/logs/<name>`.

The age key that opens `secrets.yaml` is not in git (`RIVETER_AGE_KEY`, `RIVETER_AGE_KEY_FILE` or `~/.config/riveter/age.key`); Gantry's runner reads it from the Secret `gantry-age-key`. `homecloud/README.md` describes the day-to-day edits and the rebuild-from-nothing order.

A release of a service is, in outline (*Unverified:* the exact command for service images is not exercised here; `anvil docker release` is the documented image command): `anvil release -p <service>` (bump + image push), bump the image tag in `overlays/forge/<service>/deployment.yaml.j2`, commit, let the pipeline publish the new `forge` package, upgrade it with Gantry or `riveter install`.
