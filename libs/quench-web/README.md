# Quench Web

Quench is a simple web UI framework for building HTML-based interfaces.

The built-in shell components (`HeaderBuilder`, `NavPanelBuilder`, `FooterBuilder`, and modal helpers)
share one common style system (`q-shell-*` classes) so web and terminal entrypoints can keep consistent UI semantics.

## Available API

`quench-web` (`quench_web`) is the library-only crate in this workspace.

### Primary Entry Points

- `quench_web::prelude::*` re-exports the main API surface:
  - app/shell builders: `AppBuilder`, `AppShellBuilder`, `AppShell`
  - theme enum: `Theme`
  - asset generation: `create_asset_files`, `create_asset_files_with_options`
  - HTML builders: `Element`, `Link`, `Script`, and tag helpers (`div`, `h1`, `p`, `form`, `input`, `select`, `option`, `script`, `style`, etc.)
  - CSS builder: `CssRule`

### App and Shell Builders

- `AppBuilder` builds a full HTML page string with:
  - title, links, scripts
  - theme support (`supported_themes`, `default_theme`)
  - header/content/footer element composition
  - resources prefix for static assets
- `AppShellBuilder` builds reusable shell configuration and returns `AppShell`.
- `AppShell::page(content: Element) -> String` renders page HTML for supplied body content.

### Components

- `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`
- `nav_button()`

These compose ready-made layout and navigation elements (including theme/locale selectors in nav panel).

### DOM/JS String Helpers

- `on_dom_ready(blocks: &[String]) -> String`
- `toggle_modal(overlay_class, panel_class, show_class) -> String`
- `update_from_select(select_id, update_fn) -> String`
- `set_select_value(select_id, get_fn) -> String`

These return JavaScript snippets that can be embedded into `<script>` elements.

### HTML Builder Primitives

- `Element` is a chainable node builder:
  - `.attr()`, `.class()`, `.text()`, `.child()`, `.child_opt()`
  - `.on_click()`, `.on_change()`
  - `.raw()` (disable text escaping), `.defer()`
  - `.render() -> String`
- `PageBuilder` assembles complete HTML documents.
- `Link` and `Script` model head assets.

### Theming and Styling

- `Theme` enum: `DefaultDark`, `DefaultLight`, `BootstrapDark`, `BootstrapLight`
- `Theme::theme(theme) -> String` renders theme CSS
- `theme_shared() -> String` renders shared base CSS
- `CssRule` provides programmatic CSS construction:
  - `CssRule::new(selector).property(name, value).child(rule).render()`

### Example

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
