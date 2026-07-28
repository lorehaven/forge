# Foreman

A local development estate, described in TOML rather than in a shell script.

A project drops a `foreman.toml` at its root naming its services, the containers
underneath them, and the tasks that have to run before either. Foreman starts
what you asked for plus what it cannot start without, keeps a pid per service so
`stop` can actually stop it, and gets out of the way.

```bash
foreman                    # start everything
foreman start conveyor     # start one service, and what it needs
foreman repl               # pick services interactively, then `up`
foreman stop [all]         # stop the services, or the containers too
foreman status             # what is up, and on which port
foreman logs sage          # follow one service's log
foreman db | reset         # install the schema catalog, or drop it and rebuild
foreman test [service]     # run the test suite
foreman run <task>         # run a task from the config
foreman list               # what this project defines
foreman env <service>      # the command line and environment one service gets
foreman init               # write a starter foreman.toml
```

`foreman init` writes a commented starter file. The rest of this is what is in
it.

## Where the config lives

Foreman looks for `foreman.toml` (or `.foreman.toml`) in the working directory
and every parent. The directory holding it is the **project root**, and every
relative path in the file is relative to that — services are launched with their
own working directory, so a path relative to anything else would not survive the
trip.

## Templates

Every string is a template. `${name}` is replaced from `[vars]`, plus:

| Available | Where |
| --- | --- |
| `${project_root}`, `${project_name}` | everywhere |
| `${name}`, `${package}`, `${port}`, `${base_path}` | inside a service |
| `${service}` | inside a task's `each_selected`, and in `[test].service_arg` |
| `${pids}` | inside a `[[warnings]]` message |

A name that is not defined is an error rather than an empty string: a blank
`DATABASE_URL` fails much further from its cause than a config error does. A
lone `$` is literal, and unknown *keys* are rejected too — a misspelled key is a
setting that silently does nothing.

## Vars

```toml
[vars]
database_url = "postgres://postgres:postgres@localhost:5432/postgres"
jwt_secret = { env_file = "docker/api/.env", key = "JWT_SECRET", default = "dev" }
```

The second form lifts a value out of a dotenv file the services already read, so
the config cannot quietly drift from what they expect. Vars cannot refer to one
another — one pass, no ordering to reason about, nothing that can loop.

## Containers

```toml
[[containers]]
name = "postgres"
image = "pgvector/pgvector:pg18"
container_name = "postgres"          # defaults to `name`
ports = ["5432:5432"]
env = { POSTGRES_USER = "postgres" }
args = []                            # extra `docker run` arguments
ready = ["pg_isready", "-U", "postgres"]   # run *inside* the container
ready_timeout_secs = 30
address = "localhost:5432"           # for the message only
```

Started before any service, stopped only by `foreman stop all`, and run with
`--rm`: a development database is meant to be disposable.

The readiness command runs inside the container because a published port is
listening long before anything is answering on it.

## Services

`[defaults]` holds what every service shares; a service's own value wins, and
`env` merges key by key rather than replacing.

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

The order services are listed in is the order they start, and `needs` is what a
service cannot start without. Selecting a service selects its dependencies too:
asking for one and getting a version of it that fails every request would be the
wrong kind of obedience.

Services are launched from their built binary rather than through `cargo run`,
because `stop` needs a real pid — killing a `cargo run` parent leaves the service
running and holding its port. Each one gets its own process group, so a Ctrl-C
aimed at foreman does not reach through the terminal and take down everything
that had already come up.

```toml
[build]
command = ["cargo", "build", "-q", "--package", "${package}"]
binary = "target/debug/${package}"
```

Either can be overridden per service with `build` and `binary`.

## Tasks

Work that happens around the services rather than in them. Schema installation is
the reason this exists: it has to run after the database is up and before the
first service connects.

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

`role` is one of `migrate` (runs before a start and on `foreman db`), `reset`
(runs on `foreman reset`), or the default `manual` (runs on `foreman run
<name>`).

`each_selected` is repeated once per selected service, in place of the literal
`${selection}` element of `command` — position matters when a trailing verb has
to stay last. With `selection = "subset-only"` it only expands when a strict
subset of the estate was asked for, so starting one service installs one
service's schemas, and starting it later re-runs the task with the wider
selection.

## After a start, and after a stop

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

Warnings are for processes that can outlive the service that spawned them. They
are reported after a stop rather than killed — some of them may not be yours.

## Tests

```toml
[test]
command = ["cargo", "run", "--package", "forge-bdd", "--"]
service_arg = ["--service", "${service}"]
stop_services = true
note = "the estate is down; `foreman` brings it back"
```

A bare service name becomes `service_arg`; anything starting with `-` is passed
straight through. Suites usually bind the same ports as the estate, so
`stop_services` takes it down first and says so, rather than failing with a
confusing bind error.

## What it needs on PATH

`docker` for the containers, `curl` for health checks, `pgrep` and `tail` for
`[[warnings]]` and `foreman logs`, and whatever your `build` and `command` lines
name.
