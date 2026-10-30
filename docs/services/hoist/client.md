---
title: Client
parent: Hoist
grand_parent: Services
nav_order: 5
---

Hoist has no browser client. The screens are in `hoist-app` and are drawn by the native renderer (`quench-ui-native`) in a Wayland window on Linux and on a Surface view on Android. The program does the network itself (`hoist-net`), so the page security policy and the services' lack of CORS do not matter.

*Note:* `hoist-app` still has `crate-type = ["cdylib", "rlib"]`, `quench_ui_web::app!(app)` and a `sample_catalog()` demo `app()` from the web template; the real entry point is `hoist_app::live::hoist`.

## Screens

Tabs are the bottom bar (`Main`): **Catalog**, **Public** (only when `Setup.public`, i.e. on Android), **Settings**. Before sign-in there is only the sign-in form.

| Screen | Content |
|---|---|
| Sign-in (`Point Hoist at your server`) | Fields `Server URL` (rule: starts with `https://`, message `The address starts with https://`), `Username` (`Enter your user name`), `Password` (`Enter your password`, show/hide button), button `Log in`. |
| Catalog | App bar `Catalog` with buttons Updates, Refresh, Sign out. Search field (`Search apps`). Rows from `build_entries`: title, package, status text, and a button `Install`, `Update` or `Open` (by `InstallState`); the row opens the detail screen. Under the registry's rows a list `Installed apps` shows the device's other apps (Android). A button `Update all` when updates exist (this app is updated last). |
| Updates | The list of programs with an update, opened by the Updates button; `Everything is up to date` when none. |
| Detail | Facts: Version, Size (`format_bytes`, e.g. `39.8 MiB`), Format, Published by, SHA-256 (spaced), Installed, Android (`from API <min>`), Permissions list; the action button. |
| Public | Home (`Top rated for you`, category chips with `See all`), search (`Search all apps`), a category view, an app page (Version, Size, From = F-Droid / IzzyOnDroid / Aptoide, Category, Rating, Downloads; note on whether the download is checked against the catalogue's checksum), `Show more` / `Loading...`, `Try again`, `Nothing to suggest yet`, `No apps found`. |
| Settings | `Server`, switch `Check for updates in the background`, `Check now`, `Sign out`, `Version`. |
| Dialog | `Sign out?` with `Cancel` and `Sign out`. |

A full-screen loader (`role="progressbar"`, label `Working`) shows while busy; a toast shows messages for 4000 ms where the host has timers.

## Requests and messages

`hoist_app::live::Request` (app to host) and `Msg` (host to app) carry plain data. `hoist(requests, setup) -> Hoist { view, deliver }`; hand every `Msg` to `deliver` on the views' thread.

| Request | Meaning |
|---|---|
| `Login { server, username, password }` | Sign in. |
| `Catalog` | Read the registry for this platform. |
| `Download(Artifact)` | Download and verify only. |
| `Install(Artifact)` | Download, verify, hand to the system installer, record in the ledger. |
| `Logout` | Forget the session. |
| `PublicPage(PublicQuery)` | A page of the public catalogue. |
| `PublicCategories` | The public categories. |
| `DownloadPublic(PublicApp)` / `InstallPublic(PublicApp)` | As above for a public app; no token goes to the store. |
| `Launch(String)` | Start an installed app. |
| `Preferences { server, auto_check }` | The user changed a setting; the host keeps it. |

| Msg | Meaning |
|---|---|
| `LoggedIn`, `LoginFailed(String)` | Sign-in result. |
| `Catalog(Vec<Artifact>)`, `CatalogFailed(String)` | Registry result (already filtered by `offered`). |
| `Downloaded { program, path }`, `DownloadFailed(String)` | |
| `Installed { program, version_code, version_name, path }` | Handed to the installer; the ledger is updated. |
| `Known(Vec<Installed>)` | What the system says is installed, merged into the ledger. |
| `Device(Vec<DeviceApp>)` | The device's apps (Android). |
| `PublicPage { asked, page }`, `Categories(Vec<Category>)`, `PublicFailed { asked, why }` | Public catalogue results. |
| `SignedOut` | The refresh token was refused: show the sign-in. |

`Setup` fields: `server`, `has_session`, `installed`, `self_program`, `check_every_ms`, `auto_check`, `public`, `toast_ms`, `loader_min_ms`.

## Network calls (`hoist-net`)

| Method | Call |
|---|---|
| `login(base, user, password)` | Gatehouse sign-in through `gatehouse-client`; the password is not kept. |
| `Session::catalog(platform)` | `GET /warehouse/api/v1/artifacts?platform=<platform>`. `404` is `RegistryDisabled`; `401`/`403` is `Unauthorized`. |
| `Session::download(artifact, dir, progress)` | `GET /warehouse/api/v1/artifacts/{program}/{platform}/{version_code}/download?format=<format>`; the whole body is read, its SHA-256 compared with the registry's (case-insensitive), a mismatch is `HashMismatch` and nothing is written; the file name is the last path component of `filename`. `progress` is called once, at the end. |
| `Session::public_apps(query)` | `GET /hoist/v1/apps` with `sort` (default `downloads`), `offset`, `limit` (1..100) and the optional filters. `404` is `PublicDisabled`; `401`/`403` is `PublicRefused(code)` (not a sign-out). |
| `Session::public_categories()` | `GET /hoist/v1/categories`. |
| `Session::download_public(app, dir, progress)` | Plain `GET` of `download_url` with no token; refused unless `https` (or a loopback `http` address); name `<package>-<version_code>.apk` limited to letters, digits, `.`, `_`, `-`; checked against `sha256` when present, kept unverified when not. |

Refresh tokens rotate: `keeping_tokens` stores each new one as soon as it is given, `following` reads the newest stored one before a refresh.

## Platform specifics

### Linux (`hoist-desktop`)

* Install: `xdg-open <file>` for `deb`, `rpm`, `tar.gz`; for `.pkg.tar.zst/.xz/.gz` `pkexec pacman -U --noconfirm <file>` (on failure the message gives `sudo pacman -U <file>` to run by hand).
* Notifications through `notify-send` (silently nothing where it is missing).
* `hoist --check`: one background look without a window (see below).

### Android (`android/`, `hoist-surface`)

Application id `dev.lorehaven.hoist`, `minSdk 26`, `targetSdk`/`compileSdk 36`. Debug builds get the id suffix `.debug` and the label `Hoist (debug)`.

Manifest permissions: `INTERNET`, `REQUEST_INSTALL_PACKAGES`, `QUERY_ALL_PACKAGES`, `ACCESS_NETWORK_STATE`, `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED`. Components: `MainActivity` (exported, web host), `SurfaceActivity` (launcher), `InstallReceiver` (action `dev.lorehaven.hoist.INSTALL_RESULT`), `UpdateCheckService` (`BIND_JOB_SERVICE`).

Plugins (`Plugins.kt`): `EchoPlugin`, `PackagesPlugin`. The `packages` plugin methods, as used by `hoist-surface`:

| Method | Args | Result |
|---|---|---|
| `can_install` | none | `{allowed}` (`canRequestPackageInstalls`). |
| `open_install_settings` | none | Opens the system's "unknown apps" screen for this app. |
| `installed` | `{package}` | `{installed, version_name, version_code}`. |
| `list` | none | `{apps: [{package, label, version_name, version_code, system}]}`; the app hides `system: true`. |
| `launch` | `{package}` | `{launched}`. |
| `install` | `{uri}` (`file://<path>`) | Streams the APK into a `PackageInstaller` session; events `install {status: pending|success|cancelled|failure, package, message}`. |
| `install_from_url` | `{url, headers}` | Present in the plugin (downloads to `cacheDir/download.apk`, emits `download {sent,total}`); `hoist-surface` does not use it. |

*Unverified:* the full `PackageInstaller` flow (the user-action prompt) was read, not run, for this page.

## Background check

The setting **Check for updates in the background** (default on) makes Hoist look every 6 hours and notify once per set of updates (`announcement_key`); it never installs anything.

* **Android:** `UpdateCheck.schedule` registers job id `4100` (`setPeriodic` 6 h, any network, persisted; re-registered only if the existing job differs). `UpdateCheckService` runs `libquench_surface.so --check` with `HOME=<files dir>`, writing the package manager's list (`[{"package","version_code","version_name"}]`) to its stdin and reading one JSON line (above). Notification channel `updates` (`IMPORTANCE_DEFAULT`), id 1, title `Hoist: <title>`. `SurfaceActivity` asks for `POST_NOTIFICATIONS` on first start (Android 13+).
* **Linux:** `hoist --check` prints what it found (`the background check is switched off in the settings`, `not signed in: open Hoist and sign in`, `no updates`, or `<title>: <body>`), notifies with `notify-send` when news, and exits 1 on error (`could not check: ...`). Two user units are shipped in `desktop/systemd/`: `hoist-update-check.service` (`Type=oneshot`, `ConditionEnvironment=WAYLAND_DISPLAY`, `ExecStart=/usr/bin/hoist --check`) and `hoist-update-check.timer` (`OnStartupSec=5min`, `OnUnitActiveSec=6h`, `Persistent=true`). Copy to `~/.config/systemd/user/` and `systemctl --user enable --now hoist-update-check.timer`.
