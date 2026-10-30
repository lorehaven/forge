---
title: Data
parent: Forklift
grand_parent: Services
nav_order: 2
---

Everything is plain text in the [data directory](configuration.md#data-directory). `Store` writes a file to `<name>.tmp`, `sync_all`s it and renames it over the target, so a crash leaves the old or the new content; the `session` file is created with mode `0600`.

| File | Content | Writer |
|---|---|---|
| `session` | The refresh token plus `\n` (trimmed on read). | `Store::set_token`; removed by `clear_token`. |
| `settings` | [Settings file](#settings-file). | `Store::set_settings` |
| `manifest` | [Manifest file](#manifest-file). | `Store::set_manifest` / `set_manifest_in_background` |
| `snapshot` | [Snapshot file](#snapshot-file). | `Store::set_snapshot`; removed by `clear_snapshot` (on sign-out). |
| `checked` | Epoch milliseconds of the last completed scan, decimal, no newline. | `set_checked_ms` |
| `sync-<storage>` | The last change-feed id read for that storage, decimal. `<storage>` is passed through `sanitize`. Absent means 0. | `set_sync_cursor` |
| `run.lock` | Empty. Held with an exclusive non-blocking file lock (`File::try_lock`) by whichever program has the backup. | `try_lock_run` |
| `hidden` | Empty marker created by the Android host while the window is in the background (see [Android](android.md#who-sends-files)). | Kotlin `Backup.windowClosed` |

Manifest writes are numbered: a write with a lower number than the last one written for that directory is skipped, so a slow background write cannot overwrite a newer one. `Store::manifest()` first waits for pending background writes, then reads the file and calls `Manifest::recover()` (files left `uploading` go back to `pending`).

## Escaping

Used in the manifest, settings and snapshot rows (`escape` / `unescape`): `\` becomes `\\`, TAB `\t`, LF `\n`, CR `\r`. When reading, `\t`, `\n`, `\r` are decoded; `\` followed by any other character yields that character; a trailing lone `\` stays `\`.

## Manifest file

```text
forklift-manifest 1
<category>\t<local_id>\t<name>\t<uri>\t<size>\t<modified_ms>\t<status>\t<attempts>\t<retry_at_ms>\t<error>\t<remote>\t<gone>
```

First line is the header (not checked on read: it is skipped). Then one line per record, in key order (`category`, 0x1F, `local_id`, sorted as strings). 12 tab-separated fields:

| # | Field | Format |
|---|---|---|
| 1 | category | escaped string (`photos`, `videos`, `custom:<slug>`) |
| 2 | local_id | escaped string |
| 3 | name | escaped string |
| 4 | uri | escaped string (content URI on Android, path on Linux) |
| 5 | size | unsigned decimal, bytes |
| 6 | modified_ms | signed decimal, epoch milliseconds |
| 7 | status | `pending`, `uploading`, `done`, `failed`, `too_large` |
| 8 | attempts | unsigned decimal |
| 9 | retry_at_ms | signed decimal, epoch ms; 0 is "now" |
| 10 | error | escaped; empty when none |
| 11 | remote | escaped; empty until first sent |
| 12 | gone | `1` or `0` |

Reading accepts 11 fields (older files, `gone` = false) or 12; any other line, or one whose numeric fields or status do not parse, is skipped. The Android host reads the file itself to look for work (`Backup.workWaiting` reads field 7 of each line after the header: `pending` or `uploading`; `Backup.updateStatus` counts `pending`, `uploading`, `failed`).

Status lifecycle:

* `pending`: new, changed, retried, released.
* `uploading`: started; found at load it means the app died, and `recover` makes it pending.
* `done`: Warehouse has this version.
* `failed`: 5 attempts used; reset by a manual scan (`retry_failed`) or when the file changes.
* `too_large`: the server answered 413; only a change of `size`/`modified_ms` resets it.

Scan rules (`Manifest::scan(category, found, retry_failed)`): a file is identified by `(category, local_id)`. New: added `pending`. Known with different `size` or `modified_ms`: counted changed, name/size/mtime updated, set `pending`, attempts and retry and error cleared (the old `remote` is kept). Known with the same `(size, modified)`: unchanged (a `failed` one is reset only when `retry_failed`). In all known cases `gone` is cleared and `uri` updated.

After a scan that read everything, `mark_missing_ids(category, seen)`: records of that category not seen and not `done` are **deleted** (nothing to send any more); `done` ones are kept with `gone = 1` as the record of what the storage holds.

`forget_category` (a folder removed) drops its records; the Warehouse copies stay. `reset` (storage changed) makes a record pending, clears attempts, retry, error **and remote**.

## Settings file

`key=value` lines; unknown lines and lines without `=` are ignored (a newer file can be read by an older app). Values `1` mean true; anything else false. Strings are escaped.

```text
server=https://ennor.ddns.net
storage=moto_edge_60_pro
storage_confirmed=1
auto=1
photos=1
videos=1
videos_wifi_only=0
charging_for_large=1
concurrency=3
folder=<slug>\t<name>\t<uri>\t<enabled>\t<wifi_only>
```

The `folder` line has exactly 5 tab-separated fields (a line with another count is dropped). Slug: `storage_name(name)` made unique among existing folders by appending `_2`, `_3`, ... (`folder_slug`). The category is `custom:<slug>`. `uri` is the tree URI the Storage Access Framework granted on Android, or an absolute path on Linux.

Parsed order of categories (`Settings::categories`): `photos` (label `Photos`, if enabled), `videos` (`Videos`, `wifi_only` from `videos_wifi_only`), then each enabled folder (label = its name, `wifi_only` from the folder).

## Snapshot file

What the dashboard shows at the next start before the manifest is read.

```text
forklift-snapshot 1
total <pending> <uploading> <done> <failed> <too_large> <done_bytes> <waiting_bytes>
row <key>\t<label>\t<pending> <uploading> <done> <failed> <too_large> <done_bytes> <waiting_bytes>
checked <epoch ms>          (optional)
quota <used> <quota>        (optional)
```

Seven decimal numbers separated by single spaces. Any parse error yields the empty snapshot. `done_bytes` sums sizes of `done`; `waiting_bytes` sums `pending`, `uploading` and `failed` (not `too_large`).

Dashboard headline (`Snapshot::headline`), in this order: the pause text if paused; with no files, `Looking for files` (while working or not yet settled) or `Nothing to back up yet`; `Backing up: <done> of <files> files` while any pending/uploading; `<n> file(s) could not be backed up` for failures; else `All <done> files are backed up`. `files()` is `done + pending + uploading + failed`. Sizes print in decimal units (`950 B`, `12.5 KB`, `3.4 GB`, base 1000).
