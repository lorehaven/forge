# Forklift
Backs up photos, videos and chosen folders to Warehouse.

## What it is
Forklift backs up photos, videos and chosen folders to Warehouse's dynamic storage (the successor of the Flutter app `mobile-apps/mathom`). It is a client of Gatehouse and Warehouse; there is no server of its own. Migration checklist: `quench/docs/migration/forklift.md`.

## Use cases
- Back up a phone's photos and videos automatically.
- Back up chosen folders from a computer, on a timer.
- Restore files into the media library or their folders.

## Layout
```text
core/      the logic: the manifest, the scheduler, what to restore (no I/O)
net/       gatehouse (through libs/gatehouse-client), warehouse, and the Store (the manifest is a SQLite database)
app/       the screens
host/      the controller both hosts share, and the worker thread that talks to the servers
surface/   the Android program (the window and `--backup`, over JSON lines to the Kotlin job service)
desktop/   the Linux app (a Wayland window, and `forklift --backup`)
android/   the Gradle project
```

## Documentation
Full documentation: [services/forklift](../../docs/services/forklift/index.md)

## On a phone

Photos, videos and chosen folders; automatic backup by jobs and a foreground service; restore into the media library
(Pictures/Forklift, Movies/Forklift, Downloads/Forklift/<folder>). `quench android build-native` builds the program for
`arm64-v8a` and `x86_64`; Gradle in `android/` makes the APK.

## On a computer

The folders you choose (a computer has no media library). `cargo run -p forklift-desktop` opens the window (Wayland);
choosing a folder uses `zenity`, `kdialog` or `yad`, whichever is installed.

- **Without the window:** `forklift --backup` does one backup and ends. It does nothing while the window is open (the
  window has the backup then), or while no server, storage or folder is set up. To run it every 15 minutes, copy the two
  files in `desktop/systemd/` to `~/.config/systemd/user/` and
  `systemctl --user enable --now forklift-backup.timer`. A desktop notification says how many files were sent.
- **Restore:** a backed-up folder's files come back into that folder (never over a file that is there: `name (2).ext`);
  photos and videos from a phone backup, and files of a folder that is no longer set up, go under `~/Forklift Restore`.
- **Limit:** a file is read whole to be sent (the HTTP client has no streaming body), so files over 512 MiB are left out
  here; the phone streams them.
- **Packages:** `quench build deb|rpm|arch|source`; `anvil artifact release forklift` builds and publishes them with the APK.
