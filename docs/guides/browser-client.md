---
title: The browser-client pattern
parent: Guides
nav_order: 2
---

Eight services (`conveyor`, `gantry`, `gatehouse`, `gauge`, `sage`, `switchboard`, `warehouse`, `workbench`) and Daybook serve a UI that is a `quench-ui` program compiled to WebAssembly. The service keeps its JSON API, its data and its SSO routes; the browser runs the screens and calls the API with `fetch` and the session cookie. This page describes the layout from the code of those crates (Workbench is the reference).

## Crates

```text
services/<name>/
  core/      <name>-core     wire model, form drafts, logic. No I/O, builds for wasm32.
  app/       <name>-app      screens; Request/Msg enums; style.css; the shell frame
  web/       <name>-web      cdylib + rlib: fetch transport, Client::execute, quench_ui_web::app!
  service/   <name>-service  API, SSO routes, the routes that serve the client; i18n/*.ftl; web/app.wasm
  icon.png
```

Dependency edges: `service -> web -> app -> core`; `app -> forge-shell`; `service -> forge-web -> forge-shell`. `web` has `crate-type = ["cdylib", "rlib"]` (the module the page loads; the rlib is what tests and the service use, for `<name>_web::stylesheet`). Test targets use `autotests = false` and a custom harness (`harness = false`, `quench_test::main!();` at the end of the entry file).

## Request and Msg

The app does no I/O. `<name>_app::Request` is a plain enum of what the app asks of its host (Workbench: `Startup`, `Projects`, `Board(id)`, `Issue(id)`, `CreateProject`, `CreateIssue`, `Move`, `Save`, `Comment`, `AddLink`, `RemoveLink`, `Language(code)`); `Msg` is what the host answers (including `Rejected(text)`, `SignedOut`, `NotFound`). The entry point is

```rust
pub fn workbench(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Workbench // { view, deliver: Rc<dyn Fn(Msg)> }
```

with `Setup { base, language, login_url, history }`. The app owns the client-side routes (`{base}/ui/home`, `{base}/ui/projects/{id}/board`, `{base}/ui/issues/{id}`).

In the web crate, `client::Client::execute(Request) -> Vec<Msg>` turns a request into one or more HTTP calls (compound reads, writes followed by a re-read) over a `Transport` trait:

```rust
pub trait Transport {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>>;
}
```

`browser.rs` implements `Transport` as `Fetch` (URL = `{base}{path}`, `Accept: application/json`, `Content-Type: application/json` when there is a body, cookie from the sign-in), reads the language from `quench_ui_web::browser::storage_get("language")` (default `en-US`), stores a chosen language with `storage_set("language", ..)`, and ends with `quench_ui_web::app!(app);`. `base_of(path)` derives the service prefix from the page URL, so the same module works under any `BASE_PATH`.

## Hosting: `forge-web`

`forge_web::Client` (const-constructible) serves the page and the files:

```rust
static CLIENT: Client = Client::new("Workbench", workbench_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../../icon.png"));
```

| Method | Serves | Path constant |
|---|---|---|
| `page()` | HTML with an empty root, the stylesheet, the loader script and the module; headers `cache-control: no-store`, a nonce-based `content-security-policy`, `referrer-policy: same-origin`. Answers `404` with "the browser client is not installed on this server" when `app.wasm` is missing in the web directory | the app's own routes |
| `script()` | `quench.js` from `quench-ui-web` | `/ui/assets/quench.js` |
| `style()` | `forge_shell::STYLE` (when `.framed()`) then the client's stylesheet | `/ui/assets/app.css` |
| `shell()` | `forge_shell::SCRIPT` | `/ui/assets/shell.js` |
| `favicon()` | the `icon` PNG, `404` without one | `/ui/assets/favicon.png` |
| `module()` | `app.wasm` as `application/wasm`, `404` if absent | `/ui/assets/app.wasm` |

All asset responses carry `cache-control: no-cache`. The web directory is `WEB_DIR` (default `web`, relative to the service's working directory). With `.framed()` the head also gets `<script src=".../shell.js" data-gatehouse="$GATEHOUSE_URL">` and, with an icon, the `<link rel="icon">`.

The service declares the routes itself (`services/workbench/service/src/routers/ui/pages/client.rs`): the page routes behind its own session check (`ActorOrRedirect`: no session redirects to `/ui/login`) and the five public asset routes. The SSO routes (`/ui/login`, `/ui/auth/callback`, `/ui/logout`, `/ui/status`, `/ui/refresh`) stay in the service.

## Building the module

```bash
scripts/web.sh
```

For every `services/*/web` that has a `services/<name>/service` it runs `cargo build -q --target wasm32-unknown-unknown --profile wasm -p <package>` and copies `target/wasm32-unknown-unknown/wasm/<package_with_underscores>.wasm` to `services/<name>/service/web/app.wasm`. Foreman runs it before a start. `/web/*.wasm` is gitignored; `web/.gitkeep` is committed. The Docker image does the same: `images/Dockerfile.alpine` takes build arg `WASM_PACKAGE` (set per package in `.anvil/docker.toml`, for example `RESOURCE_DIR = "web", WASM_PACKAGE = "conveyor-web"`), builds with `--profile wasm` and copies the module to `${RESOURCES_PATH}/web/app.wasm`.

## The frame: `forge-shell`

`forge_shell` exports `STYLE` (tokens, base, components, shell CSS), `SCRIPT` (`shell.js`), `PALETTES` (`green teal blue purple rose orange`), `Brand { name, icon }`, `icon(name)`, `flag(locale)` (`pl-PL` gives the regional-indicator flag), and the bar:

```rust
pub struct Bar {
    pub brand: Brand, pub image: Option<String>, pub home: String, pub spa: bool, pub account: bool,
    pub brand_in_bar: bool, pub logout: String, pub on_logout: Option<Rc<dyn Fn()>>, pub dashboard: bool,
    pub languages: &'static [&'static str], pub on_language: Rc<dyn Fn(&str)>,
}
fn brand_head(bar: &Bar) -> Element; fn sidebar(bar: &Bar, nav: Vec<View>) -> Element; fn topbar(bar: &Bar, t: &Rc<I18n>) -> Element;
```

The app's `live.rs` builds a `Bar`, the sidebar with the service's navigation, and the top bar inside a `View::dynamic` so it follows the language. `shell.js` keeps the `qtheme` and `fui_palette` cookies in step with `data-fui-theme` / `data-fui-palette` on the root (checked every second), copies the `qlocale` cookie into `localStorage` key `q:language` when the app has none, writes `qlocale` and `qlocale_chosen` when the `#language` select changes, closes `details.fui-dropdown` menus on outside click or Escape, and fills gatehouse links from `data-gatehouse`.

The shell keys the translation files must contain: `locale_label`, `ui_dashboard_button`, `ui_home_button`, `ui_logout`, `ui_profile_edit`, `ui_theme_mode`, `ui_theme_palette`, `ui_theme_toggle`, `ui_user_menu`, and `ui_palette_<name>` for each palette (the code builds the key with `format!("ui_palette_{name}")`).

## Styling

The app's `style.css` (compiled in with `include_str!` and returned by `<name>_web::stylesheet()`) adds only service classes, written with the forge tokens (`var(--fui-bg)`, `--fui-surface`, `--fui-border`, `--fui-text`, `--fui-brand-500`, ...). Dark is the default; light and palettes follow the person's choice. The `#<name>` root should not set `display` or `height`, the frame owns it. Layout: `div#<name>.fui-app.fui-client > [sidebar, div.fui-client-col > [topbar, main]]`.

## Lists, forms and charts

* Lists that change use `View::keyed(items, key, render)`; the key must include everything a row shows or the list rebuilds.
* Forms use `quench_ui::form::Field` with a `Draft` type in core that returns the request body or a `DraftError`.
* The wasm client has no SVG elements: Gauge draws charts as `<img src="data:image/svg+xml,...">`.
* Drag and drop is handled by `shell.js` through data attributes (Workbench board).

## Updates: polling and server-sent events

* Polling: Conveyor (`poll(every_ms, again)` in `conveyor-app/src/live.rs`) and Gauge (`poll(every_ms, ask)` in `gauge-app/src/live.rs`) schedule `quench_ui::platform::after(ms, ..)` repeatedly and stop through `on_cleanup` when the screen is gone.
* Server-sent events: `conveyor-web` (a job's log) and `sage-web` use `quench_ui_web::EventSource` and `SseEvent::{Open, Message{name,data}, Reconnecting, Closed}`. Only GET is possible.

## Tests

| Crate | Entry | What |
|---|---|---|
| `core` | `tests/unit.rs` (modules in `tests/unit/`) | unit tests |
| `app` | `tests/screens.rs` | screens mounted in `TestRenderer` with `MemoryHistory`, driven by events, read back as text; the `Request`s asked are recorded |
| `web` | `tests/unit.rs` | `Client::execute` against a stand-in `Transport` that records requests |
| `service` | `tests/unit.rs`, `tests/integration.rs` | `unit/client_tests.rs`: page served or "not installed", redirect when signed out, files public. The API integration tests need `<NAME>_TEST_DATABASE_URL` (see [Testing](testing.md)) |

To check a service in a real browser: build the module (`scripts/web.sh` or the cargo command), run the service with `WEB_DIR` pointing at the directory holding `app.wasm`, and open `{base}/ui/home`. The stylesheet is compiled into the service binary: rebuild the service after CSS changes.

## Native apps share the screens

Daybook's `daybook-app` is also the screen set of its Android and Linux hosts and runs in the browser via `daybook-web`; its `Request`/`Msg` live in `daybook-core`. Hoist and Forklift have no browser build.
