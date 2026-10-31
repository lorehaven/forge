---
title: Operations
parent: Hoist
grand_parent: Services
nav_order: 4
---

## The service

Run locally with Foreman (the estate needs Gatehouse, Redis and Postgres):

```bash
foreman db            # installs the schemas, including `hoist`
foreman start hoist   # https://localhost:14443/hoist
```

The first refresh fetches the three upstream stores over the network; `HOIST_APTOIDE_PAGES=5` keeps a local crawl short. Readiness does not wait for the first refresh, so the API can answer with an empty catalogue for a while.

Tests: `cargo test -p hoist`. Integration tests are in `service/tests/integration.rs`, unit tests (`merge_tests`, `connectors_tests`) in `service/tests/unit/`; see [configuration](configuration.md) for the test variables.

Image: docker package `hoist` in `.anvil/docker.toml` (`module_name = "hoist"`, `image_name = "hoist"`, registries `ossiriand.arda:8080/forge` and `ennor.ddns.net/forge`, `build_args = { RESOURCE_DIR = "config" }`; the service has no translations but the image wants the directory). Release with `anvil release`. The deployment is the `hoist` overlay under `homecloud/overlays/forge/`.

Failure modes: every source failing leaves the old catalogue in place (logged `every source failed; keeping the catalogue as it is`); one failing source counts as empty and its apps vanish from the next catalogue; in-memory database gives `503`.

## The apps

Desktop:

```bash
cargo run -p hoist-desktop            # the window (Wayland)
cargo run -p hoist-desktop -- --check # one background look
```

Android (from `services/hoist`): `quench android build-native` builds `hoist_surface` for `arm64-v8a` and `x86_64` and copies it into the app as `libquench_surface.so`; `quench build apk --release` (what the release target runs) produces the APK; or Gradle in `android/`. After upgrading the quench crates, `quench plugin update` rewrites the Kotlin host. Tests: `cargo test -p hoist-core -p hoist-net -p hoist-app -p hoist-desktop`; `tests/browser.mjs` is a leftover headless-Chromium script of the web template.

## Release

`.anvil/hoist.toml` defines the artifact program `hoist` (`program = "dev.lorehaven.hoist"`, `dir = "services/hoist"`, `version_file = "desktop/Cargo.toml"`). One version number drives all builds. Targets:

| Name | Platform | Format | Arch | Build command | File |
|---|---|---|---|---|---|
| (android) | android | apk | - | `quench build apk --release` | `dist/hoist-${version_name}-${version_code}.apk` |
| `linux-deb` | linux | deb | x86_64 | `quench build deb` | `dist/hoist_${version_name}_amd64.deb` |
| `linux-rpm` | linux | rpm | x86_64 | `quench build rpm` | `dist/hoist-${version_name}-1.x86_64.rpm` |
| `linux-arch` | linux | pkg.tar.zst | x86_64 | `quench build arch` | `dist/hoist-${version_name}-1-x86_64.pkg.tar.zst` |
| `linux-source` | linux | tar.gz | universal | `quench build source` | `dist/hoist-${version_name}.tar.gz` |

```bash
anvil artifact release hoist
```

builds and publishes them to Warehouse's registry, where Hoist itself finds its own updates. `versionCode` is computed from the version as `major * 1_000_000 + minor * 1_000 + patch` (`0.1.7` is `1007`), passed to Gradle as `quench.versionCode`. The Linux package metadata is in `services/hoist/.quench.toml` (`id`/`wm_class` `dev.lorehaven.hoist`, `bin = "hoist"`, `depends = ""`). The Android release key is `release.keystore` (alias `release`); passwords come from the environment only.

Dev profile: `hoist-app` is built with `opt-level = 3` and without debug assertions in the workspace `Cargo.toml` (unoptimised layout and paint cost about 200 ms per change on a phone).
