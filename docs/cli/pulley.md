# Pulley

Pulley is an interactive, REPL-based backup/sync tool built on `rsync`, configured through TOML files. It exists in the workspace as a lightweight, scriptable alternative to ad hoc rsync invocations: jobs are declared once in TOML, previewed with a dry run, and re-run from a persistent REPL session without re-typing flags.

## Features

- Interactive REPL (`list`, `run`, `reload`, `help`, `quit`/`exit`) instead of one-shot CLI invocations.
- Multi-file TOML configuration, merged from a global directory and local project files.
- Dry-run preview (via `rsync --dry-run --itemize-changes`) showing creates/modifies/deletes before anything happens, with a confirmation prompt unless `no-confirm` is set.
- Local and remote (`user@host:/path`) sources, since `src`/`dest` are passed straight to `rsync`.
- Per-job directory exclusion (`skip`) and optional deletion sync (`delete`, applied to `rsync --delete` on the real run).
- A constant sync mode (`pulley daemon`) that polls jobs on their own interval, unattended, and a `pulley service` subcommand to run it as a systemd user service from boot.

## Requirements

- Rust (stable), to build it.
- `rsync` on `PATH`.
- `ssh` on `PATH` for remote sources.

## Usage

Build it with `cargo build --release -p pulley`, or run it directly:

```bash
cargo run -p pulley
# or, once built:
pulley
pulley --version   # / -V
```

### REPL commands

| Command | Purpose |
|---|---|
| `list` | List all configured jobs |
| `run <job_id> [job_id2...]` | Run specific job(s) by id |
| `run all` | Run every configured job |
| `reload` | Re-read and re-merge the global/local config files |
| `help` | Show available commands |
| `quit` / `exit` | Exit the REPL |

Each `run` first does an `rsync --dry-run` pass and prints a summary; if it finds changes and the job isn't `no-confirm`, it asks `Continue? (y/n)` before doing the real `rsync` transfer.

## Configuration

Pulley reads every `*.toml` file in `~/.config/pulley/` (global) and every `*.pulley.toml` file in the current directory (local), sorted alphabetically within each group. Local configs are merged after (and so override) global ones; jobs are matched and overwritten by `id`, otherwise appended. At least one job must be found across all files or Pulley exits with an error explaining where it looked.

```toml
[[jobs]]
id = "documents"
desc = "Backup documents folder"
src = "/home/user/Documents"
dest = "/mnt/backup/documents"
delete = true
skip = ["temp", "cache"]
no-confirm = false
```

Job fields: `id` (unique key used for merging), `desc`, `src`, `dest`, `delete` (default `false`), `skip` (list of `--exclude` names, default empty), `no-confirm` (skip the confirmation prompt, default `false`), `interval` (seconds between runs under `pulley daemon`; omit to keep a job REPL-only).

An example file ships at `cli/pulley/example.pulley.toml`.

## Constant sync mode

`pulley daemon` runs in the foreground (no REPL, no prompts) and continuously polls every job that sets `interval`, re-syncing it once that many seconds have elapsed since its last run. Jobs without `interval` are invisible to the daemon but remain runnable manually from the REPL — the same config files drive both modes. Each due job runs the same dry-run-then-update flow as `run`, but unattended: if the dry run finds changes, `pulley daemon` applies them immediately, without a confirmation prompt.

```bash
pulley daemon
```

### Running it as a service

`pulley service` manages a systemd **user** unit that runs `pulley daemon`:

| Command | Purpose |
|---|---|
| `pulley service install` | Write `~/.config/systemd/user/pulley.service`, `daemon-reload`, then `enable --now` |
| `pulley service uninstall` | `disable --now`, remove the unit file, `daemon-reload` |
| `pulley service status` | `systemctl --user status pulley` |

The installed unit points `ExecStart` at whichever `pulley` binary ran `install` (`std::env::current_exe()`), with `Restart=on-failure`.

User services normally only start when you log in. To have pulley start at boot as well, enable lingering for your account once:

```bash
loginctl enable-linger $USER
```

`pulley service install` checks this and prints the exact command if lingering isn't already on.

[Home](../README.md)
