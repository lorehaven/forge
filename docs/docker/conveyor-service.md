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

### Artifacts

A run's checkout is deleted when it finishes, so `artifacts` paths are uploaded rather than merely noted:

```toml
[[stage.job]]
artifacts = ["target/release/thing"]
steps     = [{ anvil = "build --release" }]
```

They go to warehouse's file storage under `conveyor/{run-id}/`, and the run records the name, the URI and a sha256 digest — visible on `GET /runs/{id}`. Collection happens only for a job that **passed**; keeping the output of a failed build would keep whatever half-written thing it left behind.

A path that escapes the checkout is refused, and one the job did not produce is reported — both as warnings that do not turn a green run red, because the build itself was fine. So is one over 512MB: a build that produces something that large wants a registry, not a file store, and streaming it through this service would hold a worker and a warehouse connection for as long as it took. With no `WAREHOUSE_URL` configured nothing is recorded and the job says what it produced and did not keep: a row promising an artifact conveyor cannot produce would be worse than no row.

### Triggers

`on` takes `push`, `pull_request` and `tag`, each a list of glob patterns matched against the bare ref name (`master`, `release/*`). `*` crosses slashes.

- Omitting `on` entirely builds **every push**.
- Naming any event turns the others **off** — `on = { pull_request = ["*"] }` builds no pushes.
- A tag push matches `tag` only. It does not fall back to `push`, so `push = ["*"]` does not fire twice on a release tag.
- A manual run is always allowed, whatever the patterns say.

### `when`

Deliberately small: `branch`, `tag`, `event` and `sha`, compared with `==` or `!=` against a quoted literal, joined by `&&` and `||`. `&&` binds tighter. There are no parentheses and no negation operator — anything that wants them belongs in a shell step, where it shows up in the log.

A ref is a branch or a tag, never both, so on a tag build `branch` is empty and `tag != ''` is how you say "only on a tag".

Skipping propagates: a stage whose `when` is false is skipped, and so is everything that `needs` it. Both still appear in the run report, with the reason — a report that hides them cannot answer "why did this not deploy".

### Secrets on a job

A job sees a secret only if it named it in `secrets = [...]` — that is the whole access model:

```toml
[[stage.job]]
secrets = ["DEPLOY_TOKEN"]
steps   = ["./deploy.sh"]
```

A repository's own value wins over the estate's, so a shared default can be set once and overridden where it matters. A declared secret that is set nowhere **fails the job**, rather than running a deploy step with a blank token that fails further on in a way that takes much longer to understand. See [Secrets](#secrets) below for how they get there in the first place.

## Secrets

Sealed with XChaCha20-Poly1305 under `CONVEYOR_SECRET_KEY`, so the database holds ciphertext rather than tokens. Each value is bound to the scope and name it lives under, so a row copied between repositories — or renamed in place by someone with write access to the database but not the key — fails to open rather than quietly granting a secret to a repository that was never given one.

| | |
|---|---|
| `PUT /repos/{id}/secrets/{name}` · `PUT /secrets/{name}` | Write, replacing what was there. |
| `GET /repos/{id}/secrets` · `GET /secrets` | Names only. |
| `DELETE /repos/{id}/secrets/{name}` · `DELETE /secrets/{name}` | Remove. |

**Nothing reads a value back out.** A stolen session can overwrite a secret, which is visible, but cannot read the estate's tokens.

Names must be usable as environment variables. Values shorter than 4 characters are refused: they cannot be kept out of a build log without destroying it. How a job gets access to one it's named is covered under [Secrets on a job](#secrets-on-a-job) above.

### Redaction

Injected values are stripped from output as it is emitted, so both the stored log and the live stream are redacted. This is a backstop, not a guarantee — a step that transforms a secret before printing it gets past it, and nothing short of not injecting the secret would stop that.

### Webhook secrets

A repository with a `WEBHOOK_SECRET` signs its deliveries with that; one without falls back to `CONVEYOR_WEBHOOK_SECRET`. Per repository is the better arrangement: one compromised hook does not let somebody forge deliveries for every other repository conveyor builds.

## API / Webhooks

All under `/conveyor/api/v1`, behind the realm's `Auth` middleware except where noted (see [Organisational tree and authorization](#organisational-tree-and-authorization) for exactly what each route then checks).

Deliveries land on `POST /api/v1/webhooks/{github|generic}`, deliberately outside the realm's auth — a provider has no realm token, so a delivery is authenticated by its own HMAC signature instead. The endpoint **refuses to serve at all** without `CONVEYOR_WEBHOOK_SECRET` configured somewhere: accepting unverified deliveries would let anyone on the network start a build. The order matters, and is the same for every provider:

1. read the event, to learn which repository it claims to be about — a ping, a branch deletion or a pull request being labelled is accepted and does nothing;
2. find that repository, which must already be registered;
3. verify the signature, with that repository's own secret if it has one, over the raw bytes;
4. refuse a fork's pull request unless `CONVEYOR_ALLOW_FORK_PR` is set;
5. queue the run, once per delivery id.

Reading comes before verifying because the secret is per repository and the body is the only thing that says which repository this is. That is safe: step 1 is deserialisation and step 2 is a read, and nothing is acted on until the signature checks out. It does mean an unauthenticated caller can tell a registered repository from an unregistered one — a deliberate trade for per-repository secrets, and the same one every multi-tenant receiver makes. A redelivery answers `200` with the run it already made rather than queueing a second one — providers retry deliveries they did not get a prompt answer for, and a second run of the same commit would double every side effect.

**GitHub** signs with `X-Hub-Signature-256` and builds `push` plus the pull-request actions that change code (`opened`, `reopened`, `synchronize`, `ready_for_review`). A fork's pull request is built from `refs/pull/N/head`, since its branch does not exist in the base repository; a same-repository one uses its branch, so `branch == '…'` in a `when` still reads the way it looks. Results go back as commit statuses: `Skipped` reports as success (nothing ran, so nothing is wrong), `Cancelled` as an error rather than a failure (the code was never shown to be broken).

**Generic** is for a repository on a host conveyor has no integration with. It signs with `X-Conveyor-Signature-256` over a small payload the sender writes, and nothing is reported back:

```bash
BODY='{"delivery_id":"'$(git rev-parse HEAD)'","owner":"me","name":"thing",
       "ref":"refs/heads/master","sha":"'$(git rev-parse HEAD)'"}'
SIG="sha256=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -r | cut -d' ' -f1)"
curl -X POST "$CONVEYOR/api/v1/webhooks/generic" \
     -H "X-Conveyor-Signature-256: $SIG" -d "$BODY"
```

Everything else — `/projects`, `/repos`, `/repos/{id}/runs`, `/runs`, `/runs/{id}/restart`, `/jobs/{id}/logs`, `/jobs/{id}/stream`, `/jobs/{id}/raw`, `/secrets` — sits behind `Auth` and the project-scoped checks described above. A repository is registered under a project (`POST /repos` needs `project_id`), a run can be triggered by hand against a ref or an exact sha (without one, conveyor resolves the ref against the remote), and a job's logs are readable as a finished JSON array, a live server-sent-event stream (`?format=html` renders the same stream as an HTML fragment for conveyor's own UI), or as plain lines with no framing at all (`/jobs/{id}/raw`, for opening a log outside conveyor's own UI). `/runs/{id}/restart` needs the same write grant `POST /repos/{id}/runs` does, and 409s for a run that is not currently failed or cancelled — see [Restarting a run](#restarting-a-run).

Conveyor's UI (`/conveyor/ui/...`) mirrors this: `/home` and the equivalent scoped `/projects/{id}` page show recent runs and the registered tree with no manual reload (htmx polling, not a second stream, since a run's state lives in the database rather than in whichever worker is holding it); `/runs` is the full paginated history (`CONVEYOR_RUNS_PAGE_SIZE` per page); `/runs/{id}` shows the run's jobs as a dependency graph — one row per level, connected top to bottom, stages that run at once drawn side by side — with logs streamed the same way `switchboard` and `sage` stream theirs (htmx's SSE extension, appending frames rather than replacing them, since a log is unbounded where those two services' payloads are whole-state replacements) and two icon buttons above each open log: one opens `/jobs/{id}/raw` in a new tab, the other copies what has streamed in so far to the clipboard. `/repos` additionally offers plain HTML forms to register, edit and delete a repository, enforcing the same project-scoped write grants as the JSON API while browsing itself stays open to any signed-in visitor; `/repos/{owner}/{name}/scan` summarises the most recent run's `anvil lint`/`machete`/`audit` and `cargo llvm-cov` (test coverage) steps, if its pipeline ran any.

## Requirements

- Postgres — conveyor refuses to start its scheduler without it.
- `git` on `PATH`, under either executor, since conveyor checks out the commit before it can even read the pipeline that commit declares.
- Whatever the pipelines themselves call: `cargo`, `docker`, `kubectl`, `anvil`, `riveter`. Reported (not enforced) at startup if missing.

## Configuration

See `.env` for the full set with commentary. The complete table:

| Variable | Default | |
|---|---|---|
| `CONVEYOR_EXECUTOR` | `native` | Where a job's steps run: `native`, `kubernetes` or `mock`. |
| `CONVEYOR_WORK_DIR` | `/tmp/conveyor` | Root for per-run checkouts. |
| `CONVEYOR_MAX_CONCURRENT_RUNS` | `2` | Runs in flight on this replica. |
| `CONVEYOR_JOB_TIMEOUT_SECS` | `3600` | Ceiling on a job with no timeout of its own. |
| `CONVEYOR_CHECKOUT_TIMEOUT_SECS` | `600` | Ceiling on the checkout. |
| `CONVEYOR_CLAIM_STALE_AFTER_SECS` | `300` | When a silent worker's run is requeued. |
| `CONVEYOR_ALLOW_FORK_PR` | `false` | Whether a fork's pipeline may run. |
| `CONVEYOR_HOME_RECENT_RUNS` | `5` | Pipelines shown on the front page. |
| `CONVEYOR_HOME_MAX_RUNS_PER_REPO` | `1` | Of those, at most this many from one repository. |
| `CONVEYOR_RUNS_PAGE_SIZE` | `25` | Rows per page on the full pipeline history (`/ui/runs`). |
| `CONVEYOR_SECRET_KEY` | — | 32 bytes, hex or base64, sealing the secret store. |
| `CONVEYOR_WEBHOOK_SECRET` | — | Estate-wide signing secret, for repositories with no `WEBHOOK_SECRET`. |
| `CONVEYOR_GITHUB_TOKEN` | — | Token with `repo:status`. Without it, builds happen but are not reported. |
| `CONVEYOR_GITHUB_API` | `https://api.github.com` | For GitHub Enterprise. |
| `CONVEYOR_STATUS_CONTEXT` | `conveyor` | The name conveyor's mark appears under. |
| `CONVEYOR_PUBLIC_URL` | — | Where conveyor is reachable, for linking a mark back to the run. |
| `WAREHOUSE_URL` | — | Where artifacts go. Unset means they are not kept. |
| `WAREHOUSE_TECH_USERNAME` / `_PASSWORD` | — | The service account artifacts are uploaded as. |
| `CONVEYOR_ARTIFACT_STORAGE` | `artifacts` | Which warehouse storage to put them in. |
| `WAREHOUSE_TLS_VERIFY` | `true` | Set to `false` to accept the estate's internal certificates. |

### Kubernetes executor

Only read when `CONVEYOR_EXECUTOR=kubernetes`:

| Variable | | |
|---|---|---|
| `CONVEYOR_K8S_NAMESPACE` | Where Jobs are created. Defaults to conveyor's own. |
| `CONVEYOR_K8S_DEFAULT_IMAGE` | For a job whose pipeline names no `image`. |
| `CONVEYOR_K8S_GIT_IMAGE` | The init container's image. |
| `CONVEYOR_K8S_SERVICE_ACCOUNT` | What the pods run as. |
| `CONVEYOR_K8S_TTL_SECONDS` | How long a finished Job lingers if conveyor never cleans it up. |

## Testing

Database-backed tests are skipped unless `CONVEYOR_TEST_DATABASE_URL` is set — every test truncates conveyor's tables first, so it must point at a throwaway database:

```bash
docker run --rm -d --name conveyor-test-pg \
    -e POSTGRES_PASSWORD=postgres -p 55432:5432 pgvector/pgvector:pg18

cargo run -p foundry-service -- apply \
    --catalog docker/foundry-service/migrations \
    --database-url postgres://postgres:postgres@localhost:55432/postgres \
    --install conveyor

CONVEYOR_TEST_DATABASE_URL=postgres://postgres:postgres@localhost:55432/postgres \
    cargo test -p conveyor-service
```

See [Executors: local process vs. Kubernetes](#executors-local-process-vs-kubernetes) above for what is and is not covered on the Kubernetes side — the manifest-building decisions are unit-tested, but the round trip against a real cluster has not been exercised.

[Home](../README.md)
