---
title: Usage
parent: forge-web
grand_parent: Libraries
nav_order: 2
---

## Routes a service declares

Paths are relative to the service's `BASE_PATH`. The page route keeps the service's own session check; the asset routes are anonymous (the files hold no data). Typical wiring (see `services/gatehouse/service/src/ui/pages/client.rs` and `services/warehouse/service/src/routers/ui/pages/client.rs`):

| Route | Handler |
|---|---|
| `GET /ui/home` (and other client routes) | `CLIENT.page()` behind the session check |
| `GET /ui/assets/quench.js` | `CLIENT.script()` |
| `GET /ui/assets/app.css` | `CLIENT.style()` |
| `GET /ui/assets/app.wasm` | `CLIENT.module()` |
| `GET /ui/assets/shell.js` | `CLIENT.shell()` (framed clients) |
| `GET /ui/assets/favicon.png` | `CLIENT.favicon()` |

## Building the module

The module is built apart and put in `WEB_DIR` (`web` by default) as `app.wasm`:

```bash
cargo build --target wasm32-unknown-unknown --profile wasm -p <name>-web
```

The image build does this for a service that asks for it (build argument/env `WASM_PACKAGE`). Without the file the page route answers `404` with the "not installed" text.

## Environment

| Variable | Default | Used by |
|---|---|---|
| `WEB_DIR` | `web` | `web_dir()`: where `app.wasm` is read from |
| `GATEHOUSE_URL` | empty | framed pages: written into `data-gatehouse` of the shell script tag, so `[data-gh-href]` links point at gatehouse |

## A framed client with an icon

```rust
static CLIENT: Client = Client::new("Gatehouse", gatehouse_web::stylesheet)
    .framed()
    .icon(include_bytes!("../../../../icon.png"));
```

Pitfalls: `Client::new` is `const`, so the stylesheet must be a plain `fn() -> String`; the page is `no-store` while assets are `no-cache` (revalidated each time, no versioned URLs).

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
