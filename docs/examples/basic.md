# Example: Basic

`quench-example-basic` (`examples/basic`) is the smallest possible Quench web application — a minimal `axum` server that builds its pages with `quench-web`'s `AppShellBuilder`. It exists in the workspace as a smoke test for the Quench UI framework: a quick way to confirm the shell, theming, and static-asset pipeline still work end to end after a change to `quench-web`, and a copy-pasteable starting point for a new service's UI.

## What it covers

- `AppShellBuilder::new().default_theme(...).supported_themes(...).build()` — builds an `AppShell` with a default theme of `Theme::BootstrapDark` and both `BootstrapDark`/`BootstrapLight` enabled. Because `with_header`/`with_nav` default to `true`, the resulting shell renders a header, a nav panel (including its slide-out modal panel), the page content, and a footer.
- Three pages built with the `quench-web` element builders (`content()`, `div()`, `h1()`, `h2()`, `p()`, ...): `/` (a `Hello.` heading), `/about`, and `/contact`.
- A `GET /api/data` route returning a hardcoded JSON string, showing that ordinary `axum` routes sit alongside shell-rendered pages in the same `Router`.
- `/assets/*` served from `dist/assets` via `tower_http::services::ServeDir` — the CSS/JS the shell needs (`style.css`, theme stylesheets under `assets/css/themes/`, `locale.js`, `theme.js`) is written there by `AppShellBuilder::build()` itself (`create_asset_files_with_options`), not checked in by hand.
- Two Fluent locale files under `i18n/` (`en-US.ftl`, `pl-PL.ftl`) supplying `header_label`, `footer_label`, `locale_label`, and `theme_label` — `AppShellBuilder` picks these up automatically since no explicit `supported_locales` was passed.

## How to run it

```bash
cargo run -p quench-example-basic
```

This starts the server on `http://127.0.0.1:3000` (bound via `SocketAddr::from(([127, 0, 0, 1], 3000))` in `src/main.rs`). Visit `/`, `/about`, or `/contact` in a browser, or `curl http://127.0.0.1:3000/api/data`.

The root workspace README also lists this as one of its "Quench UI Smoke Tests" (`cargo run -p quench-example-basic`).

## Requirements

- Just the Rust toolchain — no database, no external services. Dependencies are `axum`, `quench-web`, `tokio`, and `tower-http` (all workspace-managed).

[Home](../README.md)
