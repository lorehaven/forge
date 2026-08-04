# Conveyor CLI

Conveyor CLI (binary name `conveyor`) is the terminal client for [conveyor](../docker/conveyor-service.md), the Forge CI/CD service. It talks to conveyor's HTTP API to register repositories, start and inspect runs, stream logs, manage per-repo secrets, and validate `.conveyor.toml` pipeline files, so pipelines can be driven from a terminal or a script instead of only through conveyor's own UI.

## Features

- **Repository management**: register, list, enable/disable, and remove the repositories conveyor builds.
- **Runs**: start a run against a ref or exact commit, list recent runs, inspect one run's jobs (including why any were skipped) and artifacts, and cancel a run.
- **Logs**: print or follow (`--follow`) a run's or job's output.
- **Secrets**: set (from an argument or stdin), list, and remove secrets, scoped to the whole estate or one repository. Values are never read back.
- **Offline validation**: `validate` checks a `.conveyor.toml` using conveyor's own parser (linked in as a library), so what it accepts is exactly what a real run would accept.
- **Authentication via gatehouse**: a username/password is exchanged once at startup for a bearer token via gatehouse's `/api/v1/auth/login`; the password itself is never sent to conveyor.

## Requirements

- A running `conveyor` service reachable at the configured URL.
- `gatehouse` reachable at `GATEHOUSE_URL` whenever a username/password is supplied (conveyor no longer accepts credentials directly).

## Usage

```bash
export CONVEYOR_URL=https://localhost:9443/conveyor
export CONVEYOR_USERNAME=admin
export CONVEYOR_PASSWORD=…

conveyor repo add lorehaven/forge https://github.com/lorehaven/forge.git
conveyor run lorehaven/forge --wait
conveyor logs <run-id> --follow
```

Repositories can be referred to as `owner/name` anywhere an id is accepted.

### Commands

| Command | Purpose |
|---|---|
| `repo add <owner/name> <clone-url> [--provider github\|generic] [--default-branch <branch>]` | Register a repository. |
| `repo list` / `repo enable <repo>` / `repo disable <repo>` / `repo remove <repo>` | Manage repositories. |
| `run <repo> [--git-ref <ref>] [--sha <sha>] [--wait]` | Start a run. |
| `runs [--repo <repo>] [--limit <n>]` | List recent runs (default limit 20). |
| `show <run-id>` | One run: its jobs, why any were skipped, its artifacts. |
| `logs <run-id\|job-id> [--follow\|-f]` | Print or follow output. |
| `cancel <run-id>` | Ask a run to stop. |
| `secret set <name> [value] [--repo <repo>]` | Write a secret (reads from stdin if `value` is omitted). |
| `secret list [--repo <repo>]` / `secret remove <name> [--repo <repo>]` | Manage secrets. |
| `validate [path]` | Check a `.conveyor.toml` (default `.conveyor.toml`) without sending it anywhere. |

`run --wait` polls until the run rests and exits non-zero if it failed, so it can end a script:

```bash
conveyor run lorehaven/forge --wait || exit 1
```

`secret set NAME` with no value reads it from stdin, keeping it out of shell history and the process list:

```bash
printf '%s' "$TOKEN" | conveyor secret set DEPLOY_TOKEN --repo lorehaven/forge
```

## Configuration

Global flags (`--url`, `--username`, `--password`, `--gatehouse-url`, `--insecure`) beat environment variables, which beat the config file:

| Env var | Flag | Purpose |
|---|---|---|
| `CONVEYOR_URL` | `--url` | Where conveyor is. |
| `CONVEYOR_USERNAME` / `CONVEYOR_PASSWORD` | `--username` / `--password` | A realm account. |
| `GATEHOUSE_URL` | `--gatehouse-url` | Where gatehouse is; required whenever a username/password is given. |
| `CONVEYOR_INSECURE` | `--insecure` | Accept a self-signed certificate. |

The lowest-priority source is `~/.config/conveyor/config.toml` (or `$XDG_CONFIG_HOME/conveyor/config.toml`), which accepts the same `url`, `username`, `password`, `gatehouse_url`, and `insecure` keys. A missing file is fine; a malformed one is an error.

[Home](../README.md)
