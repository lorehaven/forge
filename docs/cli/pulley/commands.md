---
title: Commands
parent: pulley
grand_parent: Command-line tools
nav_order: 1
---

Verified locally with a scratch `HOME` and the example config: `--version`, `--help`, `list`, the REPL's non-sync commands, the missing-config failure, argument errors. Not run (they run rsync, a daemon, or change system service state): `run` (past job selection), `daemon`, `service install|uninstall|status`.

Every command except `service` first loads the merged configuration and prints, on stdout:

```text
pulley loaded configuration from:
  - ./a.pulley.toml
config loaded successfully. 3 job(s) found.

```

(the `loaded configuration from` block lists every file that parsed; a file that fails to parse prints `pulley failed to load <file>: <error>` as a warning and is skipped). If no jobs result, it prints `config failed to load: Config file not found: Global: <dir>/*.toml, Local: *.pulley.toml` on stdout, a help text with an example job on stderr, and exits 1.

## pulley (no command)

Starts the [REPL](repl.md). Requires `rsync`.

## list

```text
pulley list
```

Prints the configured jobs and exits (the REPL's `list`):

```text
Configured jobs:
  documents - Backup documents folder
    src: /home/user/Documents
    dest: /tmp/documents
    delete: true
    skip: temp, cache
    interval: 300s (daemon-eligible)

  remote-photos - Backup photos from remote server
    src: user@server.com:/home/user/photos
    dest: /mnt/backup/photos
    skip: thumbnails
    no-confirm: true
```

Only non-default fields are shown (`delete`, `skip`, `no-confirm`, `interval`). `No jobs configured` cannot normally occur.

## run

```text
pulley run <JOB_ID>...
pulley run all
```

At least one id is required (parse error otherwise). `all` (as the first id) selects every job. Otherwise the jobs named are selected, **in configuration order, not argument order**, and unknown ids are silently ignored (`No matching jobs found` only if none match). Then:

```text
Jobs to be run: documents, projects

Starting job: `Backup documents folder`
Executing dry-run
CREATE a.txt
MODIFY b.txt
DELETE  old.txt
Backup documents folder: 3 total changes
Continue? (y/n):
```

*Unverified:* sample from the source (`rsync.rs`, `repl.rs`).

For each job: a dry run (see [Sync behaviour](#sync-behaviour)); if there are changes, then unless the job has `no-confirm = true` it asks `Continue? (y/n): ` and proceeds only on `y`/`Y` (anything else prints `Skipped`); `no-confirm = true` proceeds directly; no changes prints `**Nothing to do**`. Before each job the source is checked (see [Sources that are sometimes absent](configuration.md#sources-that-are-sometimes-absent)): a job whose source is not there prints `skipped: job <id>: source not available (<why>)` and nothing is synced or deleted for it; the other jobs still run, and at the end the command fails (exit 1) naming the skipped jobs. Finally `Done job: <desc>`. An rsync failure aborts the whole run with `Error: rsync exited with <status>: <stderr>` (exit 1), skipping later jobs. `run` reads confirmations from standard input, so a job without `no-confirm` blocks in a non-interactive script.

## daemon

```text
pulley daemon
```

Foreground loop. Requires `rsync`. Only jobs with `interval` are watched; with none: `daemon no jobs have interval set; nothing to watch. add interval = <seconds> to a job to include it.` and exit 0. Otherwise prints `daemon watching N job(s)` and `  <id> - every <N>s`, then runs the jobs as follows. It never exits on its own (stop with Ctrl-C or the service manager); it does exit, with an error, if a worker dies, so a service manager restarts it rather than leaving jobs unwatched. On Windows the console window is hidden first.

**Jobs run at the same time.** Each job has a worker of its own, so a copy that takes hours (the first sync of a big drive) does not hold up the others. Jobs that share a **destination** share a worker and run one after another, in the order of the files, because two rsyncs must not write to one destination at once; the daemon says so when it starts. Give jobs distinct destinations if they should run side by side.

**Each pass** (every 1 s, per worker) finds the jobs that are due (never run, or `interval` seconds since the last start). For each it first checks the source (see [Sources that are sometimes absent](configuration.md#sources-that-are-sometimes-absent)). If the source is not there the job is **not run**; a single `daemon job <id>: source not available (<why>)` line is printed and logged, the source is looked at again about a minute later, and when it is back `daemon job <id>: source is available again` is printed and the job runs at once. A source that is there is synced: dry run; if there are changes, **sync without asking** (`no-confirm` is irrelevant in the daemon).

**Quiet output.** The daemon prints and logs counts, never a line per file, and shows no progress: copying millions of files writes a handful of lines, so a journal does not drop the lines that matter. Per job and run: `daemon running job <id>`, then, if something changed, `daemon job <id>: 6000 new, 12 changed, 3 deleted; synced in 4m12s`. The job's log has the same (see [Configuration](configuration.md#job-logs)). To see every file, run the job with `pulley run <id>`.

**A host that goes away mid-sync.** If a sync fails and the source cannot be reached right after, it is treated as a source that is not there, not as a failed job: it is reported once, stays due, and resumes within a minute of the host coming back, not after its whole `interval`. A sync that fails with the source still there prints `daemon job <id> failed: <error>`, counts as an attempt (tried again at its `interval`), and the loop continues.

## service

```text
pulley service install
pulley service uninstall
pulley service status
```

Manages a background service that runs `pulley daemon`. The backend is detected at call time (Unix): systemd if `/run/systemd/system` exists, else runit if PID 1 is `runit`; otherwise `no supported init system detected (looked for systemd at /run/systemd/system, runit as PID 1); pulley service management currently supports systemd and runit`. On Windows it uses a scheduled task.

| Backend | install | uninstall | status |
|---|---|---|---|
| systemd (user) | Writes `~/.config/systemd/user/pulley.service`, `systemctl --user daemon-reload`, `systemctl --user enable --now pulley.service`; warns `user services only start at login by default; to start pulley at boot run: loginctl enable-linger <user>` if linger is off | `systemctl --user disable --now pulley.service`, removes the unit, `daemon-reload` | `systemctl --user status pulley.service` |
| runit | **Needs root** (`re-run as: sudo <exe> service install`): writes `/etc/sv/pulley/run` (`#!/bin/sh` / `exec chpst -u <user> <exe> daemon`, mode 0755, `<user>` = `$SUDO_USER`, else `$USER`, else `root`), symlinks it into the first existing of `/var/service`, `/etc/service`, `/run/runit/service`, runs `sv up` | `sv down`, removes the link and `/etc/sv/pulley` (root) | `sv status <link>` |
| Windows | `schtasks /Create /TN Pulley /TR "<exe> daemon" /SC ONLOGON /RL LIMITED /F`, then `/Run` | `/End` then `/Delete /TN Pulley /F` | `schtasks /Query /TN Pulley /V /FO LIST` |

The systemd unit (`unit_file_contents`):

```ini
[Unit]
Description=Pulley continuous sync daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=<path of the pulley binary> daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

The unit records the absolute path of the binary that ran `install`; reinstall after moving or upgrading it. Requires `systemctl` (`pulley service management needs a systemd user session`), `sv` and `chpst` for runit, `schtasks` on Windows.

## Sync behaviour

Both the dry run and the update run `rsync` with the job's **source directory as working directory** and the paths `"<src>/"` and `"<dest>/"` (a trailing slash is always appended, so the contents of `src` are synced into `dest`).

| Step | rsync arguments |
|---|---|
| Dry run | `-avz --dry-run --itemize-changes --delete` plus one `--exclude=<skip>` per `skip` entry (`-az` in the daemon, whose lines are counted as they arrive and never kept) |
| Update | `-avzu --progress` plus `--delete` only if the job sets `delete = true`, plus the excludes (`-azu` without `--progress` in the daemon) |

The dry run **always** includes `--delete`, so it reports deletions even for a job with `delete = false` that will not perform them. The report counts lines starting `*deleting` (printed `DELETE  <path>`), creations (`>f+++++++++`, `<f+++++++++`, `cd+...`: `CREATE <path>`) and file modifications (`>f...`: `MODIFY <path>`); directory-only lines are not shown. The update's output is shown on stderr. `-u` skips files newer on the destination.

For a local source the working directory is set to `src`; a remote source (`user@host:/path`, `host:/path`, `rsync://host/module`) has no directory to enter, so none is set. Transfers that involve a remote host get an `ssh` with timeouts and keepalives (see [Configuration](configuration.md#sources-that-are-sometimes-absent)).
