---
title: Client
parent: Forklift
grand_parent: Services
nav_order: 4
---

The screens are `forklift-app`, drawn by the native renderer (no WebView) in a Wayland window (Linux, 480 x 800 initial size, title `Forklift`, app id `dev.lorehaven.forklift`) and on the Android surface. The app does no I/O: `forklift_app::live::forklift(requests, setup) -> Forklift { view, deliver }`.

## Screens

Bottom bar `Main`: **Backup** and **Settings**. Before sign-in only the sign-in form; before a storage is chosen only the chooser.

| Screen | Content |
|---|---|
| Sign-in (`Point Forklift at your server`) | `Server URL` (rule `The address starts with https://`), `Username` (`Enter your user name`), `Password` (`Enter your password`, show button), `Log in`. |
| Storage chooser (`Where should this device's backup go?`) | Shown right after sign-in when `storage_confirmed` is false, and from Settings > Storage > `Change`. States: `Asking the server...`; an error with `Try again`; `This account has no storage it can use: ask whoever runs the server to add one, then try again.`; the list of the account's dynamic storages with use and quota; when changing: `Choosing another one sends everything to it again; what is already stored stays where it is.`; `Cancel`. |
| Backup (dashboard) | Card: headline (see [Data](data.md#snapshot-file)), a progress bar (`fraction()` = done / (done + waiting), 1 when settled and empty, 0 when unknown), three fixed lines: `Sending <name>` / `Sending <name> and N more` or `Looking at your files . N`; `Last checked HH:MM . <size> to go`; `Storage <used> of <quota>`. Button `Back up now` (shows `Working...` while busy). Then a row per category: label and `N backed up . N waiting[ . N failed]`. |
| Settings | `Server` (read only), `Storage` + `Change`, switch `Back up automatically` ("Looks for new files as they appear and every so often, even when Forklift is closed"), on a phone `Photos`, `Videos`, `Videos on Wi-Fi only`, `Large files only while charging` ("Files over 200 MB wait for the charger"); `Folders` with `Add` (each folder: switch, `Wi-Fi only` / `Any network`, `Remove folder`; `No folders yet`), `Restore from backup` ("Bring back what this device lacks"), `Sign out` ("Forget this session on this device"), `Version` (`v<crate version>`). |
| Restore dialog (`Restore from backup`) | Text from `restore_words`: `Looking at what is stored...`; `Restore N files (size)? M already on this device.` (+ ` K are left out: their folder is not part of the backup any more.`); `Nothing to restore: this device has the N stored files that have a place here.`; `Restoring: D of T files[, F could not be restored]`; `Restored D of T files.`; buttons `Cancel`, `Restore`, `Stop`, `OK`. |
| Sign-out dialog (`Sign out?`) | "Backing up stops until you sign in again on this device. What is already stored stays." `Cancel` / `Sign out`. |

A loader (`aria-label="Working"`) covers the screen while a sign-in or similar is running; messages show as a toast (4000 ms on both hosts that use timers).

## Requests and messages

| `Request` | Meaning | Host action |
|---|---|---|
| `Login { server, username, password }` | Sign in. | Gatehouse login; store token; adopt the server. |
| `ListStorages` | Ask which storages the account may use. | `Controller::list_storages` |
| `ChooseStorage(String)` | | `Controller::choose_storage` |
| `BackUpNow` | The person asked. | `Controller::scan(true)` |
| `Settings(Settings)` | A setting changed. | `Controller::update_settings` (keeps the storage fields as they are) |
| `AddFolder` | | `Controller::add_folder` (picker; rejects a folder already present with `That folder is already backed up`) |
| `PlanRestore`, `StartRestore`, `CancelRestore` | | `plan_restore`, `start_restore`, `cancel_restore` |
| `Logout` | | `sign_out`: stop, save, delete `session` and `snapshot`. |

| `Msg` | Meaning |
|---|---|
| `LoggedIn`, `LoginFailed(String)` | |
| `Snapshot(Box<Snapshot>)` | Numbers for the dashboard (at most every 250 ms). |
| `Settings(Box<Settings>)` | Settings changed on the host side (folder added, storage chosen). |
| `Storages(Box<Result<Vec<StorageInfo>, String>>)` | |
| `Restore(Box<RestoreView>)` | `Idle`, `Planning`, `Plan(PlanSummary)`, `Running(Progress)`, `Done(Progress)`, `Failed(String)`. |
| `Notice(String)` | A toast. |
| `SignedOut` | Show the sign-in. |

`Setup`: `settings`, `has_session`, `phone`, `offset_minutes` (for `Last checked`), `toast_ms`, `initial` (the last snapshot, so numbers show at once), `loader_min_ms`.

## Platform plugins used (Android)

The surface program calls the Kotlin plugins through the surface bridge (`bridge.call(plugin, method, args, cb)`): `files` (`media`, `list_folder`, `pick_folder`, `upload`, `cancel_upload`), `device` (`state`), `restore` (`save`, `cancel`). Details in [Android](android.md). The Linux host has no plugins: it implements `Platform` directly (`Linux`).

## Linux host

* Folders only; `media` returns an empty list.
* `device()` always reports online, on Wi-Fi, charging, so no condition ever holds a file back.
* A folder is walked with `std::fs::read_dir`; symlinks, names `.ds_store`, `thumbs.db`, `desktop.ini` (case-insensitive) and names ending `.part` are skipped; directories are queued, entries sorted by path.
* Upload: the file is read whole into memory and sent with one `PUT` (the HTTP client has no streaming body). A file over `MAX_FILE_BYTES` (512 MiB) is answered `413` without reading it, so it becomes `too_large`.
* Folder chooser: first of `zenity --file-selection --directory`, `kdialog --getexistingdirectory <HOME>`, `yad --file --directory`, each with title `Choose a folder to back up`; if none is installed the notice is `To choose a folder, install zenity or kdialog`.
* Restore destination (`restore_path`): `video` under `~/Forklift Restore/videos`; `folder` into the original folder (matched by display name; if it is no longer configured, `~/Forklift Restore/<safe name>`); photos under `~/Forklift Restore/photos`. File names lose `/`, `\`, NUL and leading dots (empty becomes `file`). An existing file is never overwritten: `name (2).ext`, `name (3).ext`, ... Written via `<name>.part` renamed when whole; a download over 512 MiB fails.
* Notifications: `notify-send --app-name=Forklift`.

### `forklift --backup`

One backup without a window. If another program holds `run.lock`: prints `the backup is running already (the window has it)` and exits 0. Else `run_background`: skips (prints `nothing done: <why>`) when `auto` is off (`automatic backup is off`), no storage is confirmed (`no storage chosen yet`) or there is no session (`not signed in`). Otherwise scans (without the 10-minute rescan), sends until idle, prints `N file(s) sent`, and when N > 0 notifies `Backed up N new file(s)`. Exit 1 with `the backup could not run: <why>` on an I/O error. The window and this mode never run together: the window holds the lock.

systemd units in `desktop/systemd/`: `forklift-backup.service` (`Type=oneshot`, `ExecStart=/usr/bin/forklift --backup`) and `forklift-backup.timer` (`OnStartupSec=3min`, `OnUnitActiveSec=15min`, `Persistent=true`, `WantedBy=timers.target`). Install: copy to `~/.config/systemd/user/` and `systemctl --user enable --now forklift-backup.timer`.
