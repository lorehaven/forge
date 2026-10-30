---
title: Architecture
parent: Forklift
grand_parent: Services
nav_order: 6
---

```text
forklift-core   manifest, engine, settings, snapshot, restore, remote_path       (no dependencies)
forklift-net    Session (gatehouse-client), Store, RunLock                        forklift-core, gatehouse-client, quench-serde
forklift-app    screens + live::{Request, Msg, Setup}                              forklift-core, quench-ui
forklift-host   Controller, Platform trait, Worker                                 forklift-app, forklift-core, forklift-net
forklift-desktop  Linux platform, window, --backup                                 forklift-app/-core/-host/-net, quench-desktop-native, quench-style, quench-ui, quench-ui-native
forklift-surface  Android program, headless --backup                               forklift-app/-core/-host/-net, quench-android-surface, quench-serde, quench-style, quench-ui, quench-ui-native
android/        Kotlin host                                                        (Gradle)
```

Direction of dependencies: `core` knows nothing; `net` and `app` know `core`; `host` knows all three; the programs know everything.

## The scheduler (`forklift_core::Engine`)

`Engine { manifest, active, next, concurrency, auth_refusals, paused }`. It does no I/O: it is told what ended and answers what to start.

* `Engine::new(manifest, concurrency)`: concurrency clamped to 1..=5.
* `pump_where(now_ms, charging, allowed)`: if paused, nothing. Else takes up to `concurrency - running` files from `Manifest::next_batch_where` (pending, `retry_at_ms <= now`, large files only when `charging`, category `allowed`; newest `modified_ms` first, ties by larger `local_id`). Each gets an upload id `u1`, `u2`, ... and a remote path fixed at the first send (`remote_path(category, local_id, name, year, month)` from the file's modification time, UTC); a later edit keeps the same path, so the edited file replaces its own copy.
* `finish(upload_id, outcome, now_ms) -> Verdict`:

| Outcome | Manifest | Verdict |
|---|---|---|
| HTTP 200-299 | `done`, attempts reset | `Done` |
| HTTP 413 | `too_large` (error text `larger than the server accepts`) | `Retry` (nothing is retried for this file) |
| HTTP 507 | released (back to pending, no failure counted) | `Paused(Quota)` |
| HTTP 404 | released | `Paused(NoStorage)` |
| HTTP 403 | released | `Paused(Forbidden)` |
| HTTP 401 | released; `auth_refusals += 1` | `Reauthenticate`, but the third consecutive 401 (`> 2`) gives `Paused(SignedOut)` |
| other HTTP | `failed(..., "the server answered N")` | `Retry` |
| transport failure | `failed(..., message)` | `Retry` |

`failed` increments attempts, stores the message and either sets `pending` with `retry_at_ms = now + backoff_ms(attempts)` or, at `MAX_ATTEMPTS` (5), `failed` (status) until the person starts a manual scan. `backoff_ms(n)`: `30 s << (n-1)` with the shift capped at 7, capped at 3600 s: 30 s, 1 min, 2, 4, 8, 16, 32, 60 min.

`Pause` variants and their texts: `Quota` "the storage is full: free some space on the server, then back up again"; `SignedOut` "the session ended: sign in again"; `NoStorage` "the server has no storage with that name: check it in the settings"; `Hidden` "the backup goes on in the background"; `Forbidden` "this account may not write to this storage: ask for write access to it in gatehouse, or choose another storage".

## The controller (`forklift_host::Controller`)

One thread owns it (`Rc<Controller>`, `RefCell` state). It touches the device only through the `Platform` trait:

| `Platform` method | Purpose |
|---|---|
| `media(kind, offset, then)` | A page of up to `MEDIA_PAGE` (2000) files of the media library, `images` or `video`. |
| `folder(uri, then)` | What is directly in a picked folder (`Node::File(Found)` or `Node::Dir(uri)`). |
| `upload(id, uri, url, token)` / `cancel(id)` | Stream a file to a URL; the host reports with `Controller::upload_event(UploadEvent::{Done{id,status}, Failed{id,message}})`. |
| `changes(storage, since, then)`, `listing(storage, then)`, `quota(storage, then)`, `storages(then)` | Warehouse reads (done by the worker thread). |
| `download(id, url, token, name, target)` / `cancel_download(id)` | Restore one file; reported with `Controller::download_event(id, ok)`. |
| `token(force, then)` | An access token with life left; `force` refreshes. |
| `device(then)` | `Device { online, wifi, charging }`. |
| `pick_folder(then)` | Let the person pick a folder: `(name, uri)`. |
| `now_ms()`, `later(ms, f)` | Clock and timer. |

Main operations:

* `start()`: does nothing until `storage_confirmed`; then `scan(false)` and a rescan timer every `RESCAN_MS` = 10 minutes (runs `scan(false)` when `auto`).
* `scan(manual)`: reads every category in order (`Settings::categories()`: `photos`, `videos`, then enabled folders). Media is read in pages of 2000; a full page means ask for the next. Folders are walked depth first with a queue. Each page goes into the manifest as it arrives (`Manifest::scan`), so the dashboard counts grow during the scan. Only a scan that read everything (`scan_ok`) calls `mark_missing_ids`. A folder document's `local_id` is `short_id(uri)`. `manual == true` also resumes a paused engine and gives failed files their attempts back.
* `finish_scan()` then `reconciled()`: asks `changes(storage, since_cursor)`; replays the feed (put/delete) into a path-to-size map, stores the last change id as the cursor, and `Manifest::reconcile`s pending fresh files against it. A failed feed (for example `409`) is ignored: nothing is recognised that time. Then the manifest is saved in the background, the snapshot published, `pump()` and a quota refresh follow.
* `pump()`: asks `device()`; offline means nothing starts. Else `pump_where(now, charging || !charging_for_large, allowed)` where a category with `wifi_only` is allowed only on `wifi`. If anything starts it takes a token (`token(false)`) and calls `upload(id, uri, upload_url(...), token)` for each. If nothing starts, `schedule_retry()` sets a timer for the earliest `retry_at_ms` + 50 ms.
* `upload_event`: runs `Engine::finish`; saves the manifest when idle, or at most every `SAVE_EVERY_MS` (3000 ms) in the background; publishes the snapshot at most every `PUBLISH_EVERY_MS` (250 ms) during bursts. `Done`/`Retry` pump again; `Reauthenticate` forces a token refresh then pumps; `Paused(why)` stops the engine, saves, shows a notice.
* Storage chooser: `list_storages()` keeps only storages **with an owner** (dynamic ones), `choose_storage(name)` sets `storage_confirmed`. Changing the storage later resets every `done` record to pending, so everything is looked at again against the new storage.
* `plan_restore()`, `start_restore()`, `cancel_restore()`: see [Protocol](protocol.md#restore).
* `pause_in_background()` / `resume_in_foreground()`: the window gives the backup to the background run and takes it back (see [Android](android.md#who-sends-files)).

## Threads

The views' thread owns the controller. `Worker` runs Warehouse reads on short-lived threads that lock the shared `Option<Session>` mutex, and posts the answer back as a message (`Worker::complete(id, answer)`), where the waiting callback runs. Uploads and downloads run on the host's own threads (Kotlin threads on Android, `std::thread` on Linux).

Sessions: `session_for(store, server, token)` is `Session::resume(...)` with `keeping_tokens` writing each new refresh token to the store's `session` file and `following` reading the newest stored one before refreshing, so the window and the background run can share one session.
