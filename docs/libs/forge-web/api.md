---
title: API reference
parent: forge-web
grand_parent: Libraries
nav_order: 1
---

## Constants

All are paths under the service's base path (`with_base_path` adds `BASE_PATH`).

| Item | Value |
|---|---|
| `SCRIPT_PATH` | `/ui/assets/quench.js` |
| `STYLE_PATH` | `/ui/assets/app.css` |
| `MODULE_PATH` | `/ui/assets/app.wasm` |
| `SHELL_PATH` | `/ui/assets/shell.js` |
| `ICON_PATH` | `/ui/assets/favicon.png` |

## `web_dir`

```rust
#[must_use] pub fn web_dir() -> PathBuf
```
`PathBuf::from(env WEB_DIR, default "web")`, relative to the service's working directory. The wasm module is read from `<web_dir>/app.wasm`.

## `Client`

```rust
#[derive(Debug, Clone, Copy)]
pub struct Client { /* title: &'static str, stylesheet: fn() -> String, icon: &'static [u8], framed: bool */ }
```
`Copy`, so it can live in a `static`/`const`.

| Method | Description |
|---|---|
| `#[must_use] pub const fn new(title: &'static str, stylesheet: fn() -> String) -> Self` | `title` is the HTML `<title>`; `stylesheet` makes the client's own CSS (`<name>_web::stylesheet`). Not framed, no icon |
| `#[must_use] pub const fn framed(self) -> Self` | the page wears forge's frame: `forge_shell::STYLE` precedes the client's CSS and the shell script tag is added |
| `#[must_use] pub const fn icon(self, png: &'static [u8]) -> Self` | the service's PNG icon (`include_bytes!`) |
| `#[must_use] pub fn page(&self) -> Response` | the HTML page from `web_dir()` |
| `#[must_use] pub fn page_from(&self, dir: &Path) -> Response` | same, module looked for in `dir` |
| `#[must_use] pub fn script(&self) -> Response` | `quench_ui_web::QUENCH_JS`, `text/javascript; charset=utf-8` |
| `#[must_use] pub fn style(&self) -> Response` | `text/css; charset=utf-8`: `forge_shell::STYLE + "\n" + own` when framed, otherwise `own` |
| `#[must_use] pub fn shell(&self) -> Response` | `forge_shell::SCRIPT`, `text/javascript; charset=utf-8` |
| `#[must_use] pub fn favicon(&self) -> Response` | the icon as `image/png`, or `404` (`Response::not_found()`) when none was given |
| `#[must_use] pub fn module(&self) -> Response` | `<web_dir>/app.wasm` as `application/wasm`, or `404` when unreadable |
| `#[must_use] pub fn module_from(&self, dir: &Path) -> Response` | same with `dir` |

## Responses

* **Page**, module present: status `200`, `text/html` document rendered by `quench_fullstack::render_document(Page::new(title, View::Empty).stylesheet(<base>/ui/assets/app.css), nonce, <base>/ui/assets/quench.js, <base>/ui/assets/app.wasm)`, with a fresh nonce from `new_nonce()`. Headers: `cache-control: no-store`, `content-security-policy: content_security_policy(nonce)` (script-src `'self' 'nonce-...'` and `'wasm-unsafe-eval'`; `style-src 'self'` without `unsafe-inline`: asserted by the tests), `referrer-policy: same-origin`.
* Before `</head>` (first occurrence) the page gets, in this order: `<link rel="icon" type="image/png" href="<base>/ui/assets/favicon.png">` when an icon was set; when framed, `<script src="<base>/ui/assets/shell.js" data-gatehouse="<GATEHOUSE_URL, trailing / removed, attribute-escaped>"></script>`.
* **Page**, `app.wasm` missing (not a file): status `404`, body `the browser client is not installed on this server` (plain text).
* **Assets** (`script`, `style`, `shell`, `favicon`, `module`): status `200`, `content-type` as above, `cache-control: no-cache`.
