---
title: Commands
parent: toolbox
grand_parent: Command-line tools
nav_order: 1
---

The interface (not run for the book; documented from `main.rs` and `lib.rs`, whose behaviour is covered by `tests/unit`).

## The screen

The screen follows the forge web and mobile apps (Untitled UI, through [forge-tui](../../libs/forge-tui/index.md)): a page background, a header with a version tag, the self-update note as an alert, the tools in a card with status badges and action buttons, the keys as chips, and the last message as an alert.

```text
  Forge Toolbox   v0.1.22
  Monitor and install the forge command-line tools.
  Registry ennor  sparse+https://ennor.ddns.net/warehouse/index/

  ▌ ! forge-toolbox update available: 0.1.22 -> 0.1.23. run forge-toolbox self-update

  ╭────────────────────────────────────────────────────────────────────────╮
  │ Tools  4 packages   ● 1 update                                         │
  ├────────────────────────────────────────────────────────────────────────┤
  │  Package  Binary   Installed  Latest   Status               Action     │
  │  anvil    anvil    0.4.38     0.4.38   ● Up to date         —          │
  │▌ riveter  riveter  0.3.1      0.3.4    ● Update available   Update     │
  │  welder   welder   —          1.0.0    ● Not installed      Install    │
  │  pulley   pulley   —          —        ● Registry error     —          │
  │  ✕ registry lookup failed                                              │
  ╰────────────────────────────────────────────────────────────────────────╯

   ↑↓  Navigate    ↵  Install / update    r  Refresh    q  Quit

  ▌ ✓ riveter updated
```

*Verified:* the layout is the output of `render_with`, and a run in a pseudo-terminal against the registry showed the same screen; this sample is redrawn narrower and its versions are illustrative.

Columns:

| Column | Content |
|---|---|
| (marker) | `▌` in the brand colour on the selected row, which also gets the hover background. |
| `Package` | Cargo package name. |
| `Binary` | Binary the package installs. |
| `Installed` | Version from `cargo install --list`, or `—`. |
| `Latest` | Newest version from `cargo search <package> --limit 10 --registry ennor` (the first result line whose name matches exactly), or `—`. |
| `Status` | A badge: `Up to date` (success), `Update available` (warning), `Not installed` (gray, installable), `Unavailable` (gray, not installed and not in the registry), `Not in registry` (gray, installed but the registry has no such crate), `Registry error` (error). |
| `Action` | A button: `Install` (not installed and installable), `Update` (updatable), else `—`. |

A package whose registry lookup failed has its row followed by `✕ Failed to fetch latest version for <package>: <cause>`. The card header counts the packages and, when some can be updated, shows a warning badge such as `1 update`. Columns shrink to fit the terminal width (minimums 9, 9, 11, 8, 14, 9 with the two-column gutter); if that is not enough the `Binary` and then the `Latest` column are hidden, and cells that still do not fit end in `…`. When the terminal is too short for every tool, the table scrolls with the selection and the card header shows the range (`1–8 of 20`). The banner under the title reports on `forge-toolbox` itself:

| Note | Tone | When |
|---|---|---|
| `forge-toolbox update available: X -> Y. run forge-toolbox self-update` | warning | A newer version exists. `forge-toolbox self-update` runs `cargo install forge-toolbox --registry ennor --force` with its output on the terminal (or prints that it is up to date). |
| `forge-toolbox is up to date` | success | Same version. |
| `forge-toolbox is not installed (latest Y)` | info | Not in `cargo install --list` (for example run with `cargo run`). |
| `could not determine latest forge-toolbox version from registry` | gray | Lookup failed. |

The last row is the message: `Ready` at start, then the result of the last action (success tone for `installed`/`updated`, error tone for a failure), or, in the brand tone, a spinner (`⠋ running <package> (installing)` / `(updating)`) while an install runs. The terminal cursor is hidden while the screen is up.

## Keys

| Key | Action |
|---|---|
| `Up` | Move the selection up (stops at the first row). |
| `Down` | Move down (stops at the last row). |
| `Enter` | Act on the selected crate (below). Ignored with `busy` while an action runs. |
| `r` | Re-read installed versions and the registry (message `Status refreshed`). Ignored while an action runs. |
| `q` | Quit. Works while an action runs too (the install keeps running in its thread until the process exits). |

Other keys do nothing. (`k`/`j` are not bound; `Ctrl-C` is not special-cased because raw mode is on: use `q`.)

## Enter: install and update

* Not installed and installable: runs `cargo install <package> --registry ennor` in a background thread (output captured, UI keeps redrawing the spinner every 120 ms). Success: message `<package> installed`.
* Installed and updatable: the same with `--force`; success: `<package> updated`.
* Installed and up to date: nothing is run; message `<package> is already up to date`.
* Not in the registry: message `<package> is not installable from registry ennor`.
* A failing `cargo install` shows the message `action failed: <package>: <first non-empty stderr line>` (or `cargo install failed`).

After any action the whole state is refreshed (same as `r`), keeping the selection.

Because the install runs `cargo install` with output captured, a long compile shows only the spinner.

## Monitored crates

Discovered from the registry at every start and refresh: toolbox asks `GET <registry>/api/v1/crates?bins=true&per_page=100` and lists each crate that builds a binary and is a command-line tool (the `binary` column is its first `[[bin]]`), sorted by package. A tool declares itself with the `command-line-utilities` or `development-tools` category; `-service` crates (they ship as images), GUI and fixture crates and `forge-toolbox` itself are left out. Warehouse reads the binary names from the `Cargo.toml` inside the crate's newest `.crate` and reads its `categories` too, and caches both as `manifest-info.json` beside it, so crates published earlier appear without a republish. If the registry cannot be asked, or lists no tools, toolbox falls back to the fixed list `MONITORED_CRATES` in the source; see [Configuration](configuration.md).
