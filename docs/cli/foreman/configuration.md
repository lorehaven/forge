---
title: Configuration
parent: foreman
grand_parent: Command-line tools
nav_order: 3
---

Everything is in `foreman.toml`, searched from the working directory upwards (names tried in each directory in this order: `foreman.toml`, `.foreman.toml`). Source: `src/config.rs`, `estate.rs`, `vars.rs`; the starter file is `forge/cli/foreman/templates/foreman.toml` (written by `foreman init`). All tables use `deny_unknown_fields`: an unknown key is an error, for example (verified):

```text
error        parsing /tmp/.../foreman.toml
               unknown field `bad`, expected one of `project`, `vars`, `build`, `containers`, `tasks`, `defaults`, `services`, `notes`, `warnings`, `test`
```

Validation (`config.validate`): duplicate service name; `needs` naming an unknown service; a task with an empty `command`; a task naming an unknown container; a note whose `when_selected` is not a service.

## Templates

Every string value (except TOML keys) is a template. `${name}` is replaced from `[vars]` and context; an unknown name is an error (`unknown variable ${x} in "..."`), an unterminated `${` too; a lone `$` is literal. There is no escape for a literal `${`.

| Variable | Available |
|---|---|
| each `[vars]` key | everywhere |
| `project_root` (absolute path of the root), `project_name` (`[project].name`) | everywhere; a `[vars]` entry of the same name wins |
| `name`, `package`, `port`, `base_path`, `service` | inside a `[[services]]` block's templates (`service` equals `name`) |
| `service`, `name` | in a task's `each_selected` and in `[test].service_arg` (per service) and in `pre_stop` hooks |
| `selection` | only as the literal element `"${selection}"` of a task's `command` (see tasks) |
| `pids` | in a `[[warnings]]` message |

## [project]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | `"project"` | Shown by the picker banner and `${project_name}`. |
| `description` | string | `""` | Informational. |
| `run_dir` | string | `".run"` | Where pid files (`<run_dir>/<service>.pid`) and logs (`<run_dir>/logs/<service>.log`) live, relative to the root. |

## [vars]

`name = "literal"` or `name = { env_file = "path/.env", key = "KEY", default = "x" }`. The second form reads `KEY=` from a dotenv file (path relative to the root; a missing file or key gives `default`, default `""`); the value has surrounding whitespace and one layer of `"`/`'` trimmed. This lifts a shared secret out of a service's own `.env` so the config cannot drift from it.

## [build]

| Key | Default | Meaning |
|---|---|---|
| `command` | `["cargo","build","-q","--package","${package}"]` | Build command, run in the project root before each start. |
| `binary` | `"target/debug/${package}"` | Built binary, relative to the root. |

## [[containers]]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required | Name used by tasks and in messages. |
| `image` | string | required | Image. |
| `container_name` | string | `name` | Docker container name. |
| `ports` | array of string | `[]` | `-p` values (templated). |
| `env` | table | `{}` | `-e K=V` (templated). |
| `args` | array of string | `[]` | Extra `docker run` arguments before the image (templated). |
| `ready` | array of string | `[]` | Command run with `docker exec <container> ...` every 500 ms until it succeeds; empty means no wait. |
| `ready_timeout_secs` | integer | `30` | Give up: `<name> did not become ready`. |
| `address` | string | none | Only for the `ready on <address>` / status message. |

Containers run with `--rm -d`, are started before services and tasks, and are stopped only by `foreman stop all`.

## tasks

`[tasks.<name>]` (a table keyed by name; listed alphabetically).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `role` | `"migrate"` / `"reset"` / `"manual"` | `"manual"` | `migrate`: before every `start` and on `foreman db`; `reset`: on `foreman reset`; `manual`: on `foreman run <name>`. |
| `description` | string | none | Printed as info before the run (unless a selection is applied). |
| `containers` | array | `[]` | Containers started first (must exist). |
| `build` | array | `[]` | Run first (in the root); failure aborts: `task 'x' build step failed`. |
| `command` | array | required, non-empty | Program and arguments. The element `"${selection}"` is replaced by the per-service arguments; if absent they are appended. |
| `env` | table | `{}` | Extra environment (templated). |
| `workdir` | string | root | Relative to the root. |
| `each_selected` | array | `[]` | Arguments produced once per selected service (`${service}`/`${name}`). |
| `selection` | `"subset-only"` / `"always"` / `"never"` | `"subset-only"` | When `each_selected` applies: only when a strict subset of the estate is selected / whenever non-empty / never. |
| `stop_services` | bool | `false` | Stop all services first. |
| `warn` | string | none | Printed as a warning before the command. |
| `done` | string | none | Printed as ok on success. |

When a selection applies the task prints `<task>  applying a, b`. Failure: `task 'x' failed`.

## [defaults]

Settings every service shares; a service's own value wins, except `env` which merges key by key (service wins per key).

| Key | Built-in fallback | Meaning |
|---|---|---|
| `scheme` | `"https"` | URL scheme (the starter file sets `"http"`). |
| `host` | `"localhost"` | |
| `health_path` | `"/health"` | Appended to `scheme://host:port base_path`. |
| `start_timeout_secs` | `30` | Health wait. |
| `stop_timeout_secs` | `5` | Wait after SIGTERM before SIGKILL. |
| `workdir` | `"${package}"` | Service working directory, relative to the root. |
| `env` | `{}` | Shared environment. |
| `cert_files` | `["cert.pem","key.pem"]` | Files linked by `cert_from`. |

## [[services]]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | required, unique | |
| `package` | string | required | Cargo package (default binary name). |
| `port` | integer | required | |
| `base_path` | string | `""` | URL prefix. |
| `needs` | array | `[]` | Services selected with this one. The file order is the start order. |
| `scheme`, `host`, `health_path`, `start_timeout_secs`, `stop_timeout_secs`, `workdir` | | defaults | Override `[defaults]`. |
| `binary`, `build` | | `[build]` | Override. |
| `env` | table | `{}` | Merged over `[defaults].env`. |
| `unset` | array | `[]` | Names removed from the final environment, including inherited ones. |
| `env_when` | array of `{ env_set, env, note }` | `[]` | `[[services.env_when]]`: applied when the variable `env_set` is set in foreman's own environment; `note` is printed. |
| `cert_from` | string | none | Directory to borrow dev certificates from. |
| `cert_files` | array | defaults | |
| `pre_stop` | array of `{ shell, description, timeout_secs, settle_secs }` | `[]` | `[[services.pre_stop]]`: `sh -c` hooks run before signalling (only if up); `timeout_secs` default 30; `settle_secs` default 0. A failing hook only warns. |

## [[notes]]

`tone` (`info` default, `ok`, `warn`, `error`), `label`, `message` (templated), optional `when_selected` (a service). Printed after a successful start.

## [[warnings]]

`name`, `pgrep` (passed to `pgrep -f`), `message` (with `${pids}`). Reported after `stop`, never killed.

## [test]

| Key | Default | Meaning |
|---|---|---|
| `command` | required | Program and arguments. |
| `service_arg` | `[]` | Arguments added when a service name is given (`["--package","${service}-tests"]`). |
| `stop_services` | `true` | Stop the estate before the suite. |
| `note` | none | Printed after the run. |

## Runtime files

`<root>/<run_dir>/<service>.pid` (the pid as text; stale pids are detected by signal 0 and ignored), `<root>/<run_dir>/logs/<service>.log` (truncated at each start). Add `.run/` to `.gitignore`.
