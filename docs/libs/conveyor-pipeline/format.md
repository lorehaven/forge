---
title: Pipeline file format
parent: conveyor-pipeline
grand_parent: Libraries
nav_order: 1
---

The file is `.conveyor.toml` at the repository root (`conveyor_pipeline::PIPELINE_FILE`). It is TOML. **Every table rejects unknown keys** (`deny_unknown_fields`): a misspelt key is a parse error. All checks run in `parse`.

## Layout

```toml
on = { push = ["master"], pull_request = ["*"], schedule = ["0 0 * * *"], timezone = "Europe/Warsaw" }

[[stage]]
name  = "build"
needs = ["check"]
when  = "event != 'schedule'"

[[stage.job]]
name    = "workspace"
image   = "registry/rust-builder:latest"
env     = { RUST_BACKTRACE = "1" }
secrets = ["ANVIL_APP_TOKEN"]
timeout = 3600
artifacts = ["lcov.info"]
steps   = [{ anvil = "build --all" }, "echo done"]

[[stage.job.service]]
name  = "postgres"
image = "pgvector/pgvector:pg18"
env   = { POSTGRES_PASSWORD = "postgres" }
ready = "pg_isready -h 127.0.0.1 -U postgres"
```

Top level keys: `on` (table, optional), `stage` (array of tables, at least one).

## `on`: triggers

| Key | Type | Meaning |
|---|---|---|
| `push` | array of string | glob patterns matched against the bare branch name of a push |
| `pull_request` | array of string | matched against the target branch of a pull request |
| `tag` | array of string | matched against the tag name of a push of `refs/tags/...` |
| `schedule` | array of string | cron expressions (see [Schedules](#schedules)) |
| `timezone` | string | IANA zone name the schedules are read in; default `UTC` (value trimmed) |

Semantics:

* **`on` omitted** gives the default `push = ["*"]`, `pull_request = []`, `tag = []`.
* **`on` present replaces the default outright.** An event it does not name does not trigger (`on = { pull_request = ["*"] }` does not build pushes).
* An empty or blank pattern in `push`/`pull_request`/`tag` is an error (`` `on.push` contains an empty pattern; use "*" to match every ref``). An empty list means never.
* Patterns: `*` matches any run of characters including none and including `/`; everything else is literal; no other metacharacters. `release/*` matches `release/1.2`. Patterns match the bare ref name (`master`, not `refs/heads/master`).
* `Triggers::allows(event, git_ref)`: event `manual` is always allowed. A `git_ref` starting `refs/tags/` is allowed only for event `push` and only if the tag matches `tag`. Otherwise `refs/heads/` is stripped and `push` / `pull_request` match their lists; any other event is false.
* `PipelineSpec::allows(event, git_ref)` adds: event `schedule` is allowed exactly when `on.schedule` is non-empty.

## `[[stage]]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string, required | | trimmed; blank is an error; unique in the pipeline |
| `needs` | array of string | `[]` | stages that must finish first |
| `when` | string | none (always runs) | condition ([language](#when-conditions)) |
| `job` | array of tables | | at least one |

* Stages are shown in declaration order. Execution order (`PipelineSpec::order`) is a topological order; ties are broken by declaration order, so it is deterministic.
* Graph errors: duplicate stage name, `needs` naming a non-stage, a stage needing itself, a cycle (`a -> b -> a`).
* A stage runs its jobs independently of each other.
* A stage is `Blocked` if any stage it `needs` did not run (itself excluded or blocked); `Excluded` if its own `when` is false.

## `[[stage.job]]`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | sole job: the stage's name; otherwise `job-<n>` (1-based position) | trimmed; blank given name is an error; unique within the stage |
| `when` | string | none | evaluated in addition to the stage's; a job runs only if both allow |
| `env` | table string to string | `{}` | extra environment for every step |
| `secrets` | array of string | `[]` | names of secrets to inject; blank name is an error. A secret not listed is not visible to the job |
| `timeout` | integer (u64) seconds | none | `0` is an error. When none, the deployment's `CONVEYOR_JOB_TIMEOUT_SECS` applies (service setting) |
| `image` | string | none | container image; honoured by the kubernetes executor, ignored by the native one |
| `artifacts` | array of string | `[]` | paths collected once the job succeeds |
| `service` | array of tables | `[]` | sidecar containers, below |
| `steps` | array | | at least one |
| `needs` | | | accepted only to be rejected: `job '<j>' in stage '<s>' declares `needs`; dependencies are between stages, so move it to the [[stage]]` |

Steps run in order; the first failure stops the job (executor behaviour, documented on the `Job` type).

## Steps

Each entry of `steps` is either

* a **string**: a `run` step (`"cargo build"`), or
* a **table with exactly one key**: `{ run = "..." }`, `{ anvil = "..." }`, `{ riveter = "..." }`, `{ warehouse = "..." }`. The value must be a non-blank string.

| Step | Becomes (`steps::argv`) | Parse-time check |
|---|---|---|
| `run` | `["sh", "-c", <command>, "conveyor-step"]` (shell does the parsing) | none |
| `anvil` | `["anvil", ...split(args)]` | first word in `anvil::COMMANDS`; for `docker` and `config` the second word too |
| `riveter` | `["riveter", ...split(args)]` | command word (after leading `--env <n>`, `-e <n>`, `--env=<n>`, `-e<n>`) in `riveter::COMMANDS`; `repl` refused |
| `warehouse` | `["warehouse-cli", ...split(args)]` | first word in `["admin","crates","docker","files"]` and its second word in that group's list |

Arguments of tool steps are split with `quench_core::shell::split` (shell-like quoting, no shell); unbalanced quotes are an error; an argument list of only blank arguments is an error (`''` alone is rejected, `--message ''` is fine).

Known command words:

* anvil: `audit build clean config deny docker format install lint list machete nextest release run semver-check test upgrade`; `anvil docker`: `build build-all push release release-all tag`; `anvil config`: `check`.
* riveter: `a apply d del delete df diff env h help i images install list ls pack prune publish pull r remote render schemas secrets validate`.
* warehouse-cli: `admin` (`gc`); `crates` (`login registry search unyank versions yank`); `docker` (`catalog login registry tags`); `files` (`bulk-delete bulk-download delete download ls mkdir preview registry rmdir storages upload`).

The lists are copies kept in this crate; they can lag behind the tools.

Step error text is prefixed `step <n> of job '<job>' in stage '<stage>': `; the reason is one of: `step is empty`; `a step names exactly one tool, found a, b`; `` `k` expects a command string``; `` `k` has an empty command``; ``unknown step kind `k` (known: run, anvil, riveter, warehouse)``; `expected a command string or a table like { run = "..." }, found <type>`; or a `StepError` text ([errors](errors.md)).

## `[[stage.job.service]]`: sidecar containers

Containers that run beside the steps for the life of the job (a database for tests). The kubernetes executor starts them in the job's pod (reach them on `localhost`) and holds the steps back until each is ready. The native executor refuses a job that declares any.

| Key | Type | Default | Rule |
|---|---|---|---|
| `name` | string, required | | trimmed; non-blank; DNS label (`a-z 0-9 -`, not starting/ending `-`, at most 63); not `checkout` or `steps`; unique within the job |
| `image` | string, required | | trimmed; non-blank |
| `env` | table | `{}` | no blank key. Written into the pod as is: not for secrets; not shared with the steps |
| `args` | array of string | `[]` | arguments for the image's entrypoint; empty keeps the image's own |
| `ready` | string | | command run inside the service container; ready when it exits 0. Non-blank |
| `port` | integer (u16) | | ready when this TCP port accepts connections; `0` is an error |
| `privileged` | bool | `false` | run this container privileged (a `docker:dind` daemon needs it). Only a request: the deployment decides (below). Must be a boolean; `"yes"` is a parse error |

**`privileged`** is the pipeline asking, and a repository's pipeline is not to be trusted with the node, so it takes two settings of the deployment (see the [Conveyor configuration](../../services/conveyor/configuration.md)) and a pull request never gets it: the executor must allow privileged containers (`CONVEYOR_K8S_ALLOW_PRIVILEGED`) and the repository must be named (`CONVEYOR_PRIVILEGED_REPOS`). A job that asks without being allowed fails before it starts, saying which setting is missing. Only that one container is privileged; the steps and the other containers stay as they were. A Docker daemon beside the steps looks like this:

```toml
[[stage.job.service]]
name = "dind"
image = "docker:dind"
privileged = true
env = { DOCKER_TLS_CERTDIR = "" }   # plain tcp on 2375, reached on localhost; the steps set DOCKER_HOST=tcp://127.0.0.1:2375
ready = "docker info"
```

Exactly one of `ready` and `port` must be set. Errors are `service <n> of job '<job>' in stage '<stage>': <reason>` with reasons: `has an empty name`; `` `n` is not a valid name: ...``; `` `n` is a name the executor uses for its own containers; pick another``; `` `n` is declared twice on this job``; `` `n` has no image``; `` `n` has an environment variable with no name``; `` `n` has an empty `ready` command``; `` `n` has port 0``; `` `n` sets both `ready` and `port`; a service is ready by one or the other``; `` `n` says nothing about when it is ready: set `ready = "<command>"` or `port = <n>`, or the steps would start before it is up``.

## `when` conditions

```text
expr := and ( "||" and )*
and  := cmp ( "&&" cmp )*
cmp  := variable ( "==" | "!=" ) string
```

* Variables: `branch`, `tag`, `event`, `sha`. Unknown identifiers are an error listing the known ones.
* Strings are single- or double-quoted, no escapes, may be empty.
* `&&` binds tighter than `||`. No parentheses, no negation, no other operators.
* Values at plan time (`EvalContext::new(event, git_ref, sha)`): `refs/tags/X` gives `tag = X`, `branch = ""`; otherwise `refs/heads/` is stripped (a bare ref is a branch) and `tag = ""`. `event` and `sha` are the strings given. Events in use: `push`, `pull_request`, `manual`, `schedule`.
* `tag != ''` means "only on a tag".
* Identifier characters: alphanumeric and `_`. Whitespace is ignored.

## Schedules

`on.schedule` entries are standard five-field cron expressions `minute hour day-of-month month day-of-week`, read in `on.timezone` (default `UTC`).

| Field | Range | Names |
|---|---|---|
| minute | 0-59 | |
| hour | 0-23 | |
| day-of-month | 1-31 | |
| month | 1-12 | `jan`..`dec` (case-insensitive) |
| day-of-week | 0-7 (0 and 7 are Sunday) | `sun`..`sat` |

Field syntax: `*`, a value, a list `1,15`, a range `1-5`, a step `*/10`, `0-30/5`, and `5/10` (a single value with a step runs from the value to the field maximum). Macros: `@yearly` and `@annually` (`0 0 1 1 *`), `@monthly` (`0 0 1 * *`), `@weekly` (`0 0 * * 0`), `@daily` and `@midnight` (`0 0 * * *`), `@hourly` (`0 * * * *`). Other `@...` is an error.

Day matching as in Vixie cron: when both day-of-month and day-of-week are restricted (do not start with `*`) a day matches if either does; when one is restricted, that one decides. A local time that does not exist (spring-forward) does not fire that day; one that happens twice fires at both instants. At parse time each entry must be valid and must fire at least once after a fixed reference instant (unix 1 700 000 000) within eight years (so `0 0 31 2 *` is an error); an unknown zone is an error. Parse errors are reported as `SpecError::Toml` with text such as `` `on.schedule` entry `X`: ...``, `` `on.timezone`: unknown time zone `Z` ``, `` `on.schedule` entry `X` never fires (does that date exist?)``.

## Minimal and full examples

Minimal valid pipeline (one stage, one job named after the stage):

```toml
[[stage]]
name = "build"
[[stage.job]]
steps = ["cargo build"]
```

The repository's own `.conveyor.toml` (`forge/.conveyor.toml`) is a full example: `check` (3 jobs), `build`, `test` (a job with a postgres sidecar service and a `lcov.info` artifact) and the nightly `security` stage guarded by `when = "event == 'schedule'"`.
