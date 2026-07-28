# Conveyor CLI 🏗️

Drives [conveyor](../../docker/conveyor-service/README.md), the Forge CI/CD
service, from a terminal.

```bash
export CONVEYOR_URL=https://localhost:9443/conveyor
export CONVEYOR_USERNAME=admin
export CONVEYOR_PASSWORD=…

conveyor repo add lorehaven/forge https://github.com/lorehaven/forge.git
conveyor run lorehaven/forge --wait
conveyor logs <run-id> --follow
```

Repositories can be named `owner/name` anywhere an id is accepted, because that
is what you have to hand.

## Commands

| | |
|---|---|
| `repo add <owner/name> <clone-url>` | Register a repository. |
| `repo list` · `repo enable` · `repo disable` · `repo remove` | Manage them. |
| `run <repo> [--ref] [--sha] [--wait]` | Start a run. |
| `runs [--repo] [--limit]` | Recent runs. |
| `show <run-id>` | One run: its jobs, why any were skipped, its artifacts. |
| `logs <run-id\|job-id> [--follow]` | Output, stored or live. |
| `cancel <run-id>` | Ask a run to stop. |
| `secret set/list/remove [--repo]` | Manage secrets. Values are never read back. |
| `validate [path]` | Check a `.conveyor.toml`. |

## `--wait`

Polls until the run rests and **exits non-zero if it failed**, so this is usable
as the last line of a script:

```bash
conveyor run lorehaven/forge --wait || exit 1
```

## `validate`

The only command that needs no running service. It links conveyor's own parser
in, so what it accepts is exactly what a run will accept rather than a second
implementation that agrees most of the time:

```console
$ conveyor validate
valid   2 stage(s), 2 job(s)
  build
    build (4 step(s))
  deploy  needs build
    deploy (1 step(s))
```

## Secrets

`secret set NAME` with no value reads it from stdin, which keeps it out of shell
history and out of the process list where anyone on the machine can see it:

```bash
printf '%s' "$TOKEN" | conveyor secret set DEPLOY_TOKEN --repo lorehaven/forge
```

## Configuration

| | |
|---|---|
| `CONVEYOR_URL` | Where conveyor is. Also `--url`. |
| `CONVEYOR_USERNAME` / `CONVEYOR_PASSWORD` | A realm account. Also `--username` / `--password`. |
| `CONVEYOR_INSECURE` | Accept a self-signed certificate, as the estate's internal ones are. Also `--insecure`. |
