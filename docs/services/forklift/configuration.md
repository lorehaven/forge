---
title: Configuration
parent: Forklift
grand_parent: Services
nav_order: 1
---

Forklift has no server configuration. State lives in the data directory; the choices live in the `settings` file.

## Data directory

`Store::standard()`: `$XDG_DATA_HOME/forklift` when `XDG_DATA_HOME` is set and absolute, else `$HOME/.local/share/forklift`, else the temp dir. On Android the Kotlin host starts the program with `HOME` set to the app's `files` directory, so the directory is `files/.local/share/forklift`.

## Settings (`forklift_core::Settings`)

| Key (file) | Field | Default | Meaning |
|---|---|---|---|
| `server` | `server` | empty (the sign-in offers `https://ennor.ddns.net`) | `https://host`, trimmed, no trailing `/` (`clean_server`). Gatehouse at `/gatehouse`, Warehouse at `/warehouse`. Set at sign-in; the settings screen shows it but never changes it. |
| `storage` | `storage` | Android: from the device model (`getprop ro.product.model` lowercased, runs of other characters become one `_`, trimmed, empty becomes `device`; `Moto Edge 60 Pro` becomes `moto_edge_60_pro`). Linux: the same rule applied to `/etc/hostname` (else `$HOSTNAME`). | The Warehouse storage this device writes to. |
| `storage_confirmed` | `storage_confirmed` | `0` | The person chose the storage from the list. **Nothing is scanned or sent before this is 1.** |
| `auto` | `auto` | `1` | Automatic backup (new files, every so often, even when closed). |
| `photos` | `photos` | `1` | Back up the photos category (phone). |
| `videos` | `videos` | `1` | Back up the videos category (phone). |
| `videos_wifi_only` | `videos_wifi_only` | `0` | Videos wait for Wi-Fi (or Ethernet). |
| `charging_for_large` | `charging_for_large` | `1` | Files over `LARGE_FILE_BYTES` (200 * 1024 * 1024 = 209 715 200 bytes) wait for the charger. |
| `concurrency` | `concurrency` | `3` | Files sent at once, clamped 1..=5 (a non-number reads as 3). |
| `folder` (repeatable) | `folders` | none | Picked folders, see [Data](data.md#settings-file). |

On the desktop the toggles for photos, videos, Wi-Fi and charging are hidden (`Setup.phone == false`), but the defaults still list the `photos` and `videos` categories; on Linux the media source returns no files, so they stay empty.

## Environment

| Variable | Used by | Meaning |
|---|---|---|
| `XDG_DATA_HOME`, `HOME` | `Store::standard`, restore folder | Where state is kept; the restore directory is `$HOME/Forklift Restore`. |
| `HOSTNAME` | `default_storage` | Fallback for `/etc/hostname`. |
| `FORKLIFT_TRACE` | `forklift-net` | Passed to `gatehouse_client::trace_var`: request tracing. |

Limits compiled in: `MAX_ATTEMPTS = 5`, `LARGE_FILE_BYTES = 200 MiB`, `MEDIA_PAGE = 2000`, `RESCAN_MS = 10 min`, `SAVE_EVERY_MS = 3000`, `PUBLISH_EVERY_MS = 250`, `RESTORE_ATTEMPTS = 3`, Linux `MAX_FILE_BYTES = 512 MiB`.
