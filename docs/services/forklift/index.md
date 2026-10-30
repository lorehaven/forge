---
title: Forklift
parent: Services
nav_order: 3
has_children: true
---

Forklift backs up photos, videos and chosen folders to a dynamic storage in Warehouse's file API. It is a client of Gatehouse and Warehouse; **there is no server of its own and no HTTP API to describe** (so no `openapi.yaml`). It is the successor of the Flutter app `mobile-apps/mathom`. It runs on Android and on Linux (Wayland window plus a `--backup` mode for a timer).

## Crates

All under `services/forklift/`, members of the Forge workspace. There is no `web` crate.

| Crate | Version | Role |
|---|---|---|
| `forklift-core` | 0.1.0 | Logic with no I/O: the manifest, the scheduler (`Engine`), settings, snapshot, restore planning, remote paths. Dependencies: none (only `quench-test` for tests). |
| `forklift-net` | 0.1.0 | Blocking calls to Gatehouse (`gatehouse-client`) and Warehouse; the on-disk `Store` and the run lock. |
| `forklift-host` | 0.1.0 | `Controller` (the backup's brain), the `Platform` trait, the `Worker` thread. Shared by both hosts. |
| `forklift-app` | 0.1.0 | The screens (`quench-ui`) and the `Request`/`Msg` protocol. |
| `forklift-desktop` | 0.1.3 | The Linux program `forklift` (library `forklift_desktop`): window, `Linux` platform, `--backup`. The app's version is this crate's (`.anvil/forklift.toml`: `version_file = "desktop/Cargo.toml"`). |
| `forklift-surface` | 0.1.0 | The Android program (binary `forklift_surface`, shipped as `libquench_surface.so`): window mode and `--backup` over JSON lines. Re-exports `forklift_host::*`. |
| `android/` | - | Gradle project `dev.lorehaven.forklift`: Kotlin host, plugins `files`, `device`, `restore`, jobs, foreground service. |

Identity (Android application id, Linux id and `wm_class`, registry program): `dev.lorehaven.forklift`.

## Dependencies and authentication

* Gatehouse: sign-in with user name and password once (`gatehouse_client::login`); afterwards only the refresh token is stored. Accounts with MFA cannot sign in.
* Warehouse file API under `/warehouse/api/v1/files` ([upload protocol](protocol.md)): needs `read` and `write` on the chosen storage (the owner of a dynamic storage has both; others need a grant `warehouse:storage:<name>:<action>`).
* Default server offered at sign-in: `https://ennor.ddns.net` (`forklift_core::DEFAULT_SERVER`). Gatehouse is at `/gatehouse`, Warehouse at `/warehouse` under it.

## What it does

1. **Scan** the sources (media library on a phone, picked folders on both), keep one record per file in a [manifest](data.md).
2. **Recognise** what Warehouse already holds (the storage's change feed), so a fresh install does not send everything again.
3. **Send** pending files, newest first, up to `concurrency` (1-5) at a time, each as one `PUT`, with retries and back-off.
4. **Restore** what the device lacks into the media library (Android) or into folders (Linux).

## Pages

* [Architecture](architecture.md): layers, the controller, threads.
* [Upload protocol](protocol.md): the exact calls to Warehouse and what each answer means.
* [Configuration](configuration.md): settings file, environment, defaults.
* [Data](data.md): manifest, settings, snapshot and the other files, byte exact.
* [Client](client.md): screens, requests and messages, platform plugins.
* [Android](android.md): MediaStore vs SAF, jobs, the foreground service, the JSON-line protocol.
* [Operations](operations.md): build, run, release.
