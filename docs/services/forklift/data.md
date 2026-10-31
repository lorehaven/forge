---
title: Data
parent: Forklift
grand_parent: Services
nav_order: 2
---

The files are in the [data directory](configuration.md#data-directory). All but the manifest are plain text: `Store` writes a file to `<name>.tmp`, `sync_all`s it and renames it over the target, so a crash leaves the old or the new content; the `session` file is created with mode `0600`. The manifest is a SQLite database (see [the manifest database](#the-manifest-database)).

| File | Content | Writer |
|---|---|---|
| `session` | The refresh token plus `\n` (trimmed on read). | `Store::set_token`; removed by `clear_token`. |
| `settings` | [Settings file](#settings-file). | `Store::set_settings` |
| `manifest.db` | [The manifest database](#the-manifest-database). | `Store::set_manifest` / `set_manifest_in_background` |
| `manifest.counts` | Three lines the Android host (Kotlin) reads without opening the database: `forklift-counts 1`, `pending N`, `uploading N`, `failed N` (four lines with the header). Written with every manifest save. | `Store` |
| `manifest`, `manifest.imported` | The manifest of versions before the database, one text line per record. The first `Store::manifest()` that finds an empty database and this file reads it into the database and renames it to `manifest.imported`; it is never read again. | `Store` (import only) |
| `snapshot` | [Snapshot file](#snapshot-file). | `Store::set_snapshot`; removed by `clear_snapshot` (on sign-out). |
| `checked` | Epoch milliseconds of the last completed scan, decimal, no newline. | `set_checked_ms` |
| `sync-<storage>` | The last change-feed id read for that storage, decimal. `<storage>` is passed through `sanitize`. Absent means 0. | `set_sync_cursor` |
| `run.lock` | Empty. Held with an exclusive non-blocking file lock (`File::try_lock`) by whichever program has the backup. | `try_lock_run` |
| `hidden` | Empty marker created by the Android host while the window is in the background (see [Android](android.md#who-sends-files)). | Kotlin `Backup.windowClosed` |

Manifest writes are numbered: a write with a lower number than the last one written for that directory is skipped, so a slow background write cannot overwrite a newer one. `Store::manifest()` first waits for pending background writes, then reads the database and calls `Manifest::recover()` (files left `uploading` go back to `pending`).

## Escaping

Used in the settings and snapshot rows, and by the importer of the old text manifest (`escape` / `unescape`): `\` becomes `\\`, TAB `\t`, LF `\n`, CR `\r`. When reading, `\t`, `\n`, `\r` are decoded; `\` followed by any other character yields that character; a trailing lone `\` stays `\`.

## The manifest database

`manifest.db` is a SQLite 3 database file made and read by `quench-sqlite` (any `sqlite3` can open it for a look). One table, `items`, has a row per record:

```sql
CREATE TABLE items (
    category TEXT NOT NULL, local_id TEXT NOT NULL, name TEXT NOT NULL, uri TEXT NOT NULL,
    size INTEGER NOT NULL, modified_ms INTEGER NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL,
    retry_at_ms INTEGER NOT NULL, error TEXT NOT NULL, remote TEXT NOT NULL, gone INTEGER NOT NULL,
    PRIMARY KEY (category, local_id));
CREATE INDEX items_status ON items (status, retry_at_ms);
```

| Column | Meaning |
|---|---|
| category | `photos`, `videos`, `custom:<slug>` |
| local_id | what the source calls the file |
| name, uri | file name; where to read it (a content URI on Android, a path on Linux) |
| size, modified_ms | bytes; epoch milliseconds |
| status | `pending`, `uploading`, `done`, `failed`, `too_large` |
| attempts, retry_at_ms | tries so far; epoch ms, not before this (0 is "now") |
| error | the last failure in words a person reads; empty when none |
| remote | the remote path it was sent to; empty until first sent |
| gone | `1` when the last scan did not find it on the device any more, else `0` |

The program keeps the whole manifest in memory (`Manifest`) and `Store` makes the database match it: a save reads the rows, compares, and writes only the records that are new or different and deletes those that are gone, in **one transaction**. A save of an unchanged manifest writes nothing. A crash leaves the manifest as it was before the save or as it is after, never half (`quench-sqlite` journals the pages it changes).

* The file is opened for each read or save and closed again; an open takes an exclusive file lock. Another program holding it (the window and the background run both read the manifest) is waited for, up to 3 seconds. The right to *send files* is still the `run.lock`.
* If the manifest cannot be read (the database is damaged, or busy for the whole 3 seconds), `Store::manifest()` returns an empty one **and marks the directory unreadable: nothing is saved there until a later read works**, so an empty manifest never replaces the real one.
* A record that does not read back (a missing column, a status that is not one of the five) is skipped; the next scan looks at the file again.
* `Backup.kt` does not open the database: it reads `manifest.counts` (and, until the first save after an upgrade, the old text `manifest`).

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
