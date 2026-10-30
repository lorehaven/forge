---
title: Commands
parent: foreman
grand_parent: Command-line tools
nav_order: 1
---

Verified in a scratch directory with the starter config (`foreman init`): `--version`, `--help`, `init`, `list`, `env`, `status`, and the error paths. Unverified (not run: they start/stop processes, containers or databases): `start`, `stop`, `logs`, `repl`, `db`, `reset`, `test`, `run`, bare service name. Those are documented from `commands.rs`, `services.rs`, `tasks.rs`, `docker.rs`, `process.rs`.

Service names are validated before anything runs: `error  unknown service 'x' (known: api, web)` (for the bare form: `unknown service x (known: ...)`; several unknown names are reported together: `unknown services a, b (known: ...)`).

## foreman (no command), foreman \<service\>...

A bare list of words is a `start` of those services: `foreman conveyor` equals `foreman start conveyor`. With nothing, the whole estate starts. Unknown names are rejected first.

## start

```text
foreman start [SERVICES]...
```

1. Selection: the named services (all if none) plus, transitively, everything they `needs`, in the config file's order. If it is not the whole estate: `selection    api, web`. No services configured: `no services are configured in <path>`.
2. Creates `<root>/.run/logs/`.
3. Starts every `[[containers]]` entry (all containers, whatever the selection): `docker run --rm -d --name <container_name> -e K=V ... -p P ... <args> <image>`; if already running: `already running`; then waits for readiness (see [containers](configuration.md#containers)). Needs `docker` (`docker not found on PATH`).
4. Runs the `role = "migrate"` tasks (see [tasks](configuration.md#tasks)) with the selection.
5. For services with `cert_from`: symlinks the dev certificate files into the service's working directory if it has none (`linked the dev certificate from <dir>`; `no dev certificate; will serve plain HTTP` when the source lacks them too).
6. For each service in order: if its pid file points at a live process, `already running on <port>`; else run its build command in the project root (`building`), print `[services.env_when]` notes, check the binary exists (`<package> was not built to <path>`), spawn it detached in its own process group (stdin null, stdout and stderr to `.run/logs/<name>.log`, working directory = the service's `workdir`, environment as `foreman env` shows), write `.run/<name>.pid`, and poll `<scheme>://<host>:<port><base_path><health_path>` with `curl -sk` until it answers, up to `start_timeout_secs`:
   * answers: `<name>  <url> (pid N)`;
   * process died: `exited during startup` plus the last 5 log lines (pid file removed; counted as failed);
   * timeout: warning `started (pid N) but <health_path> did not answer - see <log>` (counted as started).
7. Prints the `[[notes]]` (skipping those whose `when_selected` service is not selected), the log directory and a list `name  url`, then `stop  foreman stop`.
8. If any service failed: `error  a, b did not start` and exit 1.

## stop

```text
foreman stop [SERVICES]...
foreman stop all
```

No names: every service (containers stay up). Names: only those, **without** expanding dependencies (stopping one service must not take down what authenticates against it). `all`: every service **and** every container (`docker stop <container>`).

For each service: run its `pre_stop` hooks (only if it is up), then SIGTERM its pid, wait up to `stop_timeout_secs` (default 5), SIGKILL if still alive; `stopped` or `not running`; the pid file is removed. Afterwards each `[[warnings]]` whose `pgrep -f` pattern matches is reported (`still running: 123 456` and the message with `${pids}`).

## status

```text
foreman status
```

For each container: `running on <address>` or `running` (ok) / `not running` (warn), from `docker ps -q -f name=^<container_name>$`; then each service: `<url> (pid N)` or `not running`, from pid files and signal 0.

```text
$ foreman status
postgres     running on localhost:5432
api          not running
web          not running
```

## logs

```text
foreman logs <SERVICE>
```

`tail -f .run/logs/<service>.log`; interrupting with Ctrl-C is the normal exit (returns 0). Error: `no log at <path>`.

## repl, pick

```text
foreman repl [SERVICES]...         alias: pick
```

The interactive [picker](repl.md), pre-selecting the named services.

## db

```text
foreman db
```

Starts the containers that `migrate` tasks list, then runs every `role = "migrate"` task with no selection (the whole catalog), without starting any service.

## reset

```text
foreman reset
```

Runs every `role = "reset"` task (development only). Without any: `no task with role = "reset" in <path>`.

## test

```text
foreman test [ARGS]...
```

Needs a `[test]` section (`no [test] section in <path>`). If `[test].stop_services` (default true) and any service is up, stops them first (`the estate is up (a, b); stopping it - the suite needs the same ports`), then waits 2 s. If the first argument does not start with `-` and `[test].service_arg` is non-empty it must be a service and is expanded through `service_arg` (with `${service}`/`${name}`); every remaining argument is passed through (hyphen values allowed). Runs `[test].command` + arguments in the project root with output on the terminal: `test  running every suite` or `test  <program> <args>`; then `suites passed` (exit 0) or `suites failed` (exit **1**); then the `note` if set.

## run

```text
foreman run <TASK> [SERVICES]...
```

Runs the named task (any role, normally `manual`) with the given services as its selection (dependencies included). Unknown: `unknown task 'x' (known: migrate, ...)`.

## list

```text
foreman list
```

Prints `config <path>`, then the containers (`name image`), services (`name url`, with `needs <...>` when set) and tasks (`name` and when it runs: `runs before a start, and on foreman db`, `runs on foreman reset`, `runs on foreman run`).

```text
$ foreman list
config       /tmp/.../foreman.toml

containers
  postgres     postgres:18-alpine

services
  api          http://localhost:8080/api
  web          http://localhost:8081/web  needs api

tasks
  migrate      runs before a start, and on `foreman db`
```

## env

```text
foreman env <SERVICE>
```

Shows what the service would be started with, merged from `[defaults].env`, the service's `env`, matching `env_when` blocks and minus `unset`. Prints `binary`, `workdir`, `build`, `health`, any `applied` notes, `unset NAME` lines, then `KEY=value` lines (values printed in clear text: do not paste the output of a real estate).

```text
$ foreman env web
binary       /tmp/.../target/debug/web-service
workdir      /tmp/.../services/web/service
build        cargo build -q --package web-service
health       http://localhost:8081/web/health

  DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
  RUST_LOG=info
  SERVER_ADDR=0.0.0.0:8081
  API_URL=http://localhost:8080/api
```

## init

```text
foreman init [--force]
```

Writes the commented starter `foreman.toml` (the file `templates/foreman.toml`, compiled in) into the **current directory**. Without `--force` an existing file is an error (`<path> already exists (--force overwrites it)`). Does not need an existing config.
