---
title: Architecture
parent: Daybook
grand_parent: Services
nav_order: 6
---

Daybook is split so that one set of screens and one controller serve every host, and the logic runs in fast native tests.

## Layers

```text
daybook-core                         no I/O: Task, Draft, Request/Msg, ordering, reminders, widget snapshot
   ^
daybook-net      (+ gatehouse-client when feature `client`)    Session, Store, NetError, api (paths and bodies)
   ^
daybook-host     Controller, Platform, Op/Reply, (native) Remote + Worker
   ^                         ^
daybook-app (core only)      |
   ^                         |
   +--- daybook-desktop -----+--- daybook-surface (Android) --- daybook-web (browser; no `native`, no `client`)
```

Edges taken from the `Cargo.toml` files:

| Crate | Depends on (workspace) |
|---|---|
| `daybook-core` | `quench-core`, `quench-serde`, `quench-tz` |
| `daybook-net` | `daybook-core`, `gatehouse-client` (optional, feature `client`), `quench-serde` |
| `daybook-host` | `daybook-core`, `daybook-net`, `quench-core`, `quench-serde`, `quench-tz`; feature `native` enables `daybook-net/client` |
| `daybook-app` | `daybook-core`, `quench-core`, `quench-tz`, `quench-ui` |
| `daybook-desktop` | `daybook-core`, `daybook-host[native]`, `daybook-net[client]`, `quench-core`, `quench-tz`; feature `window` (default) adds `daybook-app`, `quench-desktop-native`, `quench-style`, `quench-ui`, `quench-ui-native` |
| `daybook-surface` | `daybook-app`, `daybook-core`, `daybook-host[native]`, `daybook-net[client]`, `quench-android-surface`, `quench-core`, `quench-serde`, `quench-style`, `quench-tz`, `quench-ui`, `quench-ui-native` |
| `daybook-web` | `daybook-app`, `daybook-core`, `daybook-host` (default features off by workspace entry), `daybook-net` (same), `forge-shell`, `quench-core`, `quench-serde`, `quench-style`, `quench-tz`, `quench-ui`, `quench-ui-web` |
| `daybook-service` | `daybook-web`, `forge-web`, `quench-auth`, `quench-client`, `quench-core`, `quench-db`, `quench-http`, `quench-log`, `quench-pg`, `quench-serde`, `quench-starter`, `quench-tz`, `tokio` |

The service does not depend on `daybook-core`: it has its own domain types (`domain::task`, `project`, `label`, `recurrence`). The wire format is the contract between the two.

## The loop

1. The app (`daybook_app::daybook(requests, setup)`) shows screens and sends `daybook_core::Request` values through the `requests` closure.
2. The host passes them to `Controller::request`. The controller changes its `Data` at once where it can (completing, deleting) and calls the `Platform`: `Platform::call(Op, then)` does one call to the service on a worker thread (native) or a queue of futures (browser).
3. The answer comes back on the program's thread; the controller updates `Data` and emits `Msg` values; the app's `deliver` closure puts them into signals.
4. Changes schedule reminders 2000 ms later (`REMINDER_DELAY_MS`) and refresh the widget (`Platform::data_changed`).

All calls go one at a time, in order. A refresh token is spent when it is used, so two calls must not renew a session at once (comment in `daybook-web/src/platform.rs`; the native `Worker` is one thread with a queue).

## Hosts

| Host | Entry | Session store | Reminders |
|---|---|---|---|
| Linux | `daybook-desktop` `src/main.rs`, window "Daybook", app id `dev.lorehaven.daybook`, size 480x800 | `$XDG_DATA_HOME/daybook` or `$HOME/.local/share/daybook` | In-process timers (`Alarms`), shown with `notify-send --app-name=Daybook`; only while the window is open |
| Android | `daybook-surface` `src/main.rs` over `quench-android-surface` `Bridge` | same path; the Kotlin host sets `HOME` to the app's files directory | `notifications` plugin (alarms kept by the system) |
| Browser | `daybook-web` `app()` via `quench_ui_web::app!` | `localStorage` keys `refresh` and `language` | none (a page cannot wake when closed) |

## What lives where

* Wire parsing is by hand from `quench_serde::Value` (`Task::from_value`, `Label::from_value`, ...), not derive: unknown fields are ignored and missing optional fields get defaults.
* Rule of the form (what changing the due date does to the reminder and the repeat) is in `daybook_core::Draft`, not in the screens, so it is unit-tested.
* The server rolls repeating tasks forward in the single write path (`PUT /api/v1/tasks/{id}`), so clients need no recurrence logic.
