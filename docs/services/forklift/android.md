---
title: Android
parent: Forklift
grand_parent: Services
nav_order: 5
---

Application id `dev.lorehaven.forklift`, `minSdk 26`, `targetSdk` and `compileSdk` 36, `versionCode`/`versionName` passed by `quench build apk` as `quench.versionCode`/`quench.versionName` (defaults 1 and `0.1.0`). Debug builds have the id suffix `.debug`. Package `dev.lorehaven.forklift`; Kotlin in `android/app/src/main/kotlin/dev/lorehaven/forklift/`.

## Manifest

Permissions: `INTERNET`, `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO`, `READ_MEDIA_AUDIO`, `READ_EXTERNAL_STORAGE` (`maxSdkVersion 32`), `ACCESS_NETWORK_STATE`, `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC`. Components: `MainActivity` (exported, web host), `SurfaceActivity` (launcher), `BackupJobService` (`BIND_JOB_SERVICE`), `BackupReceiver` (`BOOT_COMPLETED`, `MY_PACKAGE_REPLACED`), `BackupService` (foreground service type `dataSync`). `allowBackup=false`.

## MediaStore versus the Storage Access Framework

| | Media library (photos, videos) | Picked folders |
|---|---|---|
| API | `MediaStore.Images/Video.Media.EXTERNAL_CONTENT_URI` queried with `ContentResolver.query` | `Intent.ACTION_OPEN_DOCUMENT_TREE`, then `DocumentsContract` child queries |
| Permission | Android 13+ `READ_MEDIA_IMAGES` / `READ_MEDIA_VIDEO` (asked by the plugin on first use); older: `READ_EXTERNAL_STORAGE` | None in the manifest; the grant comes from the picker and is kept with `takePersistableUriPermission(FLAG_GRANT_READ_URI_PERMISSION)` |
| Identity (`local_id`) | The last path segment of the content URI = the MediaStore `_ID` | `short_id(document URI)` (16 hex digits) |
| Name / size / mtime | `DISPLAY_NAME`, `SIZE`, `DATE_MODIFIED` * 1000 | `COLUMN_DISPLAY_NAME`, `COLUMN_SIZE` (null becomes -1, which the Rust side reads as 0), `COLUMN_LAST_MODIFIED` (milliseconds; null becomes 0) |
| Order and paging | `DATE_MODIFIED DESC, _ID DESC`; the window pages by key (`before_modified` seconds and `before_id`: `DATE_MODIFIED < ? OR (DATE_MODIFIED = ? AND _ID < ?)`) because an offset makes MediaStore walk every skipped row; the headless run uses `offset` | Walked depth first by the controller (`list_folder {uri, limit}` per directory, limit 100000 from the window) |
| Writing back (restore) | Not needed | Not possible: the app has only read access to a picked folder, so a folder's files are restored to `Download/Forklift/<folder>` |

The audio collection is supported by the `files.media` plugin method (`kind` `images`, `video` or `audio`), but Forklift never asks for it.

## The `files` plugin (`FilesPlugin.kt`)

| Method | Arguments | Result |
|---|---|---|
| `media` | `kind`, `limit` (1..5000, default 50; Forklift sends 2000), `offset` or `before_modified` + `before_id` | Array of `{uri, name, size, modified_ms}` |
| `pick_folder` | none | `{uris: [..]}` (request code 0x5101) or reject `cancelled` |
| `pick_files` | `multiple` (default true) | same, request code 0x5102 |
| `list_folder` | `uri`, `limit` (default 500) | Array of `{uri, name, mime, is_dir, size, modified_ms}` |
| `stat` | `uri` | `{name, size, mime}` |
| `read` | `uri`, `offset`, `length` (1..256 KiB) | `{data (base64), size, eof}` |
| `upload` | `id`, `uri`, `url`, `method` (default `PUT`), `headers` | Answers at once `{id, total}`, then events `progress {id, sent, total}` (at most every 200 ms) and `done {id, status, body}` (first 4096 bytes) or `failed {id, message}` |
| `cancel_upload` | `id` | Disconnects |

An upload streams the file with `HttpURLConnection` using `setFixedLengthStreamingMode(total)` (or chunked 64 KiB when the size is unknown) and a 64 KiB buffer, so no file is held in memory; this is why the phone has no size limit. Errors: `permission_denied`, `not_found`, `invalid_args`, `unknown_method`.

## The `device` plugin

`state` returns `{online (validated network), transport (wifi|cellular|ethernet|other|none), metered, battery_percent, charging, battery_low (<= 15 and not charging), power_save}`; `watch` pushes `state` events when the state string changes, `unwatch` stops. Forklift uses `online`, `charging` and `transport` (`wifi` or `ethernet` counts as Wi-Fi).

## The `restore` plugin

`save {id, url, token, name, kind: images|video|folder, folder}` answers at once and streams the download (connect timeout 15 s, read timeout 120 s, `Authorization: Bearer <token>`) into a new MediaStore entry with `IS_PENDING = 1`, then clears the flag so other apps see only complete files; a failed download deletes the half-written entry. Events `done {id}` / `failed {id, message}`. `cancel {id}`.

| `kind` | Collection | `RELATIVE_PATH` |
|---|---|---|
| `images` | `MediaStore.Images` (primary volume) | `Pictures/Forklift` |
| `video` | `MediaStore.Video` | `Movies/Forklift` |
| `folder` | `MediaStore.Downloads` | `Download/Forklift/<folder>` (`/` in the name becomes `_`) |

The MIME type is guessed from the extension (`application/octet-stream` otherwise). Needs Android 10 (API 29) or newer (`restoring needs Android 10 or newer`); needs no permission.

## Who sends files

Exactly one program sends files at a time. `Backup.foreground` is true while the window is up.

1. **Window** (`SurfaceActivity` runs `forklift_surface` in window mode): `onCreate` and `onStart` call `Backup.windowOpened` (sets `foreground`, deletes the `hidden` marker, stops any running background program), `Backup.schedule` registers the jobs, and `POST_NOTIFICATIONS` is asked once (Android 13+). The program holds `run.lock`.
2. `onPause` calls `Backup.windowPaused`: `foreground = false`; if automatic backup is on and work waits, a **foreground service** (`BackupService`, notification id 3, `FOREGROUND_SERVICE_TYPE_DATA_SYNC`, a `Stop` action) is started while the app is still in front (the only moment Android allows it).
3. `onStop` calls `Backup.windowClosed`: writes the `hidden` marker; the window's program checks the marker every 2 s (`watch_background`): when it appears it pauses in the background (`Pause::Hidden`, uploads cancelled) and releases the lock; when it goes it takes the lock back and resumes. A one-time job (id 4202, minimum latency 3 s) picks up what is left.
4. **Jobs** (`BackupJobService`, all through `Backup.run`): periodic id 4200 (every 15 minutes, any network, persisted), content-trigger id 4201 (new image or video in the media collections, update delay 10 s, max delay 120 s; one-shot, re-armed after each run), once id 4202. A job that starts while the window is up, or while backup is not enabled, does nothing. `onStopJob` asks the program to stop.
5. **Foreground service** loops `Backup.run` every 3 s while work waits, until idle, stopped, or the window returns; on Android 15 `onTimeout` stops it in order (jobs carry on).

`Backup.enabled` is true when the `session` file exists and the `settings` file contains the lines `auto=1` and `storage_confirmed=1`.

### `Backup.run` and the JSON-line protocol

`Backup.run` starts `<nativeLibraryDir>/libquench_surface.so --backup` with `HOME` = the app's `files` directory and serves it over its standard streams, one JSON object per line. Standard error goes to the log (tag `forklift-backup`).

Program to host:

```json
{"id":1,"call":"media","kind":"images","offset":0}
{"id":2,"call":"folder","uri":"content://..."}
{"id":3,"call":"upload","upload":"u1","uri":"content://...","url":"https://.../file?path=...","token":"..."}
{"id":4,"call":"device"}
{"status":{"headline":"Backing up: 3 of 10 files","done":3,"waiting":7}}
{"notice":"..."}
{"signed_out":true}
{"finished":true}
```

Host to program: `{"id":N,"ok":<value>}` or `{"id":N,"err":"<text>"}` as the answer to a call; `{"event":"done","id":"u1","status":200}`, `{"event":"failed","id":"u1","message":"..."}` for uploads; `{"event":"stop"}` to end the run (the host closes the process after 5 s if it has not ended). The answer to `upload` only says whether it started; the end is an event. In this mode `download` and `pick_folder` do nothing (restore and folder picking belong to the window).

The program ends when idle (`Controller::is_idle`: not scanning, not pumping, nothing running). Its own skip reasons (automatic backup off, no storage chosen, not signed in) still end with `{"finished":true}`. The `skipped` field, which makes the host report "busy", is printed only by `forklift_surface --backup` when `run.lock` is held (`{"finished":true,"skipped":"another run is going"}`).

Host side of `upload` (in `Backup.kt`): `PUT` with `Authorization: Bearer <token>` and `Content-Type: application/octet-stream`; events `done` (status) or `failed` (exception message).

### Notifications

| Channel id | Importance | Use |
|---|---|---|
| `backup` | low | Progress ("Progress of the backup"; job progress and the foreground service). |
| `status` | min | A quiet ongoing notification `Forklift`: `Waiting for the first look`, or `Last checked HH:MM . N waiting` / `Last checked HH:MM . everything is backed up`, read from `checked` and `manifest`. Put back when swiped away (`BackupReceiver`, action `dev.lorehaven.forklift.STATUS_DISMISSED`), after boot and after an update. Removed on sign-out or when automatic backup is off. |

Result notifications: `Backed up N new file(s)` and `Sign in again to continue backing up`.

## Build specifics

`quench android build-native` builds `forklift_surface` for `arm64-v8a` and `x86_64` and copies it as `libquench_surface.so` (the name is the same for three projects; the binary is named for the project so they can live in one workspace). `jniLibs.useLegacyPackaging = true` and `extractNativeLibs=true` because the program is executed from the native library directory. `android/app/src/main/jniLibs/` in the tree holds built copies. The signing key comes from the environment (`QUENCH_KEYSTORE`, `QUENCH_KEYSTORE_PASSWORD`, `QUENCH_KEY_ALIAS`, `QUENCH_KEY_PASSWORD`; the key password falls back to the keystore password); `.quench.toml` names `release.keystore` and alias `release`.
