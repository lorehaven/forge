# Klaxon
An alarm clock for Android.

## What it is
Alarms that ring on time and never need a network: a list with a switch for each, a time, a label, the days it
repeats on, and a snooze length. There is no server, no account and no desktop app. A client of nothing.

## Use cases
- Wake up on weekdays at 06:45 and on weekends not at all.
- Set a one-time alarm; it switches itself off once it has rung.
- Snooze a ringing alarm, or dismiss it, from the lock screen or the notification.

## Layout
```text
core/      the logic: alarms, when one rings next, the file the list is kept in (no I/O)
app/       the screens: the list and the editor
surface/   the Android program (the window, and `--next` / `--fired <id>` for the Kotlin side)
android/   the Gradle project: the system alarm, the receiver, the ringing service and its screen
```

## Documentation
Full documentation: [services/klaxon](../../docs/services/klaxon/index.md)

## How it rings
The Rust program works out *which* alarm rings next and *how many days ahead* (the wall clock's time is kept: a change
of clocks moves nothing). Kotlin turns that into a system alarm (`AlarmManager.setAlarmClock`, which Doze lets through
and the status bar shows), and when it fires starts a foreground service that plays the alarm sound and shows
`RingActivity` over the lock screen. After a restart, an update, or a change of time or zone the next alarm is set again.

## Building
`quench android build-native` builds the program for `arm64-v8a` and `x86_64`; `quench build apk` makes the APK;
`anvil artifact release klaxon` builds a signed one and publishes it (it needs `services/klaxon/release.keystore`, made
with `quench android keystore`).
