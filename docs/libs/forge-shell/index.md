---
title: forge-shell
parent: Libraries
has_children: true
permalink: /libs/forge-shell/
---

`forge-shell` (library `forge_shell`) is the frame that forge's browser clients share: the top bar (dashboard and home buttons, brand, language picker, theme menu, account menu), the side panel helpers, the stylesheet with the design tokens and components, and the script that keeps theme, palette and language in step with the cookies the other forge services use. It builds for the browser (the `quench-ui` views) and for the server (the CSS and JS strings a service serves; see `forge-web`).

## Facts

| | |
|---|---|
| Version | 0.1.0 (`publish = false`: not published to a registry) |
| Edition | 2024 (workspace) |
| License | MIT |
| Test harness | `harness = false`, `quench-test` (`tests/unit.rs`) |
| Unsafe | none |

## Install

Not published. Inside the workspace: `forge-shell = { workspace = true }` (`path = "libs/forge-shell"`). Used by the `app` crates of conveyor, gantry, gatehouse, gauge, sage, warehouse, switchboard, workbench, by `daybook/web`, and by `forge-web`.

## Features

None.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-ui` | normal | `Element`, `View`, `I18n`, router `link`, prelude (`div`, `a`, `button`, ...) |
| `quench-test` | dev | harness |

## Where to go next

* [API reference](api.md): `STYLE`, `SCRIPT`, `PALETTES`, `Brand`, `Bar`, `icon`, `flag`, `brand_head`, `sidebar`, `topbar`.
* [CSS vocabulary](css.md): the `fui-*` classes, grouped, with the HTML they expect.
* [Design tokens](tokens.md): every `--fui-*` custom property.
* [CSS source](css-source.md): the stylesheet verbatim.
* [JavaScript hooks](js.md): `shell.js` and every `data-*` attribute it reacts to.
* [Testing](testing.md).
* `forge-web` serves these assets; `forge-ui` is the older server-rendered counterpart.

## Minimal example

```rust
use std::rc::Rc;
use forge_shell::{Bar, Brand, topbar};
use quench_ui::i18n::I18n;

fn bar(t: &Rc<I18n>) -> quench_ui::prelude::Element {
    let bar = Bar {
        brand: Brand { name: "Gauge", icon: "eye" },
        image: None,
        home: "/gauge/ui/home".into(),
        spa: true,
        account: true,
        brand_in_bar: false,
        logout: "/gauge/ui/logout".into(),
        on_logout: None,
        dashboard: true,
        languages: &["en-US", "pl-PL"],
        on_language: Rc::new(|code| { /* remember the choice */ let _ = code; }),
    };
    topbar(&bar, t)
}
```

This is the construction used by `tests/unit.rs`. The i18n catalogue must contain the keys listed in [API](api.md#i18n-keys).
