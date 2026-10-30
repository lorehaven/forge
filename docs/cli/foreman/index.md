---
title: foreman
parent: Command-line tools
nav_order: 5
has_children: true
permalink: /cli/foreman/
---

Foreman runs a project's local development estate from one `foreman.toml`: Docker containers the services sit on, tasks that must run first (schema install), and the services themselves, started from their built binaries with health checks, pid files and logs.

Version: `foreman 0.1.13` (`forge/cli/foreman/Cargo.toml`). Binary and library crate: `foreman`.

## Install

```bash
cd forge/cli/foreman
cargo install --path .
```

External programs used: `docker` (containers; `docker version`, `run`, `ps`, `exec`, `stop`), `curl` (health checks, `curl -sk -o /dev/null --max-time 2 <url>`), `pgrep` (`[[warnings]]`), `tail` (`logs`), `sh` (pre-stop hooks), plus whatever the config's `build`/`command` entries run (typically `cargo`).

## Synopsis

```text
foreman [COMMAND]
foreman [SERVICE]...            # no command: start (the services named, or everything)
foreman start [SERVICES]...
foreman stop [SERVICES]... | all
foreman status
foreman logs <SERVICE>
foreman repl [SERVICES]...      # alias: pick
foreman db
foreman reset
foreman test [ARGS]...
foreman run <TASK> [SERVICES]...
foreman list
foreman env <SERVICE>
foreman init [--force]
```

No global options besides `-h/--help` and `-V/--version` (prints `foreman 0.1.13`). See [Commands](commands.md).

## The configuration file

`foreman.toml` (or `.foreman.toml`) is searched for in the working directory and each parent. The directory that holds it is the **project root**: every relative path in the file is relative to it, and runtime state goes to `<root>/.run/` (`pid files`, `logs/<service>.log`). Every command except `init` needs one: `error  no foreman.toml or .foreman.toml found in <dir> or any parent directory (foreman init writes one)`. All keys are on the [Configuration](configuration.md) page; unknown keys are errors.

## Output and exit codes

Output is one line per event: `<label (12 columns)> <message>`, colour by tone (info, ok, warn, error). Errors are printed as `error        <message>` plus the cause chain, indented.

| Code | When |
|---|---|
| `0` | Success. |
| `1` | Any error (config not found or invalid, unknown service, a service or task failed). `foreman test` also returns `1` when the suite fails, `0` when it passes. |
| `2` | Argument parse error. |

## Safety note

`start`, `stop`, `db`, `reset`, `run` and `test` change the machine (spawn or kill processes, start or stop containers, drop databases). The commands that only read are `list`, `env`, `status` (it runs `docker ps` and reads pid files) and `init` (writes one file). The examples in this book were produced in a scratch directory with the starter config and nothing was started.

## Where to go next

* [Commands](commands.md), [REPL (picker)](repl.md), [Configuration](configuration.md), [Recipes](recipes.md).
