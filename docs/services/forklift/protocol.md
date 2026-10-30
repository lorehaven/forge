---
title: Upload protocol
parent: Forklift
grand_parent: Services
nav_order: 7
---

Forklift uses the Warehouse file API under `{server}/warehouse/api/v1/files`. The routes are documented with Warehouse; this page lists exactly what Forklift calls and how it reads the answers. Authentication is `Authorization: Bearer <access token>` from Gatehouse.

## Calls

| Purpose | Request | Notes |
|---|---|---|
| List storages | `GET /files` | Array of `{name, owner?, quota_bytes?, used_bytes?}`. Forklift keeps only entries **with an owner** (dynamic storages made by people); statically configured storages are not offered. |
| Quota | `GET /files` | The entry whose `name` equals the chosen storage; needs both `used_bytes` and `quota_bytes` or the quota is shown as unknown. |
| Change feed | `GET /files/{storage}/sync?since=<id>` | Array of `{id, path, op, size?}` where `op` is `put` or `delete`. Warehouse answers `409` for a static storage or one without sync enabled; Forklift then recognises nothing that run. Needs `read`. |
| List | `GET /files/{storage}?n=1000[&prefix=][&last=<path>]` | Body `{"entries": [{name, path, kind, size?}]}`; a `Link` response header means more pages, the next `last` is the last entry's `path`. `list_all` follows pages up to 1000 times (at most about a million entries) and keeps `kind == "file"`. A `404` is "no such storage". |
| Upload | `PUT /files/{storage}/file?path=<remote path>` | Body: the raw file bytes; header `Content-Type: application/octet-stream`. Needs `write`. |
| Download | `GET /files/{storage}/file?path=<remote path>` | Raw bytes. |

`{storage}` and `path` are percent-encoded: every byte except ASCII letters, digits, `-`, `.`, `_`, `~` becomes `%XX` (uppercase hex). A `/` in the path is encoded too (`%2F`).

## Remote path

```text
{category}/{yyyy}/{mm}/{local_id}-{filename}
```

* `category`: `photos`, `videos` or `custom:<slug>` (a picked folder). It is used as is.
* `yyyy`/`mm`: year (4 digits) and month (2 digits) of the file's modification time, UTC, from epoch milliseconds (`year_month`).
* `local_id`: the MediaStore id for media, `short_id(uri)` for a folder document or file (16 lowercase hex digits of the 64-bit FNV-1a hash of the URI bytes; offset basis `0xcbf29ce484222325`, prime `0x100000001b3`).
* `local_id` and `filename` are passed through `sanitize`: each character that is not an ASCII letter, digit, `.`, `_` or `-` becomes `_`; an empty result becomes `_`.

Example from the tests: `remote_path("custom:docs", "a/b", "my file.pdf", 2026, 12)` is `custom:docs/2026/12/a_b-my_file.pdf`.

The path is fixed when the file is first started and stored in the manifest (`remote`); later edits of the file overwrite the same remote path.

## Answers

| Status | Meaning to Forklift |
|---|---|
| 200-299 | Stored. |
| 401 | Token refused: refresh once and retry; three in a row pause with "sign in again" (the session file is deleted and the sign-in is shown when the token cannot be refreshed). |
| 403 | The account may not write: pause (`Forbidden`). |
| 404 | No such storage: pause (`NoStorage`). |
| 413 | Too large: the file is marked `too_large` and not retried unless it changes. |
| 507 | Quota full: pause (`Quota`). |
| other / transport | Counted as a failed attempt; retried with back-off (see [Architecture](architecture.md#the-scheduler-forklift_coreengine)). |

For the other reads, `NetError` maps `401` to `Unauthorized`, `403` to `Forbidden`, `404` to `NoStorage`, `413` to `TooLarge`, `507` to `QuotaFull`, and anything else non-2xx to `Status(code)`.

## Reconciliation

Run after every scan. The change feed since the stored cursor (`sync-<storage>` file, a change id) is folded into a map `path -> size` (`delete` removes). For each pending file that has never been sent (`remote` empty, `attempts == 0`):

1. If the computed remote path is in the map with the same size, the file is marked done with that path.
2. Else, if some stored path has the same *identity* `(category, base name, size)`, where the base name is the part of the last path segment after the first `-` (`{id}-{name}` becomes `name`) and the file's own name is sanitised, it is marked done with that path. This recognises files whose device id changed (a restore, a reset).

Note that the cursor is only the checkpoint of the feed: files recognised are relative to the changes since it, so the first run (cursor 0) sees the whole history.

## Restore

`plan_restore` lists the whole storage (`list_all(storage, "")`) and `plan(remote, present, folders)`:

* A stored file is **present** when the manifest (entries not `gone`) has its remote path, or the same `(category, name, size)` identity.
* Otherwise it is restored when its category (first path segment) is `photos`, `videos` or a configured folder's `custom:<slug>`; any other category counts as `no_target`.
* The summary gives `files`, `bytes`, `present`, `no_target`.

`start_restore` fetches the plan with `Restorer` (concurrency `min(settings.concurrency, 2)`, clamped 1..=4; 3 attempts per file, `RESTORE_ATTEMPTS`; failed files are retried before new ones). Each file is fetched with `GET .../file?path=` and the base name above as its file name. After the restore a scan runs, and the pending listing is reconciled so the restored files are not uploaded again.

Targets: `videos` to `video`; a category of a configured folder to `folder` (named by the folder's display name); everything else (photos) to `images`.
