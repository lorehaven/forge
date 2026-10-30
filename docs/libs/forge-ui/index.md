---
title: forge-ui
parent: Libraries
has_children: true
permalink: /libs/forge-ui/
---

`forge-ui` (library `forge_ui`) is the older, server-rendered counterpart of `forge-shell`: Untitled UI tokens, a responsive page frame (sidebar on desktop, drawer on mobile), the `fui-*` stylesheet and a set of component helpers that return `quench_web::prelude::Element`. Services used it to render HTML on the server with `quench-web`. **No crate in the workspace depends on it any more**: the services moved to browser clients that use `forge-shell` (the `[workspace.dependencies]` entry and one comment in `gatehouse-service` still mention it). It remains buildable, has tests and an example gallery.

## Facts

| | |
|---|---|
| Version | 0.1.0 (`publish = false`) |
| Edition | 2024 (workspace) |
| License | MIT |
| Test | `[[test]] name = "components_tests"`, `harness = false`, `tests/components_tests.rs` |
| Example | `examples/gallery.rs` (`cargo run -p forge-ui --example gallery -- <out-dir>`) |
| Unsafe | none |

## Install

Not published. Workspace entry: `forge-ui = { path = "libs/forge-ui", version = "0.1.0" }`.

## Features

None.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-core` | normal | `env::get_or` for `GATEHOUSE_URL` |
| `quench-starter` | normal | `http::routers::ui::{ui_asset_path, ui_path}` (asset and page paths under the service base) |
| `quench-web` | normal | `Element`, element builders, `AppShell`, `locale_switch`, `CssRule` |
| `quench-test` | dev | harness |

## Where to go next

* [API reference](api.md): components, shell, stylesheet functions.
* [CSS vocabulary](css.md): classes of `layout.css`, `components.css`, `legacy.css`.
* [CSS source](css-source.md).
* [Scripts](scripts.md): the inline JavaScript the shell injects and its global functions.
* [Testing](testing.md).
* Tokens: identical to those of `forge-shell`; see [tokens](../forge-shell/tokens.md).

## Minimal example

```rust
use forge_ui::*;

let cfg = ShellConfig::new("Gallery", "gallery.css", "layer-group")
    .nav(vec![NavSection::new(vec![NavItem::new("house", "nav_overview", "/ui/home")])])
    .dashboard(gatehouse_dashboard_url())
    .profile(gatehouse_profile_url());
write_css("gallery.css", &[]);               // dist/assets/css/gallery.css
let shell = UiShell::new(cfg);
let html: String = shell.page(Card::new().title_key("g_title").build());
```

(`UiShell::new` writes `dist/assets/favicon.png` relative to the working directory; the directory must exist, errors are ignored.)

*Unverified:* the snippets on this page were written from the sources and the crate's tests; they were not compiled or run here.
