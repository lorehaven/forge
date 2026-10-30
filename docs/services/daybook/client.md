---
title: Client
parent: Daybook
grand_parent: Services
nav_order: 5
---

The Daybook client: one set of screens (`daybook-app`) on three hosts. This page covers the screens, the protocol between the app and its host, the platform plugins, the widget and the browser specifics.

## Screens

All text is English and hard-coded in `daybook-app` (no translations; the browser's top bar is translated, see below). Texts quoted from the source.

**Sign-in** (`login_form`). Fields "Username" and "Password" (both required: "Enter your user name", "Enter your password"); button "Sign in". A program also shows a server address field (`Setup::ask_server = true`; the browser is always on its own server and hides it). After `Msg::SignedOut` the form shows "Sign in again".

**Task list** (`tasks_screen`). Title is "Inbox", the list's name or the label's name (`title_of`). App bar buttons: lists drawer ("Lists"), "Refresh", and for a list or label "Rename" and "Delete". Body: pull-to-refresh over the open tasks of the view (`daybook_core::view`: open tasks that `ListRef::contains`, ordered), empty text "Nothing here. Add a task with the + button.", a floating button "Add task" ("+", element id `add-task`), a progress indicator with `aria-label` "Working" while loading. Each row: a check control ("Complete"/"Reopen"), title, due text and colour, label chips, a repeat marker; swiping a row offers "Delete" which asks for confirmation.

**Drawer / navigation** (`nav_view`): "Inbox"; heading "Lists" with each list and "New list"; heading "Labels" with each label (and a way to add one); "Sign out" (confirmed).

**Task form** (`form::task_dialog`). Fields: "Task" (title, required: "Enter a title"), "Notes", "Due" (opens date picker, then "Due time" with "All day" or a time picker "Choose a time…"), "Repeat" (menu: "Does not repeat", "Every day/week/month/year", "Custom…"), "Reminder" (menu "Remind me": "No reminder", presets, "Pick date and time…"), "List" (menu, default "Inbox"), labels with "New label". Buttons "Save"/"Saving…", "Cancel", "Delete" (existing task), and for a repeating task "Mark occurrence done" and "Stop repeating". The custom repeat sheet "Repeat": "Every" N unit, weekday chips Mon..Sun (weekly), option "Count from when it is done", "Back"/"OK".

**Date and time pickers** (`pickers.rs`): a month grid, Monday first (`daybook_core::calendar`/`picker::month_grid`), and a time picker; drawn by the app itself, not the platform.

**Dialogs**: confirm (`Confirm::DeleteTask|DeleteProject|DeleteLabel|SignOut`, buttons "Cancel"/"Yes"), naming (`Naming::NewProject|Project|Label`: field "Name", "Enter a name", "Cancel"/"Save"), label colour chosen from `LABEL_COLORS` (`#ef4444 #f97316 #eab308 #22c55e #14b8a6 #3b82f6 #8b5cf6 #ec4899 #888888`; the service accepts any `#rrggbb`).

**Toast / snackbar** (`toast.rs`): `Msg::Notice` text for `Setup::toast_ms` = 4000 ms.

### View rules (`daybook_core::order`)

Open tasks sort by due date (an all-day task before the timed ones of its day, tasks without a date last), then `sort_order`, then `created_at`. Completed tasks follow, by `updated_at` descending. The client loads open tasks only: `GET /tasks?all=true&completed=false`.

### Reminder presets (`daybook_core::reminders::Preset`)

Timed due dates: "At due time", "10 minutes before", "1 hour before", "1 day before". All-day: "On the day, 09:00", "The day before, 09:00", "2 days before, 09:00" (`DAY_REMINDER_HOUR` = 9, in the viewer's zone). A preset follows the due date; changing the due kind drops a preset of the other kind. Clearing the due date clears the repeat and any preset. A repeat can only be set with a due date.

Scheduling: `pending_reminders` returns open tasks whose `remind_at` is after now, soonest first, at most `MAX_SCHEDULED` = 200. The notification id is the 32-bit FNV-1a hash of the task id masked to 31 bits (`notification_id`). Body: `Due <day>` / `Due <day> <hh:mm>`; title is the task content.

## App <-> host protocol (`daybook_core::protocol`)

The app sends `Request`; the host answers with `Msg`.

| `Request` | Meaning | Controller behaviour |
|---|---|---|
| `Login{server, username, password}` | sign in | `Op::Login`; trims the server and trailing `/`; stores `server` and `user` in settings; emits `LoggedIn` then loads, or `LoginFailed(text)` |
| `Reload` | read everything | `Op::Load` (projects, labels, open tasks) |
| `Open(ListRef)` | the view opened | remembered in `settings` (`list`) |
| `Save{id, draft}` | create or replace | body from `Draft::body`; `Op::CreateTask` or `Op::UpdateTask`; emits `Saved(Ok)` or `Saved(Err)`; `NotFound` triggers a reload |
| `SetCompleted{id, done}` | check off / reopen | optimistic for ordinary tasks; a repeating task waits for the service and says "Done — next: ..." |
| `Delete{id}` | delete a task | removed at once, restored if refused (a 404 counts as deleted) |
| `CreateProject{name}` | new list | `sort_order` = current number of lists |
| `RenameProject{id,name}` | rename | keeps the list's `sort_order` |
| `DeleteProject{id}` | delete list and its tasks | local removal after the service agrees (or 404); resets the open view to Inbox |
| `CreateLabel{name,color}`, `UpdateLabel{..}`, `DeleteLabel{id}` | labels | the new label is announced with `LabelCreated` |
| `AskNotifications` | ask for permission | `Platform::ask_notifications` then `NotificationsBlocked(!allowed)` |
| `Logout` | sign out | clears the token, data, reminders, widget; `SignedOut` |

| `Msg` | Meaning |
|---|---|
| `LoggedIn`, `LoginFailed(String)`, `SignedOut` | session state |
| `Data(Box<Data>)` | everything (projects, labels, tasks), after a load or any change |
| `Saved(Result<(), String>)` | the form may close or stay with the reason |
| `LabelCreated(Box<Label>)` | so a form can tick it |
| `Notice(String)` | a message |
| `Loading(bool)` | reading from the service is going on |
| `NotificationsBlocked(bool)` | reminders cannot be shown until allowed |
| `Show{list, add, task}` | something outside asked for a view (widget +, tapped reminder) |

A failed call with `NetError::Unauthorized` signs out; any other failure becomes `Notice("<what>: <error>")`. `NetError` texts are in `daybook-net/src/lib.rs`; HTTP status mapping (`check_status`): 400 Invalid, 401 Unauthorized, 403 Forbidden, 404 NotFound, 409 NameTaken, other Status(n). Gatehouse refusals (`sign_in_error`) map `mfa_required`, `account_locked`, `account_disabled`, `email_unverified` in the body, else Unauthorized. A second factor is not supported.

`Op` (host to platform): `Login`, `Load`, `CreateTask(Value)`, `UpdateTask(id, Value)`, `DeleteTask`, `CreateProject{name, sort_order}`, `RenameProject{id, name, sort_order}`, `DeleteProject`, `CreateLabel`, `UpdateLabel`, `DeleteLabel`, `Logout`. `Reply`: `Done`, `Loaded(Data)`, `Task`, `Project`, `Label`.

### Platform trait (`daybook_host::Platform`)

`call(op, then)`, `replace_reminders(&[Reminder])`, `ask_notifications(then)` (default: allowed), `data_changed(&Data, now, &Zone)` (default no-op; the Android host writes the widget), `signed_out()`, `now()`, `zone()`, `later(ms, f)`.

### Paths the clients call

`daybook_net::api`: `API = "/daybook/api/v1"` (fixed; a deployment on another base path would not work with the programs), `LOGIN = "/gatehouse/api/v1/auth/login"`, `REFRESH = "/gatehouse/api/v1/auth/refresh"` (programs go to gatehouse directly through `gatehouse-client`). The browser build uses `/daybook/auth/login` and `/daybook/auth/refresh` (the service's forwarders) and the same API path.

## Linux (`daybook-desktop`)

* Binary `daybook`, `.quench.toml` `[linux]` id `dev.lorehaven.daybook`, `bin = "daybook"`, `depends = ""`.
* Default server offered at sign-in: `https://ennor.ddns.net` (`DEFAULT_SERVER`).
* Reminders: `Alarms` keeps the pending list and one timer; it wakes after at most 60 000 ms (`LOOK_AGAIN_MS`) to re-read the clock; due ones are shown with `notify-send --app-name=Daybook <title> <body>`. A closed window shows none.
* `cargo run -p daybook-desktop --example shots -- <dir>` draws the screens offscreen to PNG files; example `bench`.

## Android (`daybook-surface` + `android/`)

The Rust program is started by the Kotlin host (`SurfaceHost`) with `HOME` set to the app's files directory, draws with the native renderer (no WebView) and talks to the app's plugins through the surface `Bridge`: stdin carries touches and keys, stdout frames (so log to stderr).

* Manifest permissions: `INTERNET`, `ACCESS_NETWORK_STATE`, `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED`, `USE_EXACT_ALARM`, `SCHEDULE_EXACT_ALARM` (max SDK 32). `allowBackup=false`. Release builds forbid cleartext traffic (`network_security_config.xml`); a debug variant has its own config.
* `minSdk` 26, `compileSdk`/`targetSdk` 36, application id `dev.lorehaven.daybook`.
* Plugins (`Plugins.kt`): `EchoPlugin`, `NotificationsPlugin` (`channel`, `show`, `schedule`, `cancel`, `pending`, `permission`), `DaybookPlugin` (`refresh_widgets`).
* Reminders: `Android.replace_reminders` sends `notifications.schedule {id, at_ms, title, body, channel: "reminders"}`; after the first call only the difference is sent (`cancel` for removed ones, `schedule` for new or changed). The first call reads `notifications.pending` and cancels reminders that are armed but not wanted. Alarms survive reboot through `BootReceiver` (BOOT_COMPLETED, TIMEZONE_CHANGED, TIME_SET, MY_PACKAGE_REPLACED).
* Permission: `notifications.permission` -> `{granted: bool}`; the controller asks the first time it has reminders to schedule.

### Home-screen widget

* Provider `DaybookWidgetProvider` (4x2 cells default, resizable, `updatePeriodMillis` 1 800 000), rows from `DaybookWidgetService`, list chooser `ListPickerActivity` opened from the widget title.
* Content comes from `widget.json` (see [Data](data.md)) in `<filesDir>/.local/share/daybook`; the program rewrites it on every data change (`Platform::data_changed`) and calls plugin `daybook.refresh_widgets`. The widget computes "Today 14:30" and the colour when it draws (`DueColors.kt`).
* Background refresh: `WidgetRefresh` schedules job id 2701 every 30 minutes, any network, persisted; it runs `libquench_surface.so --widget` (no window): load with the saved session, write the snapshot, print one line (`Refreshed(n)`, `SignedOut`, `Unreachable(reason)`). `Unauthorized` signs out and empties the widget.
* Taps: the widget "+" opens `daybook://add?list=<id>`, the logo and rows open `daybook://open?list=<id>&task=<id>`. `SurfaceActivity.handleLink` writes a `launch` file (`<action>\n<list>\n<task>\n`, via `launch.tmp` then rename); the program polls every 400 ms (`LAUNCH_POLL_MS`), reads and deletes it, and delivers `Msg::Show`. Unknown actions are ignored; an unparsable list id means the inbox.
* One session is shared by the window and the refresh job: the refresh token rotates, so each looks at what the other saved (`Session::following`).

## Browser (`daybook-web`)

* `app()` builds the screens over `Web` (platform) and `Client<Fetch, Stored>`. The server is the page's own (`ask_server = false`, `server` empty). Refresh token in `localStorage` key `refresh`; language in key `language`.
* Calls are queued and run one at a time (`pump`). An access token is kept in memory; a 401 on the first attempt renews through `/daybook/auth/refresh` and retries once.
* No scheduled reminders (`replace_reminders` is a no-op); the clock is `Date.now`-based, the zone is the browser's (`env::time_zone`, UTC if unknown).
* Hosted by `forge_web::Client::new("Daybook", daybook_web::stylesheet).framed().icon(...)`: the page, scripts, CSS and the module under `{base}/ui/...` (see [API](api.md)); `framed()` adds the forge top bar script (`shell.js`) with `data-gatehouse=<GATEHOUSE_URL>`.
* Top bar (`forge_shell::Bar`): brand "Daybook", icon `list-check`, dashboard and home buttons, language menu, account menu (edit profile at gatehouse; "Log out" sends `Request::Logout`).
* i18n: locales `en-US` (fallback), `de-DE`, `es-ES`, `fr-FR`, `pl-PL`, files `services/daybook/web/i18n/*.ftl`, compiled in. They hold only the top bar's keys (`locale_label`, `ui_dashboard_button`, `ui_home_button`, `ui_theme_toggle`, `ui_theme_mode`, `ui_theme_light`, `ui_theme_dark`, `ui_theme_palette`, `ui_palette_green|teal|blue|purple|rose|orange`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`). The app's own texts are English only.
* Styling: `daybook_app::STYLE` (`app/src/style.css`, classes prefixed `dbk-`) on top of `quench-style` tokens/mobile/components, then `web/frame.css`. The theme follows `html[data-fui-theme]`, dark by default.
