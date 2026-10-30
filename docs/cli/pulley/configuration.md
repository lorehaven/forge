---
title: Configuration
parent: pulley
grand_parent: Command-line tools
nav_order: 3
---

## Config files

| Source | Pattern | Read |
|---|---|---|
| Global | `$HOME/.config/pulley/*.toml` (on Windows `HOME`, else `USERPROFILE`), non-recursive, sorted by name | first |
| Local | `*.pulley.toml` in the **current directory**, sorted by name | second (overrides) |

Files are merged job by job on `id`: a job whose id already exists replaces the earlier one in place (keeping its position); new ids are appended. A file that cannot be read or parsed is skipped with `pulley failed to load <path>: <error>`. If no job remains the program fails (see [Commands](commands.md)). Global `logs/` directories are not config (only `*.toml` files directly in `~/.config/pulley/` count).

## File schema

```toml
[[jobs]]
id = "documents"                  # required, unique key for merging and `run`
desc = "Backup documents folder"  # required, shown in output
src = "/home/user/Documents"      # required, local directory
dest = "/tmp/documents"           # required, local path or rsync remote (host:path)
delete = true                     # default false: pass --delete when syncing
skip = ["temp", "cache"]          # default []: one --exclude per entry
no-confirm = false                # default false: skip the "Continue? (y/n)" prompt
interval = 300                    # optional, seconds: daemon-eligible
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | Job id. |
| `desc` | string | required | Description. |
| `src` | string | required | rsync source; also pulley's working directory for rsync, so it must be a local directory. |
| `dest` | string | required | rsync destination. |
| `delete` | bool | `false` | Delete files in `dest` that are not in `src` (update step only; the dry run always reports deletions). |
| `skip` | array of string | `[]` | rsync `--exclude` patterns. |
| `no-confirm` | bool | `false` | Do not ask before syncing (REPL and `run`; the daemon never asks). |
| `interval` | integer | none | Seconds between runs under `pulley daemon`; without it the job is invisible to the daemon. |

The top level must contain `jobs` (a file without `[[jobs]]` fails to parse). Unknown keys are ignored. The sample is `forge/cli/pulley/example.pulley.toml`.

## Job logs

The daemon (and only the daemon) appends to `~/.config/pulley/logs/<job id>/<YYYY-MM-DD>.log` (local time), lines `[HH:MM:SS] <message>` with the messages `running job`, `no changes`, `changes detected, syncing`, `sync completed`, `dry-run failed: <e>`, `sync failed: <e>`. Log files older than 7 days (by their file name date) are deleted on each append. Failures to write logs are ignored.

## Environment

`HOME` (or `USERPROFILE`) locates the global directory and logs; `USER`/`SUDO_USER` and `HOME` are used by `service install`. No pulley-specific variables.

## Stray directory

The source tree of the CLI contains a directory literally named `C:\Users\me` (empty, created by a path typed on Linux). It is not read by anything.
