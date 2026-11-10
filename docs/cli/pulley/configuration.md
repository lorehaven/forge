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
src = "/home/user/Documents"      # required, local directory or rsync remote (user@host:/path)
dest = "/tmp/documents"           # required, local path or rsync remote (host:path)
delete = true                     # default false: pass --delete when syncing
skip = ["temp", "cache"]          # default []: one --exclude per entry
no-confirm = false                # default false: skip the "Continue? (y/n)" prompt
interval = 300                    # optional, seconds: daemon-eligible
marker = "projects"               # optional: a path in `src` that must exist (nothing needs creating)
```

| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | Job id. |
| `desc` | string | required | Description. |
| `src` | string | required | rsync source: a local directory, or a remote one (`user@host:/path`, `host:/path`, `rsync://host/module`). |
| `dest` | string | required | rsync destination. |
| `delete` | bool | `false` | Delete files in `dest` that are not in `src` (update step only; the dry run always reports deletions). |
| `skip` | array of string | `[]` | rsync `--exclude` patterns. |
| `no-confirm` | bool | `false` | Do not ask before syncing (REPL and `run`; the daemon never asks). |
| `interval` | integer | none | Seconds between runs under `pulley daemon`; without it the job is invisible to the daemon. |
| `marker` | string | none | Optional: a path **relative to `src`** that must exist for the job to run (an existing file or directory; nothing has to be created). Normally not needed, see [Sources that are sometimes absent](#sources-that-are-sometimes-absent). |

The top level must contain `jobs` (a file without `[[jobs]]` fails to parse). Unknown keys are ignored. The sample is `forge/cli/pulley/example.pulley.toml`.

## Sources that are sometimes absent

A source can be absent in two ways, and the second is the dangerous one with `delete = true`:

* **The host is off** or cut off. rsync fails and nothing at the destination changes.
* **The host is up but the data is not**: a drive that is not mounted yet. The source answers and looks empty, so a sync that deletes would make the destination match it and empty it.

Nothing has to be created on the source to tell them apart. Before every sync, the daemon and `pulley run` check that the source **answers and exists** and, for a job with `delete = true`, that it **holds something**: an empty source is what an unmounted drive looks like. If the check fails the job is **skipped, and nothing is synced or deleted**. (If the data lives in a folder on the drive, such as `/mnt/data/projects`, an unmounted drive means the folder does not exist, which is caught by the first check.) A source that is present is synced exactly as before: deleting a whole project, thousands of files and all, propagates like any other deletion. Nothing here looks at *how much* would change.

* For a remote source the check is one `rsync --list-only` over the same `ssh`, so ssh aliases, ports and jump hosts apply. It uses `BatchMode` (it never waits for a password) and a 5 second connect timeout.
* The one case this holds back: a job with `delete = true` whose source you have emptied **completely** on purpose looks the same as an unmounted drive, so the backup is not emptied to match. Remove the old backup by hand, or give the job a `marker` (below).
* `marker` is optional. Name any path that is always in the source (an existing file or directory, say `projects`); the job then runs only while that path exists, which also stands in for the "not empty" rule. Use it when an empty source is legitimate.
* The daemon looks again at an absent source about once a minute (or every `interval`, if shorter) and syncs as soon as it is back, not after the full interval.

Transfers to or from a remote host use `ssh -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4`, so a host that is off costs seconds rather than the system's two minutes and one that goes off mid-transfer ends the transfer. Set `RSYNC_RSH` to use your own remote shell; pulley then adds nothing. (Not on Windows, whose rsync may bring its own `ssh`.)

## Job logs

The daemon (and only the daemon) appends to `~/.config/pulley/logs/<job id>/<YYYY-MM-DD>.log` (local time), lines `[HH:MM:SS] <message>` with the messages `running job`, `no changes`, `changes detected, syncing`, `sync completed`, `dry-run failed: <e>`, `sync failed: <e>`, and for an absent source `source not available: <why>` and `source is available again`. An absent source is logged **once** when it goes and once when it returns, not at every check, so a host that is off for hours does not fill the log. Log files older than 7 days (by their file name date) are deleted on each append. Failures to write logs are ignored.

## Environment

`HOME` (or `USERPROFILE`) locates the global directory and logs; `USER`/`SUDO_USER` and `HOME` are used by `service install`. No pulley-specific variables.

## Stray directory

The source tree of the CLI contains a directory literally named `C:\Users\me` (empty, created by a path typed on Linux). It is not read by anything.
