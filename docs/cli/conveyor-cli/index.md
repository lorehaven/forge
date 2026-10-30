---
title: conveyor-cli
parent: Command-line tools
nav_order: 6
has_children: true
permalink: /cli/conveyor-cli/
---

The terminal client of the Conveyor CI/CD service. Package `conveyor-cli`, binary **`conveyor`**, library crate `conveyor_cli` (modules `cli`, `client`, `commands`, `config`). It registers repositories, starts and inspects runs, streams logs, manages projects, secrets and git credentials through Conveyor's HTTP API, and validates `.conveyor.toml` pipeline files locally.

Version: `conveyor 0.3.28` (`forge/cli/conveyor-cli/Cargo.toml`).

## Install

```bash
cd forge/cli/conveyor-cli
cargo install --path .       # installs `conveyor`
```

## Global options

Defined on the top-level command with `global = true` (accepted before or after the subcommand). Each has a fallback chain: flag, then environment variable, then the config file (see [Configuration](configuration.md)).

| Option | Env fallback | Meaning |
|---|---|---|
| `--url <URL>` | `CONVEYOR_URL` | Conveyor including its base path, e.g. `https://localhost:9443/conveyor`. Required for every command except `validate`. A trailing `/` is removed. |
| `--username <NAME>` | `CONVEYOR_USERNAME` | Realm account. When set, the CLI logs in to Gatehouse and sends the resulting bearer token with every request; when absent, requests carry no `Authorization`. |
| `--password <PASSWORD>` | `CONVEYOR_PASSWORD` | Its password (empty string if unset). A flag is visible in the process list: prefer the environment variable. |
| `--gatehouse-url <URL>` | `GATEHOUSE_URL` | Gatehouse with base path; required whenever a username is in effect. |
| `--insecure` | `CONVEYOR_INSECURE` (boolean), config `insecure` | Accept self-signed certificates (for Gatehouse login and Conveyor). The env var and config can only turn it on. |
| `-h`, `--help`, `-V`, `--version` | | `conveyor 0.3.28`. |

A `.env` in the working directory is loaded into the environment at start (failure ignored).

## Commands

```text
conveyor repo     add | list | enable | disable | move | set-branch | remove
conveyor run      <REPO> [--git-ref REF] [--sha SHA] [--stage STAGE]... [--wait]
conveyor runs     [--repo REPO] [--limit N]
conveyor show     <RUN_ID>
conveyor logs     <ID> [-f|--follow]
conveyor cancel   <RUN_ID>
conveyor project  add | list | show | rename | move | remove
conveyor secret   set | list | remove
conveyor credential set | show | remove
conveyor validate [PATH]
```

See [Commands](commands.md).

## Exit codes

| Code | When |
|---|---|
| `0` | Success. `run --wait`: the run ended `success` or `skipped`. |
| `1` | Any error: one line `conveyor <message>` written with `print_status`, i.e. on **stdout** (the code comment says stderr; see the report). `run --wait`: the run ended `failed` or `cancelled` (`run failed: <reason>`). `validate`: invalid pipeline. |
| `2` | Argument parse error. |

HTTP errors are shown as the server's `error` field (or the raw body, or `conveyor answered <status>`); a `401` appends `(set CONVEYOR_USERNAME and CONVEYOR_PASSWORD)`.

## Verification status

Verified locally: `--version`, `--help`, `validate` (valid file, missing file, invalid file). Unverified (not run: they talk to a Conveyor service): every other command, documented from `commands.rs` and `client.rs`.

## Where to go next

[Commands](commands.md) | [Configuration](configuration.md) | [Recipes](recipes.md)
