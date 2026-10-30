---
title: Architecture
parent: Hoist
grand_parent: Services
nav_order: 6
---

Hoist is a join of two lists: what Warehouse's registry offers and what is installed on the machine. Everything the screens show is a projection of that join.

## Layers

```text
hoist-core      pure data and functions (no I/O)           quench-serde
hoist-net       blocking client: Gatehouse + Warehouse     gatehouse-client, hoist-core, quench-crypto, quench-serde
hoist-app       screens, Request/Msg protocol              hoist-core, quench-ui, quench-ui-web
hoist-desktop   Linux window + worker + --check            hoist-app, hoist-core, hoist-net, quench-desktop-native, ...
hoist-surface   Android program (surface + --check)        hoist-app, hoist-core, hoist-desktop (no window), quench-android-surface, ...
android/        Kotlin host: plugins, jobs, notification
hoist-service   public catalogue server                    quench-auth, quench-client, quench-db, quench-http, quench-pg, quench-starter
```

The service is independent of the apps: the apps only call its three `GET` routes.

## The app does no I/O

`hoist-app` asks its host for work through a `Request` enum and is told the results through a `Msg` enum (see [Client](client.md)). The desktop host runs the requests on one worker thread; the Android host (`hoist-surface`) uses the same worker pattern and delegates device operations (install, list packages, launch) to the Kotlin `packages` plugin over the surface bridge. No WebView is used by either host.

## Install-state join

`hoist_core::build_entries(catalog, installed, self_program)`:

1. Yanked builds are dropped (`yanked == true`), whatever the registry sent.
2. One row per `program`: the build with the highest `version_code`.
3. The row for the running app (`program == self_program`) gets the label `Hoist` if it has none.
4. State per row (`state_for`): not in `installed` is `NotInstalled`; `installed >= latest` is `UpToDate`; else `UpdateAvailable`.
5. Order: updates first, then up to date, then not installed; inside a group by lowercase title, then program id.

Only programs in the registry are listed ("a store, not a device manager"). `version_code("1.2.3")` is `1002003`: `major * 1_000_000 + minor * 1_000 + patch`, a minor or patch above 999 counts as 999, a non-numeric part counts as 0.

## Registry builds offered on a host

`hoist_desktop::check::offered(rows, arch, formats)` keeps a build when its `arch` is absent, equal to the host architecture or `universal`, and (where `formats` is given) its `format` is in the list. On Linux the formats come from `/etc/os-release` (`ID` and `ID_LIKE`): Arch family `pkg.tar.zst`, Debian family `deb`, Fedora/RHEL/SUSE family `rpm`, anything else `tar.gz`. On Android `formats` is `None` (only APKs exist).

## Background update check

Described in [Client](client.md#background-check). It shares the saved session with the window, so each side reads the token the other stored before refreshing (`Session::following`) because a refresh spends the token.

## Public catalogue (service)

Four modules in `hoist_service`:

| Module | Job |
|---|---|
| `connectors` | Fetch. F-Droid and IzzyOnDroid share the index-v2 format (`fdroid_format`); Aptoide (`aptoide`) browses `apps/get` pages (30 apps per page, sorted by downloads) and resolves each surviving package with one `getMeta` call (concurrency 20). One HTTP client: timeout 120 s, user agent `hoist-service`. |
| `merge` | Pass 1 `dedupe_within_source`: one listing per (source, package), ranked by `malware_rank == "TRUSTED"`, then downloads, then rating count. Pass 2 `merge_all`: one row per package. |
| `store` | `replace_all` rewrites the table in one transaction (`DELETE` then chunked `INSERT`, 200 rows per statement); `search`, `get`, `categories` read it. |
| `refresh` | `fetch_all` fetches all sources at once; `apply` merges and stores; `spawn_scheduler` runs a refresh immediately and then every `HOIST_REFRESH_MINUTES`. |

### Merge rules

* Source priority: `fdroid`, `izzyondroid`, `aptoide` (`DEFAULT_SOURCE_PRIORITY`).
* Install slot: the first source in priority order whose listing has a `download_url`. `version_code`, `version_name`, `size_bytes`, `download_url`, `install_source`, `sha256`, `md5` and `malware_rank` come from that listing.
* `label`, `summary`, `description`, `icon_url`, `category`: per field, the first source in priority order that has a value (`label` falls back to the package name).
* `rating_avg`, `rating_count`, `downloads`: always from the Aptoide listing, if any.
* A package with no listing that has a download URL anywhere gets no row.
* A source that fails counts as empty. When every source fails nothing is written and the previous data stays; if the store itself fails the error is logged.
* Readiness (`HealthState`) waits only for Gatehouse; it does not wait for the first refresh.
