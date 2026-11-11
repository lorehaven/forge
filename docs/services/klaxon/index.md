---
title: Klaxon
parent: Services
nav_order: 15
has_children: true
---

Klaxon is an alarm clock for Android. **There is no server and no network use**: no HTTP API to describe (so no `openapi.yaml`), no sign-in. Android only.

## Crates

All under `services/klaxon/`, members of the Forge workspace.

| Crate | Version | Role |
|---|---|---|
| `klaxon-core` | 0.1.0 | Logic with no I/O: `Alarm`, `Now`, `next` (which alarm rings first and how many days ahead), the text format of the list. No dependencies. |
| `klaxon-app` | 0.1.0 | The screens (`quench-ui`): the list and the editor, and the `Request`/`Msg` protocol. |
| `klaxon-surface` | 0.1.0 | The Android program (binary `klaxon_surface`, shipped as `libquench_surface.so`): the window, the `Store`, and the background commands. The app's version is this crate's. |
| `android/` | - | Gradle project `dev.lorehaven.klaxon`: `Klaxon` (system alarm), `AlarmReceiver`, `RingService`, `RingActivity`, `AlarmsPlugin`. |

## Data

The alarms are one text file, `files/.local/share/klaxon/alarms`: a line per alarm, `id|hour|minute|days|enabled|snooze|vibrate|loud|sound|sound name|label`. `days` has bit 0 for Monday to bit 6 for Sunday; 0 means the alarm rings once and then switches itself off. `sound` is a content URI from the system's picker (empty: the phone's default alarm sound), with `%` and `|` written as `%25` and `%7C`. A line that does not read is skipped. The window and the background commands (`--next`, `--fired <id>`, each answering one line of JSON) share the file; it is written through a temporary file and renamed.

## What an alarm can do

- **Repeat** on any days of the week, or **once** (no day chosen).
- **Sound**: the system's alarm sound picker (the phone's own sounds and the ones the person added); if the chosen sound is gone, the default rings instead.
- **Vibrate**: on or off, per alarm.
- **Extra loud**: raises the alarm volume to its top while it rings and puts it back when it ends.
- **Snooze** 5, 10, 15, 20 or 30 minutes; **Dismiss**.

## What happens when an alarm is due

1. `AlarmManager.setAlarmClock` fires `AlarmReceiver` (permission `USE_EXACT_ALARM`, granted at install to alarm apps).
2. The receiver starts `RingService` (a `mediaPlayback` foreground service): the alarm's sound on the alarm volume with audio focus, vibration if the alarm has it, a notification with Snooze and Dismiss and a full-screen intent for `RingActivity` (shown over the lock screen; with the phone in use it is a heads-up notification, as Android decides).
3. At once the receiver runs `--fired <id>`: a one-time alarm is switched off, and the next alarm is set.
4. Snooze sets one more system alarm after the alarm's snooze minutes (a separate request code, so the regular alarm stays). Dismiss ends the ringing. Nobody answering for 10 minutes ends it too, with a "Missed alarm" notification.
5. After a restart, an app update, or a change of time or time zone, `AlarmReceiver` sets the next alarm again; the window sets it whenever it starts.

## Not done

24-hour clock only; no gradual volume, no widget; the pending snooze is lost when the app is updated.
