# Daybook
Tasks, lists, labels and repeats, with reminders, on the phone, the desktop and in the browser.

## What it is
Daybook is one set of screens for the phone (Android), the desktop (Linux) and the browser; the service that holds the tasks is in `service/`. The Flutter app it replaces is `mobile-apps/daybook`.

## Use cases
- Keep an inbox and custom lists, with due dates, labels and repeats.
- Get reminders on Android (with a home-screen widget) and on Linux.
- Use the same tasks from the browser at `/daybook/ui`.

## Layout
```text
service/   daybook-service: the API (tasks, lists, labels, repeats) at /daybook/api/v1, and the browser client at /daybook/ui
core/      the logic with no I/O: tasks, due dates and their colours, the order, reminders, the task form's rules, the calendar,
           the widget's snapshot, what the app and its host say to each other
net/       the API over gatehouse-client; the saved session and settings (also builds for the browser, without the client)
host/      the controller the hosts share (changes show at once and are undone if refused; reminders) over a `Platform`
app/       the screens (a calendar and a time picker of their own)
desktop/   the Linux window; reminders while it is open
surface/   the Android program (`daybook_surface`, shipped as `libquench_surface.so`; `--widget` is the refresh with no window)
android/   the Gradle project: reminders (notifications plugin), the home-screen widget, its refresh job
web/       the browser build (wasm) over `fetch`; the service serves it from WEB_DIR
```

## Build
Build the browser module with `cargo build --target wasm32-unknown-unknown --profile wasm -p daybook-web` and put
`target/wasm32-unknown-unknown/wasm/daybook_web.wasm` in the service's `WEB_DIR` (default `web`) as `app.wasm`.
`cargo run -p daybook-desktop --example shots -- <dir>` draws the screens to PNG files with no window.

Plan and checklist: `quench/docs/migration/daybook.md`.

## Documentation
Full documentation: [services/daybook](../../docs/services/daybook/index.md)
