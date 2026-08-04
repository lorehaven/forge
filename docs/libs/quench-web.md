# Quench Web

`quench-web` (crate `quench_web`) is a library-only, dependency-light HTML/CSS/JS builder used to assemble server-rendered UI pages without a templating engine. It provides chainable `Element`/`PageBuilder` builders, a small set of framework-level page/shell/theme/i18n helpers, and a shared `q-shell-*` class vocabulary so every service's UI looks and behaves consistently. It is depended on directly by `quench-starter` (for its own UI plumbing), `quench-web-components`, and the `switchboard-service`, `conveyor-service`, `sage-service`, `warehouse-service`, `gatehouse-service` Docker services, plus `examples/basic`.

## Public API / Key Types

Reachable via `quench_web::prelude::*`:

- App/shell builders — `AppBuilder` (title, links, scripts, themes, locales, header/content/footer, `resources_prefix`) and `AppShellBuilder` → `AppShell`, which wraps `AppBuilder` with sensible defaults (nav panel, header/footer, asset generation). `AppShell::page(content: Element) -> String` renders a full page.
- `Theme` enum — `DefaultDark`, `DefaultLight`, `BootstrapDark`, `BootstrapLight`. `Theme::theme(theme) -> String` renders that theme's CSS; `theme_shared()` renders the shared base CSS.
- Asset generation — `create_asset_files(default_theme, resources_prefix)` and `create_asset_files_with_options(default_theme, supported_themes, supported_locales, resources_prefix)` write `dist/assets/{css/style.css, css/themes/<theme>.css, favicon.png, js/translations.js}` to disk.
- HTML builders — `Element` (`.attr()`, `.class()`, `.text()`, `.child()`, `.child_opt()`, `.on_click()`, `.on_change()`, `.raw()`, `.defer()`, `.render() -> String`) plus tag helpers (`div`, `h1`, `h2`, `h3`, `p`, `a`, `button`, `header`, `footer`, `nav`, `form`, `input`, `select`, `option`, `label`, `script`, `style`, `span`, `i`, `strong`, `pre`, and more in `html::builder::elements`). `PageBuilder` assembles a full `<html>` document (pretty-printed via `html5ever`). `Link` and `Script` model head assets — `Script::inline(...)`, the `js!` macro for formatted inline scripts, `Script::new(src).defer()/.immediate()/.crossorigin(...)`.
- Components (`framework::components`) — `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder` (theme + locale switcher, built as a slide-out modal panel), `nav_button()`, `locale_switch()`.
- DOM/JS string helpers (`framework::dom`) — `on_dom_ready(blocks)`, `toggle_modal(overlay_class, panel_class, show_class)`, `update_from_select(select_id, update_fn)`, `set_select_value(select_id, get_fn)`; these return JS snippets meant to be embedded via `script(...).raw()`.
- CSS builder (`styling::css`) — `CssRule::new(selector).property(name, value).child(rule).render()`.
- i18n (`framework::app::i18n`) — `available_locales()` (scans an `i18n/*.ftl` directory next to the running binary), `validate_locales_exist(locales)`, `generate_translations_js(locales)` (parses Fluent `.ftl` files into a `window.qTranslations` JS object consumed client-side by the locale script).

## Configuration

Not environment-variable driven; behavior is controlled through builder calls:

- `resources_prefix` on `AppBuilder`/`AppShellBuilder` — prefix applied to generated asset URLs (CSS, JS, favicon), so a service mounted under a base path still resolves its assets.
- `i18n/*.ftl` directory (relative to the process's working directory) — presence and contents drive `available_locales()` / translation generation; absent directory degrades gracefully to no locales.

## Testing

No test files exist under `libs/quench-web` (`tests/` is absent); coverage for this crate's behavior currently comes from the services that consume it.

## Usage example

```rust
use quench_web::prelude::*;

fn page() -> String {
    create_asset_files(Theme::DefaultDark, "");

    let shell = AppShellBuilder::new()
        .title("Demo")
        .default_theme(Theme::DefaultDark)
        .build();

    shell.page(
        div()
            .class("container")
            .child(h1().text("Hello from Quench"))
            .child(p().text("Rendered from Element builders")),
    )
}
```

[Home](../README.md)
