---
title: pulley
parent: Command-line tools
nav_order: 7
has_children: true
permalink: /cli/pulley/
---

Pulley runs rsync-backed backup and sync jobs described in TOML, interactively (a small shell), once from the command line, continuously as a foreground daemon, or as a background service (systemd user unit, runit, or a Windows scheduled task).

Version: `pulley 0.1.22` (`forge/cli/pulley/Cargo.toml`). Binary and library crate: `pulley`.

## Install

```bash
cd forge/cli/pulley
cargo install --path .
```

Requires `rsync` on `PATH` for everything that syncs (REPL, `run`, `daemon`); `list` and `service` do not check for it. When missing, the error is `pulley shells out to it for every sync job; install it (e.g. apt install rsync / pacman -S rsync)` (on Windows the hint mentions WSL, MSYS2 and cwRsync).

## Synopsis

```text
pulley                           # interactive REPL
pulley list
pulley run <JOB_ID>... | all
pulley daemon
pulley service install | uninstall | status
```

No global options besides `-h/--help` and `-V/--version` (prints `pulley 0.1.22`). See [Commands](commands.md).

## Configuration summary

Jobs come from `*.toml` files in `~/.config/pulley/` (global) and `*.pulley.toml` files in the current directory (local), merged by job id with local winning. Full schema on [Configuration](configuration.md).

## Exit codes

| Code | When |
|---|---|
| `0` | Success, including `run` with names that match no job (`No matching jobs found`) and a REPL session ended by `exit`. |
| `1` | No jobs could be loaded (the config help text is printed), an rsync failure (`Error: rsync exited with <status>...`, printed by `main` returning an error), a missing `rsync`, a service-management error. |
| `2` | Argument parse error (for example `pulley run` without a job id). |

## Where to go next

[Commands](commands.md) | [REPL](repl.md) | [Configuration](configuration.md) | [Recipes](recipes.md)
