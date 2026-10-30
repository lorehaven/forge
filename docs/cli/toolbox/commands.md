---
title: Commands
parent: toolbox
grand_parent: Command-line tools
nav_order: 1
---

The interface (not run for the book; documented from `main.rs` and `lib.rs`, whose behaviour is covered by `tests/unit`).

## The screen

```text
Forge Toolbox Status

registry: ennor (sparse+https://ennor.ddns.net/index/)
note: forge-toolbox is up to date
controls: ↑/↓ move, Enter install/update, r refresh, q quit

┌──┬─────────────┬─────────┬──────────────────┬────────┬───────────┬────────┐
│  │package      │binary   │installed         │latest  │updatable  │action  │
├──┼─────────────┼─────────┼──────────────────┼────────┼───────────┼────────┤
│> │anvil        │anvil    │0.4.23            │0.4.24  │yes        │update  │
│  │conveyor-cli │conveyor │no (can install)  │0.3.28  │no         │install │
│  │...          │         │                  │        │           │        │
└──┴─────────────┴─────────┴──────────────────┴────────┴───────────┴────────┘

status: Ready
```

*Unverified:* constructed from `render()`; versions are illustrative.

Columns:

| Column | Content |
|---|---|
| (marker) | `>` on the selected row. |
| `package` | Cargo package name. |
| `binary` | Binary the package installs. |
| `installed` | Version from `cargo install --list`; `no (can install)` if absent but in the registry; `no` if absent and not found in the registry. |
| `latest` | Newest version from `cargo search <package> --limit 10 --registry ennor` (the first result line whose name matches exactly), or `n/a`. |
| `updatable` | `yes` when `latest` is greater than `installed`. |
| `action` | `install` (not installed and installable), `update` (updatable), else `-`. |

A package whose registry lookup failed has its row followed by `warn: Failed to fetch latest version for <package>: <cause>`. Columns shrink to fit the terminal width (minimums 7, 7, 12, 6, 9, 6) and cells that still do not fit are cut with `~`. The `note:` line reports on `forge-toolbox` itself:

| Note | When |
|---|---|
| `note: forge-toolbox update available: X -> Y. run forge-toolbox self-update` | A newer version exists. **There is no `self-update` command** (see the report); update it with `cargo install forge-toolbox --registry ennor --force`. |
| `note: forge-toolbox is up to date` | Same version. |
| `note: forge-toolbox is not installed (latest Y)` | Not in `cargo install --list` (for example run with `cargo run`). |
| `note: could not determine latest forge-toolbox version from registry` | Lookup failed. |

The last line is `status: <message>`: `Ready` at start, then the result of the last action, or a spinner (`⠋ running <package> (installing)` / `(updating)`) while an install runs.

## Keys

| Key | Action |
|---|---|
| `Up` | Move the selection up (stops at the first row). |
| `Down` | Move down (stops at the last row). |
| `Enter` | Act on the selected crate (below). Ignored with `busy` while an action runs. |
| `r` | Re-read installed versions and the registry (`status: Status refreshed`). Ignored while an action runs. |
| `q` | Quit. Works while an action runs too (the install keeps running in its thread until the process exits). |

Other keys do nothing. (`k`/`j` are not bound; `Ctrl-C` is not special-cased because raw mode is on: use `q`.)

## Enter: install and update

* Not installed and installable: runs `cargo install <package> --registry ennor` in a background thread (output captured, UI keeps redrawing the spinner every 120 ms). Success: `status: <package> installed`.
* Installed and updatable: the same with `--force`; success: `<package> updated`.
* Installed and up to date: nothing is run; `status: <package> is already up to date`.
* Not in the registry: `status: <package> is not installable from registry ennor`.
* A failing `cargo install` shows `status: action failed: <package>: <first non-empty stderr line>` (or `cargo install failed`).

After any action the whole state is refreshed (same as `r`), keeping the selection.

Because the install runs `cargo install` with output captured, a long compile shows only the spinner.

## Monitored crates

Fixed in the source (`MONITORED_CRATES`); see [Configuration](configuration.md).
