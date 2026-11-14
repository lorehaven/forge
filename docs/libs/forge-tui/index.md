---
title: forge-tui
parent: Libraries
has_children: false
permalink: /libs/forge-tui/
---

`forge-tui` (library `forge_tui`) is Untitled UI for the terminal: the design tokens of the forge web and mobile apps as RGB values, a small styled-line model, and the widgets a full-screen command-line tool needs. [forge-toolbox](../../cli/toolbox/index.md) is the first tool built on it.

## Facts

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace) |
| Unsafe | none |
| Dependencies | `quench-core` (environment access); `quench-test` (dev) |
| Tests | `tests/unit` (31), including a check of every colour against `libs/forge-shell/assets/tokens.css` |

## Modules

| Module | What it has |
|---|---|
| `color` | `Rgb` (`hex`, `over` for CSS-style alpha compositing, `to_256`), `ColorLevel` (`None`, `Ansi256`, `TrueColor`; `detect()` reads `NO_COLOR`, `TERM`, `COLORTERM`). |
| `theme` | `Theme` (the semantic tokens: `bg`, `surface`, `surface_2`, `surface_hover`, `border`, `text*`, `brand_solid`, ...), `Mode` (dark default, light), `Palette` (green, purple, blue, teal, orange, rose), `Tone`, `Soft` (the `--fui-<tone>-soft-*` triple). `Theme::from_env()` reads `FORGE_THEME`, `COLORFGBG`, `FORGE_PALETTE`. |
| `line` | `Style`, `Line` (spans of styled text: `push`, `append`, `on`, `truncate`, `pad_to`, `render(level)`), `fill_screen` (paint every row to the terminal size on the page background). |
| `console` | For tools that print lines instead of owning the screen: `Tone` (`Info`, `Success`, `Warn`, `Error`) and `print_status` / `print_box_banner` as drop-ins for the `quench_cli::prelude` ones, plus `section`, `rule`, `chip`, `muted`, `strong`, `accent`. Labels are soft badges, the banner a rounded card; plain text keeps the terminal's own colour so it reads on light and dark terminals. Unstyled when stdout is not a terminal (`ColorLevel::for_stdout`) or `NO_COLOR` is set. |
| `widgets` | `badge`, `tag`, `kbd`, `hint`, `button` (`ButtonKind`), `alert`, `alert_with_icon`, `card`. |

## Use

```rust
use forge_tui::{ColorLevel, Theme, Tone, widgets};

let theme = Theme::from_env();
let level = ColorLevel::detect();
let badge = widgets::badge(&theme, Tone::Success, "Up to date");
println!("{}", badge.render(level));
```

Widths are counted in `char`s: every glyph the widgets draw is one column wide.

## Keeping it in step with the web

The token values are copied, not generated. `tests/unit/theme_tests.rs` parses `forge-shell/assets/tokens.css` and compares the dark and light surfaces, borders, text colours, soft-tone colours and all six palettes; change the stylesheet and the test names what drifted. (The test is skipped when the stylesheet is not next to the crate, as in a published package.)

## Not done

* Used by `forge-toolbox` (full screen) and, through `console`, by `anvil`, `conveyor`, `foreman`, `pulley`, `riveter`, `warehouse` and `welder`. `loupe` is left alone: it shows documents in the terminal's own colours, which `quench-markdown-term` chooses.
* Help text and error output printed by the argument parser (`quench-args`) and the `quench-cli` REPL prompt are not restyled; they come from quench.
* The theme is not detected from the terminal (no OSC 11 query); `FORGE_THEME` or `COLORFGBG` decide.
* Light mode is covered by the token test but has not been looked at in a light terminal.
