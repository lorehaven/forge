---
title: REPL
parent: riveter
grand_parent: Command-line tools
nav_order: 4
---

An interactive shell over the current environment. Start it with `riveter repl` or just `riveter`.

```text
$ riveter
┌─────────────────────────────────────────────┐
│ Riveter REPL                                │
│ env-aware manifest commands                 │
└─────────────────────────────────────────────┘
hint type `help` to list commands
riveter(dev)> list
  namespace   dev-ns  immutable
  deployment  api     mutable
  service     api     mutable
riveter(dev)>
```

* **Prompt**: `riveter(<env>)> `, where `<env>` is the current environment (`unset` when none is resolvable). The prompt is recomputed after each command, so `env set x` changes it.
* **Line editing** (the `quench-cli` line editor): arrow keys, Home/End, Ctrl-A/E/B/F/K/U/W/L/P/N/T, Alt-B/F, Up/Down browse history. History is kept in memory for the session only. No tab completion.
* **Quit**: `exit`, `quit`, `q`, Ctrl-D on an empty line, or Ctrl-C.
* Words are split on whitespace: **no quoting**, so a target containing a space cannot be given, and no shell expansion happens (patterns such as `*/api` need no quotes).
* The environment is always the current one (`$RIVETER_ENV`, else `env set`); `--env` is not available, and `-e` is rejected as an unknown option.
* Errors print as `error <message>` and the loop continues.

## Commands

| Command | Aliases | Options | Notes |
|---|---|---|---|
| `help [command]` | `h` | | Overview, or detail for one command; `help targets`. Unknown topic: a warning with suggestions. |
| `env list` / `env set <env>` / `env show` | | | `env set` prints `ok environment set to <env>`. |
| `list [options] [target...]` | `ls` | `--scope` (default `all`) | |
| `render [options] [target...]` | `r` | `--scope` (default `mutable`) | |
| `apply [options] [target...]` | `a` | `--scope` (default `mutable`), `--dry-run` | Waits for rollouts with the default 300 s timeout. |
| `diff [options] [target...]` | `df` | `--scope` (default `mutable`) | `kubectl diff` only. |
| `delete [options] [target...]` | `del`, `d` | `--scope` (default `mutable`) | Prints a `warn deleted N resource(s) ...` line. |
| `prune [--dry-run]` | | `--dry-run` | The flag is detected anywhere on the line. |
| `validate [options] [target...]` | | `--scope` (default `all`) | |
| `images` | | `--update`, `--registry-auth <value>` | Flags are read from anywhere in the line; `--registry-auth` takes the next token. |
| `exit` / `quit` / `q` | | | |

`--scope value` and `--scope=value` are both accepted; values are case-insensitive. Unknown options fail closed: `unknown option `--x``, then `accepted options: --scope <mutable|immutable|all>` (plus `, --dry-run` for `apply`). Anything else typed: `warn unknown command — type help`.

Not available in the REPL (CLI only): `pack`, `publish`, `pull`, `install`, `login`, `secrets`, `schemas`, `remote`, `validate --file`, `diff --against`, multi-environment `--env a,b`.

## Mismatches between `help` and the parser

The REPL's `help apply` (and `help diff`, `help validate`) is generated from the same table as the CLI help and lists options the REPL parser rejects:

| Listed by `help` | Actual REPL behaviour |
|---|---|
| `apply --replicas`, `apply --except` | Not accepted anywhere for `apply` (only `install` has them). |
| `apply --no-wait`, `apply --timeout <seconds>` | Rejected: `unknown option --no-wait`. The code checks `--no-wait` in the argument list but the strict parser has already refused it. |
| `diff --against <git-ref>` | Rejected: `unknown option --against` (verified). |
| `--env canary,prod apply` example | Not possible in the REPL. |

Verified:

```text
riveter(dev)> render --no-wait
error unknown option `--no-wait`

accepted options: --scope <mutable|immutable|all>
riveter(dev)> diff --against x
error unknown option `--against`

accepted options: --scope <mutable|immutable|all>
```
