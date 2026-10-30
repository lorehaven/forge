---
title: Plans and operations
parent: Gantry
grand_parent: Services
nav_order: 8
---

The mechanism: a **plan** is data (a closed set of steps), an **operation** is a plan recorded for running, the **reconciler** makes operations real by starting a **runner**, and the runner executes the steps with `riveter` and `kubectl`. Sources: `src/domain/{steps,planner,operation,service,reconciler,executor,runner,commands,cluster}.rs`, `src/bin/gantry_runner.rs`.

```text
API / UI --> gantry-service --creates--> runner Job --> riveter | kubectl
                |   ^                         |
                |   +---- status and log -----+
                +--> Postgres (operations, plans, inventory, deployment states)
                +--> registry (what is published)
```

## Steps

A plan is `{steps, title, summary, effects, touches, deployments}`. The service can only ask the runner to do what a step variant says: there is no way to express a free command, and every value that reaches a command line is validated and passed as its own argument, never through a shell. JSON form: `{"step": "<name>", ...}`.

| `step` | Fields (optional in brackets) | Command(s) run by the runner |
|---|---|---|
| `pull` | `package`, `version` | `riveter pull <package>@<version> --out <packages dir>`; with a source directory (`GANTRY_SOURCE_PACKAGES_DIR`) `cp <dir>/<package>-<version>.rivet <packages dir>` instead |
| `check` | `package`, `version`, [`values_secret`] | `riveter install <packages dir>/<package>-<version>.rivet --scope all [--env-file <values dir>/<secret>/env] --inventory --dry-run` (riveter's client-side dry run: renders everything, reaches no cluster; never "mutating") |
| `install` | `package`, `version`, `namespace`, [`sets`], [`replicas`], [`values_secret`], [`timeout_secs`], [`targets`], [`except`], [`no_wait`] | `riveter install <file> --scope all [--env-file ...] --inventory [--set K=V]... [--replicas kind/name=N]... [--timeout N] [--except kind/name]... [--no-wait] [--dry-run] [targets...]` (`--dry-run` only in dry-run mode) |
| `scale` | `namespace`, `kind` (`deployment`, `statefulset`, `daemonset`), `name`, `replicas` | `kubectl -n <ns> scale <kind>/<name> --replicas=<n>` |
| `delete` | `resource` (`kind[.group]/name`), [`namespace`] | `kubectl delete <resource> --ignore-not-found --timeout=120s [-n <ns>]` |
| `apply_yaml` | `yaml`, [`namespace`] | the runner first writes the YAML to `<packages dir>/edit-<12 hex of SHA-256>.yaml`, then `kubectl apply -f <that file>` |
| `restart_users` | `namespace`, `kind` (`configmap`, `secret`), `name`, `timeout_secs` | `kubectl -n <ns> get deployments,statefulsets,daemonsets -o json`; for each workload whose pod template (containers and init containers: `env[].valueFrom.<kind>KeyRef`, `envFrom[].<kind>Ref`; volumes) names the object: `kubectl -n <ns> rollout restart <kind>/<name>` then `rollout status --timeout=<n>s`. Says `nothing in <ns> uses <kind>/<name>` when none |
| `delete_pods` | `selector`, [`namespaces`] | `kubectl delete pods -l <selector> --wait=false`, once per namespace of the step, else per namespace of the allow-list, else with `-A` |
| `wait_gone` | `selector`, [`namespaces`], `timeout_secs` | polls `kubectl get pods -l <selector> -o name` every 2 s until none is listed (`kubectl wait --for=delete` errors when nothing matches, and nothing matching is the goal); fails with `still there after <n>s: <pods>` |
| `rollout` | `namespace`, `kind`, `name`, `timeout_secs`, [`rollback_on_failure`] | `kubectl -n <ns> rollout status <kind>/<name> --timeout=<n>s`; on failure with `rollback_on_failure`: `kubectl -n <ns> rollout undo <kind>/<name>` |

`effects` (`{"effect": "set_desired", "deployment": ..., "desired": "running"|"stopped"}`) are not steps: the reconciler applies them after the operation **succeeds**. `touches` lists the packages the plan affects (confirming needs a grant on each); `deployments` lists the deployments a start, stop or swap involves (for the staleness check).

### Validation (`Plan::validate`)

Run by `submit` before an operation is recorded (and again by the runner before it executes anything, so a tampered Job spec cannot reach outside the allow-list). A failure is `step <n> (<step in words>): <reason>`.

* At least one step.
* Names (package, namespace, kubernetes name, values secret): DNS-1123 style: lowercase letters, digits, `-`, `.`, at most 253 characters, starting and ending alphanumeric; the reason names what it was (`'x' is not a valid namespace (...)`).
* Version: starts with a digit, at most 128 characters of letters, digits, `.`, `+`, `-`.
* `sets` keys: upper-case variable names (`A-Z`, `0-9`, `_`, not starting with a digit).
* `replicas` keys: `kind/name` with a workload kind. `targets` and `except`, and a `delete` resource: `kind[.group]/name` where kind and group are alphanumeric, `.`, `-`, name alphanumeric, `.`, `-`, `:`, `_`.
* `scale` and `rollout` kinds are workload kinds; `restart_users` kind is `configmap` or `secret`.
* Selectors: at most 512 characters from letters, digits and `= ! , . / _ - ( )` and space; not starting with `-`.
* `apply_yaml`: at most 128 KiB; parses; has `kind` and `metadata.name`; not a `Secret`; its `metadata.namespace` equals the step's namespace exactly (so it cannot edit one namespace through another); without a namespace only when there is no allow-list.
* The allow-list: every namespace a step names must be in `GANTRY_NAMESPACES` when that is set; a `delete` or `apply_yaml` with no namespace is refused while it is set.
* In `submit`, additionally: a non-blank `title`, and no step may stop a protected workload (see [Deployments](deployments.md#protected-workloads)).

## Operations

One row per requested change, written **before** anything is done. Fields: `id`, `kind` (`install`, `upgrade`, `reinstall`, `downgrade`, `stop`, `start`, `swap` for plans; `delete`, `apply`, `edit`, `refresh` for direct actions; `custom` for a raw plan), `title`, `scope` (always `cluster`), `state`, `plan`, `result`, `runner_job`, `requested_by`, `cancel_requested`, `error`, `log`, `created_at`, `started_at`, `finished_at`.

### States

| State | Meaning |
|---|---|
| `queued` | recorded, waiting its turn |
| `running` | claimed; a runner exists or is being started |
| `succeeded` | the runner exited 0 and reported `ok` (or nothing) |
| `failed` | a step failed, the runner could not be started, or the Job failed |
| `rolled_back` | a step with an undo failed and the undo succeeded (only the guarded `rollout` has one) |
| `cancelled` | cancelled before it started, or stopped while running |

| From | To | By |
|---|---|---|
| (new) | `queued` | `submit` |
| `queued` | `running` | the store's `claim_next` (oldest queued first), at most one `running` per scope, enforced by the partial unique index `operations_one_running_per_scope_idx`; a racing claim gets a unique violation and returns nothing |
| `queued` | `cancelled` | `request_cancel` (immediate, `finished_at` set) |
| `running` | `cancelled` | `request_cancel` flags `cancel_requested`; the reconciler stops the runner and records `cancelled` with the error `cancelled; steps already run were not undone` |
| `running` | `succeeded` / `failed` / `rolled_back` | the reconciler reads the runner's result (below) |
| `queued` or `running` | `failed` | `could not start the runner: <error>` when starting fails |

The state column has a `CHECK` constraint on the six values. Only the reconciler writes a terminal state (`finish`); the SQL does not guard against writing over one.

### The reconciler

`reconciler::run` ticks forever: when woken by a submit or cancel, and every 3 seconds regardless. A tick:

1. For every `running` operation (`settle`):
   * `cancel_requested`: read the log, stop the runner, finish `cancelled`.
   * Ask the executor for the runner's status. If that fails (cluster unreachable) the operation is **left running**: an unreachable cluster says nothing about the operation.
   * `Active`: leave it (adoption: a restarted service picks up the runner a previous one started).
   * `Absent`: the service died between marking it `running` and creating the runner; start it again (steps are idempotent).
   * `Succeeded` or `Failed(message)`: read the runner's log and the outcome (below); on `succeeded`, record the inventory (if the plan has an `install` or `check` step and the log has a `riveter-inventory:` line) and apply the plan's `effects`; finish with the state, the error, the last 64 KiB of the log (prefixed `... (earlier output dropped)` when cut) and `result = {"runner": "<job name>"}`; then clean the runner up.
2. Claim queued operations one at a time (at most 16 per tick) and start each runner. The store refuses while a scope is busy, so the loop ends by itself.

The runner's name is derived from the operation id (`gantry-op-` + the first 12 alphanumeric characters, lowercased), so a restarted service finds the runner a previous incarnation started.

### Reading the outcome

The runner's last line is `GANTRY-RESULT: ok`, `GANTRY-RESULT: failed <reason>` or `GANTRY-RESULT: rolled_back <reason>` (the last such line of the log counts). A Job only reports success or failure; this line is how `rolled_back` is told from `failed`.

| Runner status | Marker | Operation state |
|---|---|---|
| any | `rolled_back <reason>` | `rolled_back`, error = reason |
| succeeded | `ok` or none | `succeeded` |
| any | `failed <reason>` | `failed`, error = reason |
| failed | none | `failed`, error = the Job's failure message |

## The runner program

`gantry-runner` (ships in the runner image) reads the plan from `GANTRY_PLAN`, validates it again, and runs each step: it prints `== step <i>/<n>: <step in words>` then, per command, `$ <command>`; stops at the first failure with `!! step <i> (<step>): `<command>` exited with <code>` (or `could not start`); runs the step's undo if it has one; and prints the result line. Exit code 0 only for `ok`. It reads `GANTRY_VALUES_DIR` (default `/values`), `GANTRY_PACKAGES_DIR` (default `/work/packages`), `GANTRY_NAMESPACES` (comma list), `GANTRY_DRY_RUN` (bool) and `GANTRY_SOURCE_PACKAGES_DIR`. If a step names a `values_secret` whose `<values dir>/<secret>/env` file does not exist it prints `(no values in <secret>: using the package's own defaults)` and runs the step without it. In dry-run mode every mutating command is printed and followed by `(dry run: not executed)`.

Before anything changes, the `check` step proves every `${VAR}` of the package is supplied (and that a package with encrypted values has a key), with the cluster untouched; a missing variable fails the operation naming it. Package values come from the package itself (`values.yaml`, then `secrets.yaml` opened with the age key at `RIVETER_AGE_KEY_FILE` = `/keys/age.key`).

### Runner implementations (`GANTRY_RUNNER`)

| Mode | What runs | Survives a service restart |
|---|---|---|
| `job` | a Kubernetes Job (below) | yes, the only mode that does when the service replaces itself |
| `local` | a child process: `sh -c '"$GANTRY_RUNNER_BIN" > $GANTRY_LOG_FILE 2>&1; echo $? > $GANTRY_EXIT_FILE'` in its own process group; files in `GANTRY_STATE_DIR`: `<name>.pid` (written by the service after the spawn), `<name>.log`, `<name>.exit` | yes, by the files; a pid without `/proc/<pid>` and no exit file is `the runner process disappeared without finishing` |
| `dry-run` | the same child process with `GANTRY_DRY_RUN=1` | as `local` |
| `simulate` | in-process against the fake cluster; no `riveter`, no `kubectl` (see [Operations](operations.md#the-simulated-cluster)) | in-memory only |

The local runner gets `GANTRY_PLAN`, `GANTRY_RUNNER_BIN`, `GANTRY_LOG_FILE`, `GANTRY_EXIT_FILE`, `GANTRY_VALUES_DIR`, `GANTRY_PACKAGES_DIR` (`<state dir>/<name>-packages`), `GANTRY_NAMESPACES`, `GANTRY_DRY_RUN` (`1` or `0`) and `GANTRY_SOURCE_PACKAGES_DIR`, on top of the service's own environment; stopping sends `TERM` to the process group; cleanup keeps the files for the developer.

### The Job

Created in `GANTRY_NAMESPACE` with the name above. `create` of an existing name (conflict) counts as success, which makes starting idempotent.

* labels `app.kubernetes.io/managed-by=gantry`, `app.kubernetes.io/name=gantry-runner`, `gantry.forge/operation=<operation id>` (also on the pod);
* `backoffLimit: 0`, `activeDeadlineSeconds` = `GANTRY_JOB_DEADLINE_SECS`, `ttlSecondsAfterFinished: 86400`, pod `restartPolicy: Never`, `serviceAccountName` = `GANTRY_RUNNER_SERVICE_ACCOUNT`;
* one container `runner`: image `GANTRY_RUNNER_IMAGE`, `imagePullPolicy: Always`, `command: ["gantry-runner"]`, env `GANTRY_PLAN` (the plan JSON; step data only, never a value from a Secret), `GANTRY_NAMESPACES`, `GANTRY_VALUES_DIR=/values`, `GANTRY_PACKAGES_DIR=/work/packages`, `RIVETER_AGE_KEY_FILE=/keys/age.key`; `envFrom` the Secret `GANTRY_RUNNER_CREDENTIALS_SECRET` (registry credentials: `RIVETER_WAREHOUSE_URL`, `RIVETER_GATEHOUSE_URL`, `RIVETER_CLIENT_ID`, `RIVETER_CLIENT_SECRET`); volumes: `work` (emptyDir at `/work`), for each Secret named in the plan's `values_secret` a volume `values-<secret>` (Secret `<secret>`, **optional**) at `/values/<secret>`, and `age-key` (Secret `GANTRY_AGE_KEY_SECRET`, **optional**) at `/keys`.

Status: no `status` yet or no condition: `Active`; `succeeded > 0`: `Succeeded`; a `Failed=True` condition: `Failed(message or reason or "the Job failed")`; not found: `Absent`. The log is the log of the newest pod with label `job-name=<name>`. Cleanup and stop both delete the Job with background propagation.

## Self-upgrade

If a package's workloads include Gantry's own Deployment (`GANTRY_SELF_DEPLOYMENT`, default `forge/gantry`), the planner orders the plan so a broken Gantry cannot strand the cluster: everything else is installed first (`--except deployment/gantry`), then Gantry **alone and last** (`--no-wait`), then a `rollout` of it with `rollback_on_failure`. The runner Job outlives the pod it replaces and the old pod keeps serving until the new one is ready; if the rollout does not become ready within `GANTRY_SELF_ROLLOUT_TIMEOUT_SECS` the runner runs `kubectl rollout undo` and the operation ends `rolled_back` with the reason (or `failed` when the undo also fails). A service that restarted cannot roll itself back; the runner does.

## Plans

Stored in `gantry.plans` (`target`, `action`, `version`, `basis`, `plan`, `created_by`, `operation_id`). `basis` is what the plan was made against (the running version; for deployments a signature). Confirming marks the plan with the sentinel `operation_id = 'claiming'`, submits, then stores the operation id; if submitting is refused the claim is released. A plan runs at most once.
