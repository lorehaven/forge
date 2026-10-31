---
title: Operations
parent: Daybook
grand_parent: Services
nav_order: 4
---

Build, run, release and test Daybook. Nothing here contacts the cluster.

## Run the service locally

```bash
# from forge/
foreman start daybook      # brings up gatehouse (needs), installs schemas through foundry, builds the wasm client, starts daybook
```

`foreman.toml` entry: service `daybook`, package `daybook`, port 12443, `base_path = "/daybook"`, `needs = ["gatehouse"]`, `cert_from = "services/warehouse/service"`, env `SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:12080`. The browser client is at `https://localhost:12443/daybook/ui` (gatehouse's `DAYBOOK_UI_URL`).

Build the browser module by hand:

```bash
cargo build --target wasm32-unknown-unknown --profile wasm -p daybook-web
cp target/wasm32-unknown-unknown/wasm/daybook_web.wasm services/daybook/service/web/app.wasm
# or: scripts/web.sh   (every services/*/web crate)
```

## Run the desktop app

```bash
cargo run -p daybook-desktop            # window; sign-in asks for the server (default https://ennor.ddns.net)
cargo run -p daybook-desktop --example shots -- /tmp/shots   # PNGs of the screens, no window
```

The programs call `{server}/daybook/api/v1` and `{server}/gatehouse/api/v1` (fixed paths in `daybook_net::api`), so the server address must serve both under one origin, as the cluster ingress does. Foreman runs them on separate ports. *Unverified:* a one-origin local setup.

## Android

```bash
cd services/daybook
quench android build-native        # builds daybook_surface and copies it into the app as libquench_surface.so
quench build apk --release         # signed with release.keystore (passwords from the environment)
```

Debug on an emulator uses the Gradle project in `android/` (`./gradlew installDebug`, emulator-5554). `quench plugin update` rewrites the Kotlin host files after the quench crates are upgraded.

`versionName` is the Cargo version of `daybook-desktop` (`version_file`), `versionCode = major*1_000_000 + minor*1_000 + patch` (0.1.9 is 1009), the same on every platform; `.quench.toml` can override both.

## Release

`.anvil/daybook.toml` defines program `dev.lorehaven.daybook` (dir `services/daybook`, version from `desktop/Cargo.toml`) with targets: Android APK, Linux `deb`, `rpm`, Arch `pkg.tar.zst`, source `tar.gz`. Files land in `services/daybook/dist/` as `daybook-<version>-<versionCode>.apk`, `daybook_<version>_amd64.deb`, `daybook-<version>-1.x86_64.rpm`, `daybook-<version>-1-x86_64.pkg.tar.zst`, `daybook-<version>.tar.gz`.

```bash
anvil artifact release daybook      # builds the targets and uploads them to the warehouse (artifacts.warehouse_url)
```

Raise the `daybook-desktop` version first; a versionCode must grow with every release.

## Service image and cluster

* Image `ossiriand.arda:8080/forge/daybook/daybook:<service version>` (currently 0.3.2) built by `anvil` from `.anvil/docker.toml` module `docker.daybook-service` with build args `RESOURCE_DIR=web`, `WASM_PACKAGE=daybook-web` (the image builds the wasm client into `web/`). Registries: `ossiriand.arda:8080/forge`, `ennor.ddns.net/forge`.
* homecloud overlay `overlays/forge/daybook/`: `config.yaml.j2` (ConfigMap `daybook-config`), `secret.yaml.j2` (`daybook-secret`: `DATABASE_URL`), `deployment.yaml.j2`, `service.yaml.j2`, `ingress.yaml.j2`. One replica, strategy Recreate, container port 80, CPU 50m/200m, memory 64Mi/256Mi, runs as uid 0, logs on a host path mounted at `/logs`. Depends on the foundry job and the gatehouse deployment. Probes: readiness `GET /daybook/health/ready` (initial delay 3 s, period 5 s), liveness `GET /daybook/health/live` (15 s, 10 s). Ingress host `ennor.ddns.net`, prefix `/daybook`, Let's Encrypt, SSL redirect, proxy timeouts 60 s.
* Readiness: `HealthState` turns ready once gatehouse's `/health/ready` answers (`wait_for_services`).
* Foundry installs the `daybook` module (`FOUNDRY_INSTALL` includes `daybook`).

## Failure modes

* Gatehouse down: readiness stays false; `/auth/*` answer 502; API calls with already-verified tokens keep working as JWKS is fetched by `JwtConfig` (behaviour of `quench-auth`; *unverified* for a restart without gatehouse).
* No Redis in a deployment with `REDIS_URL` set but unreachable: startup panics (`expect("session store")`). With the variable unset, an in-process store is used.
* In-memory database: every data route answers 503.
* Missing `app.wasm`: `/ui/assets/app.wasm` answers 404 and the client does not start.

## Tests

| Suite | Command | Notes |
|---|---|---|
| service unit | `cargo test -p daybook --test unit` | 16 tests |
| recurrence arithmetic | `cargo test -p daybook --test recurrence` | 28 tests, pure |
| service integration | `DAYBOOK_TEST_DATABASE_URL=<url> cargo test -p daybook --test integration` | skipped (prints "skipping ...") unless the variable names a throwaway Postgres with the foundry `daybook` module installed; 27 test functions incl. helpers |
| core, net, host, app, desktop, surface, web | `cargo test -p daybook-core` etc. | each has one `harness = false` test target: `unit`, `controller` (host), `screens` (app) |
| end to end | `cargo run -p forge-bdd -- --service daybook` | starts gatehouse and daybook, features under `tests/forge-bdd/features/daybook` |

`quench-test` is the test framework (no libtest harness).
