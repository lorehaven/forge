# Foreman

Foreman runs a project's local development estate from a single `foreman.toml` file instead of a pile of shell scripts. A project describes its services, the Docker containers they sit on, and the tasks that must run before either (like installing a schema catalog); Foreman figures out what has to start for a given request, starts it in dependency order, tracks a pid per service so `stop` can actually stop it, and gets out of the way otherwise. This is the tool the Lorehaven estate uses to bring up its local services for development.

## Features

- **Dependency-aware start/stop**: selecting a service also selects (and starts) everything it `needs`; services start in the order listed in the config.
- **Real process control**: services are launched from their built binary (not `cargo run`) so `stop` can signal a real pid, each in its own process group so Ctrl-C on foreman doesn't take down services that were already up.
- **Docker containers**: containers are started before any service, run with `--rm`, and only torn down by `foreman stop all`; readiness is checked by running a command inside the container.
- **Tasks**: shell/build work tied to a role — `migrate` (before every start and on `foreman db`), `reset` (on `foreman reset`), or `manual` (on `foreman run <name>`) — with optional per-service expansion via `each_selected`.
- **Templating**: every string in the config is a template (`${name}` from `[vars]`, plus context like `${project_root}`, `${package}`, `${port}`, `${service}`, `${pids}`); unknown keys and undefined names are hard errors rather than silent no-ops.
- **Interactive picker**: `foreman repl` (alias `pick`) lets you choose services interactively before starting them.
- **Post-start notes and orphan warnings**: `[[notes]]` print conditionally after a successful start; `[[warnings]]` report (not kill) processes like stray `vllm serve` runs that can outlive their service.
- **Config discovery**: walks up from the working directory looking for `foreman.toml` or `.foreman.toml`; the directory it's found in becomes the project root that every relative path in the file is resolved against.

## Requirements

- `docker` on `PATH` for containers.
- `curl` on `PATH` for health checks.
- `pgrep` and `tail` on `PATH` for `[[warnings]]` and `foreman logs`.
- Whatever each service's `build`/`command` lines invoke (typically `cargo`).

## Usage

```bash
foreman                    # start everything
foreman start conveyor     # start one service, and what it needs
foreman repl                # pick services interactively, then start them
foreman stop [all]         # stop the services, or the containers too
foreman status              # what is up, and on which port
foreman logs sage           # follow one service's log
foreman db                  # install the schema catalog
foreman reset                # drop what the schema tooling owns and reinstall it (dev only)
foreman test [service]      # run the test suite, or one suite
foreman run <task> [service...]  # run a named task from the config
foreman list                # services, containers and tasks this project defines
foreman env <service>       # the command line and environment one service would get
foreman init [--force]      # write a starter foreman.toml
```

A bare service name with no subcommand is a start of that service: `foreman conveyor` behaves like `foreman start conveyor`.

`foreman init` writes a fully commented starter `foreman.toml`, which is the fastest way to see every available key in context.

## Where the config lives

Foreman looks for `foreman.toml` (or `.foreman.toml`) in the working directory and every parent directory above it. The directory holding it becomes the **project root**, and every relative path in the file is resolved against that root — services are launched with their own working directory, so a path relative to anything else would not survive the trip.

## Configuration

`foreman.toml` has these top-level sections: `[project]`, `[vars]`, `[build]`, `[[containers]]`, `[tasks.<name>]`, `[defaults]`, `[[services]]`, `[[notes]]`, `[[warnings]]`, `[test]`. Unknown keys anywhere in the file are a hard error rather than a silent no-op — a misspelled key is a setting that silently does nothing, which is a bug that costs an afternoon to find.

### Templates

Every string value in the config is a template. `${name}` is replaced from `[vars]`, plus context-specific names:

| Available | Where |
| --- | --- |
| `${project_root}`, `${project_name}` | everywhere |
| `${name}`, `${package}`, `${port}`, `${base_path}` | inside a service |
| `${service}` | inside a task's `each_selected`, and in `[test].service_arg` |
| `${pids}` | inside a `[[warnings]]` message |

A name that is not defined is an error rather than an empty string — a blank `DATABASE_URL` fails much further from its cause than a config error does. A lone `$` is literal, and unknown *keys* are rejected too.

### `[vars]`

```toml
[vars]
database_url = "postgres://postgres:postgres@localhost:5432/postgres"
jwt_secret = { env_file = "docker/api/.env", key = "JWT_SECRET", default = "dev" }
```

An entry is either a literal string, or `{ env_file, key, default }`, which lifts a value out of a dotenv file a service already reads so the config can't drift from what the services expect (`env_file` is relative to the project root; `default` is used if the key is missing). Vars cannot refer to one another — one pass, no ordering to reason about, nothing that can loop.

### `[[containers]]`

```toml
[[containers]]
name = "postgres"
image = "pgvector/pgvector:pg18"
container_name = "postgres"          # defaults to `name`
ports = ["5432:5432"]
env = { POSTGRES_USER = "postgres" }
args = []                            # extra `docker run` arguments, inserted before the image
ready = ["pg_isready", "-U", "postgres"]   # run *inside* the container; empty means don't wait
ready_timeout_secs = 30
address = "localhost:5432"           # for the message only
```

Containers start before any service, are stopped only by `foreman stop all`, and run with `--rm` — a development database is meant to be disposable. The readiness command runs inside the container because a published port is listening long before anything is actually answering on it.

### `[build]`

```toml
[build]
command = ["cargo", "build", "-q", "--package", "${package}"]
binary = "target/debug/${package}"
```

The default build command and binary path for every service, with `${package}` substituted. Either can be overridden per service with the service's own `build` and `binary` keys. Services are launched from their built binary rather than through `cargo run`, because `stop` needs a real pid — killing a `cargo run` parent leaves the service running and holding its port. Each service gets its own process group, so a Ctrl-C aimed at foreman does not reach through the terminal and take down services that were already up.

### `[defaults]` and `[[services]]`

```toml
[defaults]
scheme = "https"
host = "localhost"
health_path = "/health"
start_timeout_secs = 30
stop_timeout_secs = 4
workdir = "docker/${package}"
cert_files = ["cert.pem", "key.pem"]
env = { DATABASE_URL = "${database_url}", SERVER_ADDR = "0.0.0.0:${port}" }

[[services]]
name = "sage"
package = "sage-service"
port = 8443
base_path = "/sage"
needs = ["gatehouse", "switchboard"]
unset = ["GATEHOUSE_URL"]            # drop a shared default, or an inherited name
cert_from = "docker/warehouse-service"   # borrow a dev certificate
env = { SAGE_MODE = "local" }

# Applied only when that variable is set in foreman's own environment.
[[services.env_when]]
env_set = "SKIP_MODELS"
note = "SKIP_MODELS set - not launching default models"
env = { SAGE_DEFAULT_MODELS = "[]" }

# Run before the service is signalled, and only when it is up. `sh -c`, so a
# pipeline or a loop is fine - which is what talking to an API usually takes.
[[services.pre_stop]]
description = "stopping the model instances"
shell = "curl -sk -X DELETE https://localhost:8443/sage/api/models"
timeout_secs = 60
settle_secs = 3
```

`[defaults]` holds `scheme`, `host`, `health_path`, `start_timeout_secs`, `stop_timeout_secs`, `workdir`, `env`, and `cert_files` — settings shared by every service. A service's own value wins for every field except `env`, which is merged key by key rather than replaced, so a service adds to the shared environment instead of restating it.

A `[[services]]` entry's fields:

| Field | Purpose |
| --- | --- |
| `name`, `package`, `port` | required; `package` is the cargo package, and by default the name of both the binary and the working directory |
| `base_path` | URL path prefix (default empty) |
| `needs` | services this one cannot start without — selecting it selects these too, and the order services are listed in the file is the order they start |
| `scheme`, `host`, `health_path`, `start_timeout_secs`, `stop_timeout_secs`, `workdir` | override the matching `[defaults]` key |
| `binary`, `build` | override `[build].binary` / `[build].command` for this service |
| `env` | merged with `[defaults].env` |
| `unset` | names dropped from the environment, including anything inherited from the shell — use it to keep a shared default off one service |
| `env_when` | list of `{ env_set, env, note }` blocks: `env` is applied, and `note` printed, only when `env_set` is set in foreman's own environment |
| `cert_from` | borrow another directory's dev certificate when this service has none, relative to the project root |
| `cert_files` | overrides `[defaults].cert_files` |
| `pre_stop` | list of hooks (`description`, `shell`, `timeout_secs` default 30, `settle_secs`) run before the service is signalled, and only when it is up — for children the service owns rather than foreman does |

### `[tasks.<name>]`

Work that happens around the services rather than in them. Schema installation is the reason this exists: it has to run after the database is up and before the first service connects.

```toml
[tasks.migrate]
role = "migrate"          # before every start, and on `foreman db`
description = "applying the migration catalog"
containers = ["postgres"] # brought up first
build = ["cargo", "build", "-q", "--package", "migrator"]
command = ["${project_root}/target/debug/migrator", "${selection}", "apply"]
env = { DATABASE_URL = "${database_url}" }
each_selected = ["--install", "${service}"]
selection = "subset-only"     # or "always", "never"
stop_services = false
warn = "this drops everything"    # printed before the command runs
done = "database ready"           # printed on success
```

| Field | Purpose |
| --- | --- |
| `role` | `migrate` (runs before every start and on `foreman db`), `reset` (runs on `foreman reset`), or the default `manual` (runs only on `foreman run <name>`) |
| `description` | optional label shown while it runs |
| `containers` | containers that must be up first |
| `build` | run first; aborts the task if it fails (typically a cargo build) |
| `command` | required; the command to run |
| `env` | extra environment for the command |
| `workdir` | relative to the project root; defaults to the project root itself |
| `each_selected` | repeated once per selected service, in place of the literal `${selection}` element of `command` (appended if `command` has no such element) — position matters when a trailing verb has to stay last |
| `selection` | when to expand `each_selected`: `subset-only` (default; only when a strict subset of the estate was asked for — starting everything means the task's own configuration already covers it), `always`, or `never` |
| `stop_services` | take the services down first (default `false`); what drops schemas cannot run under them |
| `warn` | printed as a warning before the command runs |
| `done` | printed on success |

### `[[notes]]` and `[[warnings]]`

```toml
[[notes]]
tone = "ok"                   # info | ok | warn | error
label = "ready"
message = "sign in at https://localhost:5443/gatehouse/ui/login"
when_selected = "gatehouse"   # only when that service is part of what started

[[warnings]]
name = "vllm"
pgrep = "vllm serve"
message = "these hold the GPU; kill them with: kill ${pids}"
```

`[[notes]]` print after a successful start; `tone` defaults to `info`, and `when_selected` restricts a note to only print when the named service was part of what started. `[[warnings]]` run `pgrep -f` with the given pattern after a stop and report (rather than kill) any matching pids via `${pids}` in `message` — they exist for processes that can outlive the service that spawned them, and are not killed because some of them may not be foreman's to kill.

## Testing

`[test]` configures `foreman test`:

```toml
[test]
command = ["cargo", "run", "--package", "forge-bdd", "--"]
service_arg = ["--service", "${service}"]
stop_services = true
note = "the estate is down; `foreman` brings it back"
```

`command` is required (typically a cargo-run of a BDD/test package); `service_arg` is the template a bare service name on the `foreman test` command line expands into (anything already starting with `-` is passed straight through); `stop_services` defaults to `true` since test suites usually bind the same ports as the estate, so `foreman test` takes the estate down first and prints `note` explaining that `foreman` brings it back.

[Home](../README.md)
