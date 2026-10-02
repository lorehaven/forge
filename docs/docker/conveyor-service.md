# Conveyor Service

Conveyor is the Forge estate's CI/CD service. A webhook arrives, conveyor checks out the commit that triggered it, reads the `.conveyor.toml` that commit declares, and runs it — reusing the tooling the estate already has rather than reinventing a build system: [anvil](../cli/anvil.md) for builds, tests and images, riveter for applying manifests, warehouse as the artifact registry, gatehouse for identity. Pipelines are versioned with the code they build: a branch can change its own build, and that change is reviewable in the pull request that makes it. Conveyor's own Postgres database holds only registrations, secrets and run history — never the pipeline itself. It is the largest single crate in the workspace (`docker/conveyor-service`, ~90 Rust files), binary and library name `conveyor-service`.

## Features

- **Webhook-triggered builds** from GitHub or a generic, provider-agnostic signed webhook, plus manually triggered runs.
- **An organisational tree of projects and repositories.** Every registered repository is attached to a project node, and project-scoped write/read grants (`conveyor:project:<id>:<action>`) inherit down the tree — a grant on a parent covers everything nested beneath it.
- **Two executors**: child processes on conveyor's own disk (`native`, the default) or one Kubernetes `batch/v1` Job per pipeline job (`kubernetes`), for pipelines that should not run with conveyor's own privileges.
- **A Postgres-backed run queue** that survives a restart — see [Architecture](#architecture).
- **Per-repository and estate-wide secrets**, sealed at rest, visible to a job only if it names them, and never readable back out through the API.
- **Live log streaming** over server-sent events, both to conveyor's own UI and to `conveyor logs --follow` from the [Conveyor CLI](../cli/conveyor-cli.md) — plus a plain `text/plain` raw view (`GET /jobs/{id}/raw`) for opening a log in its own tab or piping it somewhere.
- **Concurrent jobs**: a job starts the moment every stage it needs has finished, rather than waiting its turn in declaration order — every job in a stage (there is usually more than one) runs alongside its stage-mates, exactly as it would alongside a job in an unrelated stage.
- **Manual restarts, not automatic retries**: nothing repeats a failed run on its own. `POST /runs/{id}/restart` starts a new run of the same commit and carries over every stage that passed last time, so only what actually failed (and whatever needed it) runs again.
- **Artifact collection**: paths a job declares are uploaded to warehouse once the job passes, since a run's checkout is deleted the moment it finishes.
- **Following results by email.** A signed-in user can follow a repository, or a whole project (and everything nested under it), from `/repos` or the API, and is emailed when a run on it fails and when the first run passes after a failure - see [Following results](#following-results).
- **Commit status reporting** back to GitHub (`pending`/`success`/`failure`/`error`), when a token is configured.
- **A code-quality summary page** per repository that reads the most recent run's own `anvil lint`/`anvil machete`/`anvil audit`/`cargo llvm-cov` steps — nothing here triggers a scan; it is a best-effort read of whatever the pipeline already ran.

## Architecture

### Executors: local process vs. Kubernetes

Conveyor abstracts "where a job's steps actually run" behind a single `JobExecutor` trait (`src/executors/engine.rs`), selected once at startup from `CONVEYOR_EXECUTOR` and shared across the process:

- **`native`** (default) runs each step as a child process of the conveyor service itself, in a checkout on local disk. This is the simplest deployment, but it means whoever writes a `.conveyor.toml` gets this service's own privileges — its database and its secret key included. That is why repositories must be registered explicitly rather than built from whatever webhook arrives, and why pull requests from forks are rejected unless `CONVEYOR_ALLOW_FORK_PR` is set.
- **`kubernetes`** submits one `batch/v1` Job per conveyor job, labelled `app.kubernetes.io/managed-by=conveyor`. An init container fetches the commit into an `emptyDir`; the work container runs the steps in it, so nothing is copied in from conveyor's own disk. Details:
  - `backoffLimit: 0` — conveyor owns retries itself; a silent second attempt inside the cluster would report as one run that took twice as long.
  - `activeDeadlineSeconds` set from the job's timeout, `restartPolicy: Never`.
  - Secrets go in a Kubernetes `Secret` referenced by `envFrom`, not inline in the pod spec where anyone who can read pods can read them. It is deleted with the Job.
  - Cancelling a run deletes the Job with background propagation, so the pod goes with it rather than being orphaned.
  - Steps run as one script that announces each step on stderr, which is how the log follower knows which step is running and which one failed.
  - If the cluster is unreachable at startup, conveyor refuses to run anything at all and every job fails saying so — it never falls back to `native`, since running a repository's pipeline inside conveyor's own container instead is the one substitution that must never happen silently.

  Configured with `CONVEYOR_K8S_NAMESPACE` (defaults to conveyor's own namespace), `CONVEYOR_K8S_DEFAULT_IMAGE` (for a job whose pipeline names no `image`), `CONVEYOR_K8S_GIT_IMAGE` (the init container's image), `CONVEYOR_K8S_SERVICE_ACCOUNT` (what the pods run as) and `CONVEYOR_K8S_TTL_SECONDS` (how long a finished Job lingers if conveyor never cleans it up). Conveyor's own service account needs `create`, `get`, `list` and `delete` on `jobs`, `pods`, `pods/log` and `secrets` in that namespace.

  **This has not been run against a real cluster.** Every decision about what gets submitted is unit-tested (`tests/unit/executors_manifest_tests.rs`), and the unreachable-cluster path is verified, but the round trip — pod scheduled, log followed, verdict read back — has not been. Try it on something disposable first.
- **`mock`** records what it was asked to do and returns a scripted result. Tests only.

### Running arbitrary code

Under the **native** executor, whoever writes a `.conveyor.toml` gets this service's privileges — its database and its secret key included. That is why repositories are registered explicitly rather than inferred from whatever webhook arrives, and why pull requests from forks are rejected unless `CONVEYOR_ALLOW_FORK_PR` is set.

Under the **kubernetes** executor the pipeline runs in a pod with whatever service account you give it and nothing of conveyor's, which is what makes turning that flag on defensible.

### The Postgres-backed queue

There is no separate broker. `runs.status`, `claimed_by` and `claimed_at` on the `runs` table *are* the queue: a worker claims a run with `SELECT … FOR UPDATE SKIP LOCKED`, so several replicas share one queue without coordinating anything between themselves. This is why the queue survives a restart where an in-memory one would not — "what is running" is a plain `SELECT` against durable state, never a question for a broker that has already forgotten. A partial unique index guarantees a repository never has two runs in flight at once (the claim query alone cannot make that atomic). A worker refreshes `claimed_at` while it works; one that dies mid-job — killed, or gone with its pod — has its run put back on the queue by a janitor once its claim goes stale (`CONVEYOR_CLAIM_STALE_AFTER_SECS`), so a single dead worker does not take a repository out of service permanently.

Conveyor **requires Postgres**. Run against the estate's in-memory database it refuses to start the scheduler outright, rather than looking healthy and quietly losing every queued run on the next restart.

### Job execution order

`worker::execute_jobs` does not walk the pipeline stage by stage, or job by job within a stage. `needs` lives on the stage, not the job, so every job in a stage is exactly as independent of every other job in that same stage as it is of a job in some unrelated stage - a job starts the instant every stage it needs has finished *in full* (every one of that stage's own jobs done), not once every earlier-declared stage or job has had its turn. In practice this means the run page's job graph - one row per dependency level, a stage's jobs grouped into one card, a card in every row that does not wait on anything else in it - is exactly the order things actually execute in, not just how they are drawn: a stage with four unrelated jobs (`check/format`, `check/lint`, `check/deps`, ...) runs all four at once, the common case, just as two independent stages would. The scheduling itself needs no extra threads: it interleaves the same async work (executor polls, database writes) the sequential version already awaited, so several `sleep`-only jobs genuinely overlap in wall-clock time.

One caveat: the **native** executor runs every job's steps against the same checkout, so two jobs racing on the same files is possible if a pipeline's jobs both write to it. The **kubernetes** executor does not share this problem, since each job clones its own copy of the commit.

### Restarting a run

A failed or cancelled run offers a **Restart** button (`POST /runs/{id}/restart` under the hood, or `POST /api/v1/runs/{id}/restart`). This is deliberate: conveyor never repeats a run on its own, so getting another attempt is always something a person asks for.

A restart is a new run, not the old one requeued — the old run's row is untouched, and the new one records `resumed_from` pointing back at it. When the worker plans the new run, any stage whose jobs all passed last time is not re-executed: its result (steps, log, artifacts) is copied onto a new job row instead, marked `reused_from_run`, so the restarted run's page is one coherent record rather than sending a reader back to the old run for half of it. A stage that failed, or one that never ran because something it needed failed, runs for real.

### Organisational tree and authorization

Projects and repositories form one tree (`src/domain/project.rs`, `src/scheduler/projects.rs`): a project node may have children, an attached repository, both or neither. This is also where access control gets specific. Every API route sits behind the realm's `Auth` middleware (a verified identity, `conveyor` as an audience), but writes are **not** gated by the estate's usual blanket `RequireWrite` middleware — a route that acts on a project or repository checks a resource-scoped grant instead (`routers::api::authz::can_on_project`), walking from the target up to the root and accepting a match anywhere along the chain. A caller with no blanket `conveyor:read` still gets `GET /repos` or `GET /runs` filtered to what they can see, rather than a flat 403.

## Pipeline Definition (`.conveyor.toml`)

Read from the checkout, at the exact commit being built — not from conveyor's own configuration. Pipelines are versioned with the code they build: a branch can change its own build, and that change is reviewable in the pull request that makes it. A minimal pipeline:

```toml
on = { push = ["master"], pull_request = ["*"] }

[[stage]]
name = "build"
[[stage.job]]
name  = "cargo"
steps = [{ anvil = "build --all" }]

[[stage]]
name  = "test"
needs = ["build"]
[[stage.job]]
name  = "unit"
steps = [{ anvil = "test --all" }, { run = "cargo fmt --check" }]

[[stage]]
name  = "deploy"
needs = ["test"]
when  = "branch == 'master'"
[[stage.job]]
name    = "k8s"
secrets = ["KUBE_TOKEN"]
steps   = [{ anvil = "docker release-all" }, { riveter = "apply k8s/" }]
```

The parsing lives in a separate crate, `conveyor-pipeline`, so `conveyor validate` can link the same parser a real run uses without linking the whole service around it. A pipeline that would fail is rejected as a parse error naming the stage and job at fault, before the run starts — a misspelled key, an unknown step kind, an empty command, a stage with no jobs, a `needs` naming no stage, or a cycle.

### Keys

| On a `[[stage]]` | |
|---|---|
| `name` | Required, unique within the pipeline. |
| `needs` | Stages that must finish first. |
| `when` | Condition; the stage is skipped when it is false. |

| On a `[[stage.job]]` | |
|---|---|
| `name` | Defaults to the stage name for a sole job, `job-1`, `job-2`… otherwise. |
| `when` | Evaluated on top of the stage's — both must hold. |
| `env` | Extra environment for every step. |
| `secrets` | Names to inject; anything not listed is not visible to the job. |
| `timeout` | Seconds. Defaults to `CONVEYOR_JOB_TIMEOUT_SECS`. |
| `image` | Kubernetes executor only; ignored by the native one. |
| `artifacts` | Paths to collect once the job succeeds. |
| `steps` | Required, at least one. |

**`needs` goes on the stage, not the job.** Dependencies are between stages; a `needs` on a job is rejected with a message saying so rather than silently ignored.

### Steps

Step kinds are `run` (shell), `anvil`, `riveter` and `warehouse`. A bare string is shorthand for `run`, so these are the same step:

```toml
steps = ["cargo build"]
steps = [{ run = "cargo build" }]
```

Every key is checked — a misspelled one is an error, not something quietly dropped.

The tool steps have their **command word** checked too, against what that tool actually accepts — so `{ riveter = "aply k8s/" }` is a parse error rather than a deploy stage that fails after build and test have already spent their time. Flags are not checked: clap does that, and a second copy of each tool's argument tables here would be two definitions drifting apart. `riveter repl` is refused outright, since it waits for input conveyor never sends and the job would hang until its timeout.

### Publishing rivet packages

A `riveter` step can build and publish [packages](../cli/riveter.md#packages), which is how a deployment repository such as `homecloud` turns each overlay into a versioned artifact on every push:

```toml
[on]
push = ["master"]

[[stage]]
name = "publish"
[[stage.job]]
name = "overlays"
secrets = ["RIVETER_WAREHOUSE_URL", "RIVETER_GATEHOUSE_URL", "RIVETER_CLIENT_ID", "RIVETER_CLIENT_SECRET"]
steps = [
  { riveter = "--env forge publish --version-suffix {timestamp}.{sha}" },
  { riveter = "--env vllm publish --version-suffix {timestamp}.{sha}" },
]
```

- **The version** is the one hand-edited in each overlay's `rivet.toml`, plus build metadata. Tool steps are not run through a shell, so riveter expands `{timestamp}` (UTC, `YYYYMMDDHHMMSS`) and `{sha}` (the first seven characters of `CONVEYOR_SHA`, which every job has) itself. Put the timestamp first: Warehouse breaks a tie between two builds of one version by comparing the build metadata as text, and a timestamp sorts chronologically where a commit hash does not. Warehouse refuses a version it already holds, so a re-run of the same commit publishes a new build rather than overwriting.
- **Credentials** are secrets the job names, like any other. The client must be one Gatehouse issues `client_credentials` tokens to for the `warehouse` audience; such a token carries the estate's wildcard `service` role, which is what satisfies Warehouse's `warehouse:write` check. Give pipelines a client of their own rather than Warehouse's own login client - `conveyor-publish` in `config/clients.toml` is that, audience `warehouse` only, so a token minted for it cannot be presented to any other service. Riveter exchanges the id and secret for a token itself.
- **Packing pins image digests**, so the job needs to reach every registry the overlays' images live in (credentials via `RIVETER_REGISTRY_AUTH`); add `--no-pin` to a step to skip that.
- **The job's image needs a riveter that has these commands**, and the native executor needs one on `PATH`. A pipeline that names `publish` against an older riveter parses (conveyor knows the command) and then fails when it runs.
- `riveter install` is accepted too, for a deploy stage that applies a published package instead of a checkout.

`homecloud`, the repository the estate's own manifests live in, is the worked example: its `.conveyor.toml` runs a `check` stage (every overlay must pack) on every push and pull request, and a `publish` stage - one job per overlay - only on a push to master. It is private, so it is registered with a git credential as well as a webhook; `scripts/onboard-homecloud-ci.sh` in that repository does the registration and sets the two secrets its jobs name.
