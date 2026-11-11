---
title: Android
parent: Klaxon
grand_parent: Services
nav_order: 1
---

Application id `dev.lorehaven.klaxon`, `minSdk 26`, `targetSdk` and `compileSdk` 36. Kotlin in `android/app/src/main/kotlin/dev/lorehaven/klaxon/`.

## Manifest

Permissions: `POST_NOTIFICATIONS`, `USE_EXACT_ALARM`, `SCHEDULE_EXACT_ALARM` (`maxSdkVersion 32`), `USE_FULL_SCREEN_INTENT`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, `RECEIVE_BOOT_COMPLETED`, `WAKE_LOCK`, `VIBRATE`. No `INTERNET`. Components: `SurfaceActivity` (launcher), `RingActivity` (own task, over the lock screen), `RingService` (`mediaPlayback`), `AlarmReceiver` (`BOOT_COMPLETED`, `MY_PACKAGE_REPLACED`, `TIME_SET`, `TIMEZONE_CHANGED`, and the alarm itself). `allowBackup=false`.

## Plugin `alarms`

| Method | Does |
|---|---|
| `schedule` | Sets the system alarm from the program's `{id, days_ahead, hour, minute, snooze, label}`, or removes it for `{none: true}`. |
| `status` | `{alerts: bool}`: notifications allowed, and (Android 14+) full-screen intents allowed. |
| `pick_sound` | Opens the system's alarm sound picker on `{current}` (a URI, empty for the default); answers `{uri, name}` (empty for the default) or `{cancelled: true}`. |
| `allow` | Asks for the notification permission, then opens the system's full-screen-intent page if that is what is missing. |
