---
title: Operations
parent: Forklift
grand_parent: Services
nav_order: 3
---

Forklift has no server to run. This page is about building, running and shipping the apps.

## Run on the desktop

```bash
cd services/forklift   # or from the workspace root
cargo run -p forklift-desktop          # the window (Wayland)
cargo run -p forklift-desktop -- --backup   # one backup, no window
```

The window needs a Wayland session and a reachable Gatehouse/Warehouse (default server `https://ennor.ddns.net`). State goes to `~/.local/share/forklift`. Do not run two instances against the same directory: the second `--backup` finds `run.lock` held and exits.

## Run on Android

1. `quench android build-native` (from `services/forklift`): builds the `forklift_surface` program for `arm64-v8a` and `x86_64` and copies it to `android/app/src/main/jniLibs/<abi>/libquench_surface.so`.
2. `quench build apk --release` (release, signed from the environment) or Gradle in `android/` (`./gradlew assembleDebug`, the debug app is `dev.lorehaven.forklift.debug`).
3. Install on the emulator or a device with `adb`. After upgrading the quench crates, `quench plugin update` rewrites the Kotlin host.

Useful logs: tag `forklift-backup` (run, jobs, service) and `forklift-files` (media query timing). The background program's standard error is forwarded to the `forklift-backup` tag.

## Tests

```bash
cargo test -p forklift-core -p forklift-net -p forklift-app -p forklift-host -p forklift-desktop -p forklift-surface
```

Layout: `core/tests/unit.rs`, `net/tests/unit.rs`, `host/tests/controller.rs` (the controller against a fake `Platform`), `app/tests/screens.rs`, `desktop/tests/unit.rs`, `surface/tests/headless.rs` (the `--backup` protocol with scripted input). All are harness-less `quench-test` binaries.

## Release

`.anvil/forklift.toml` defines the artifact program `forklift` (`program = "dev.lorehaven.forklift"`, `dir = "services/forklift"`, `version_file = "desktop/Cargo.toml"`; the Android app takes the same version). Targets:

| Name | Platform | Format | Arch | Build command | File |
|---|---|---|---|---|---|
| (android) | android | apk | - | `quench build apk --release` | `dist/forklift-${version_name}-${version_code}.apk` |
| `linux-deb` | linux | deb | x86_64 | `quench build deb` | `dist/forklift_${version_name}_amd64.deb` |
| `linux-rpm` | linux | rpm | x86_64 | `quench build rpm` | `dist/forklift-${version_name}-1.x86_64.rpm` |
| `linux-arch` | linux | pkg.tar.zst | x86_64 | `quench build arch` | `dist/forklift-${version_name}-1-x86_64.pkg.tar.zst` |
| `linux-source` | linux | tar.gz | universal | `quench build source` | `dist/forklift-${version_name}.tar.gz` |

```bash
anvil artifact release forklift
```

builds all targets and publishes them to Warehouse's artifact registry, from where Hoist installs and updates Forklift. `versionCode` is `major * 1_000_000 + minor * 1_000 + patch` of the version (`0.1.1` gives `1001`, matching `dist/forklift-0.1.1-1001.apk`). The Linux package settings are in `.quench.toml` (`id` and `wm_class` `dev.lorehaven.forklift`, `bin = "forklift"`, `icon = "icon.png"`, `depends = ""`: a native app needs nothing but libc). Android signing: `release.keystore`, alias `release`, passwords in the environment only.

## Performance profile

The workspace `Cargo.toml` builds `forklift-app`, `forklift-core`, `forklift-host` and `forklift-net` (and the renderer crates) with `opt-level = 3` and without debug assertions or overflow checks in the dev profile: the manifest of a library with tens of thousands of files, and screens that are laid out on every change, are too slow unoptimised.

## Failure modes

| Symptom | Cause |
|---|---|
| Nothing is sent after sign-in | `storage_confirmed` is 0: choose a storage. |
| Banner "the storage is full ..." | Warehouse answered 507 (quota). |
| "this account may not write to this storage" | 403: grant `warehouse:storage:<name>:write` (or use a storage the account owns). |
| "the server has no storage with that name" | 404: the storage was deleted; choose another. |
| Files over 200 MB stay pending | `charging_for_large` and the device is not charging. |
| Files marked "larger than the server accepts" | 413; on Linux also every file over 512 MiB. |
| Window shows numbers but nothing moves (Android) | Another program holds `run.lock` (a background run); the window waits and takes over when it ends. |
| Android 15 stops the foreground service | `onTimeout`; the jobs carry on. |
