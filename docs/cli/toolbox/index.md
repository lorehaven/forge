---
title: toolbox
parent: Command-line tools
nav_order: 8
has_children: true
permalink: /cli/toolbox/
---

Forge Toolbox is a full-screen terminal table that shows which Forge command-line tools are installed with `cargo install`, what the private registry offers, and installs or updates one with a keypress. Package `forge-toolbox`, binary **`forge-toolbox`**, library crate `forge_toolbox` (the directory is `forge/cli/toolbox`).

Version: `forge-toolbox 0.1.17` (`forge/cli/toolbox/Cargo.toml`).

## Install

```bash
cd forge/cli/toolbox
cargo install --path .
```

Requires `cargo` on `PATH` (checked at start: `` `cargo` not found ... toolbox drives cargo search/install against the workspace registry ``) and a Cargo registry named `ennor` configured in `~/.cargo/config.toml` (`sparse+https://ennor.ddns.net/index/` is shown on screen; the name `ennor` is hard-coded).

## Synopsis

```text
forge-toolbox
forge-toolbox --help | -h
forge-toolbox --version | -V
```

There are no subcommands, flags or arguments. The `--help` output is, verified:

```text
Interactive monitor and installer for Forge crates

Usage: forge-toolbox [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

and `--version` prints `forge-toolbox 0.1.17`. The parser struct has no arguments, so any other argument should be rejected by the parser (Unverified: not run, because a call that were accepted would start the registry queries).

## What starting it does

Verified for `--help`/`--version` only. Starting the interface (not run for the book: it queries the private registry and the installed crates): prints `toolbox launching interactive monitor`; runs `cargo install --list` once and `cargo search <package> --limit 10 --registry ennor` for each monitored crate and for `forge-toolbox` itself (nine searches); then enters raw terminal mode and the alternate screen and draws the table. The terminal is restored on exit (also on an error return).

## Exit codes

| Code | When |
|---|---|
| `0` | Left with `q`. |
| `1` | `cargo` missing, `cargo install --list` fails, the terminal cannot be put in raw mode, or reading terminal events fails (`Error: <message>`). A failed `cargo search` or `cargo install` is **not** fatal; it is shown in the table or the status line. |

## Where to go next

[Commands](commands.md) (the screen and the keys) | [Configuration](configuration.md) | [Recipes](recipes.md)
