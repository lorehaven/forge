# Conveyor 🏗️

The CI/CD service for the Forge estate.

A webhook arrives, conveyor checks out the commit that triggered it, reads the
`.conveyor.toml` that commit declares, and runs it — reusing the tooling the
estate already has: `anvil` for builds, tests and images, `riveter` for
manifests, `warehouse` as the registry, `gatehouse` for identity.

## Why the pipeline lives in the repository

`.conveyor.toml` is read from the checkout, at the commit being built. Pipelines
are versioned with the code they build, a branch can change its own build, and
that change is reviewable in the pull request that makes it. Conveyor's own
database holds registrations, secrets and run history — never the pipeline.

## `.conveyor.toml`

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

**`needs` goes on the stage, not the job.** Dependencies are between stages;
a `needs` on a job is rejected with a message saying so rather than silently
ignored.

### Steps

Step kinds are `run` (shell), `anvil`, `riveter` and `warehouse`. A bare string
is shorthand for `run`, so these are the same step:

```toml
steps = ["cargo build"]
steps = [{ run = "cargo build" }]
```

Every key is checked — a misspelled one is an error, not something quietly
dropped. So is anything that cannot run: an unknown step kind, an empty command,
a stage with no jobs, a `needs` naming no stage, a cycle. A pipeline that would
fail is a parse error naming the stage and job at fault, before the run starts.

The tool steps have their **command word** checked too, against what that tool
actually accepts — so `{ riveter = "aply k8s/" }` is a parse error rather than a
deploy stage that fails after build and test have already spent their time.
Flags are not checked: clap does that, and a second copy of each tool's argument
tables here would be two definitions drifting apart. `riveter repl` is refused
outright, since it waits for input conveyor never sends and the job would hang
until its timeout.

### Artifacts

A run's checkout is deleted when it finishes, so `artifacts` paths are uploaded
rather than merely noted:

```toml
[[stage.job]]
artifacts = ["target/release/thing"]
steps     = [{ anvil = "build --release" }]
```

They go to warehouse's file storage under `conveyor/{run-id}/`, and the run
records the name, the URI and a sha256 digest — visible on `GET /runs/{id}`.
Collection happens only for a job that **passed**; keeping the output of a
failed build would keep whatever half-written thing it left behind.

A path that escapes the checkout is refused, and one the job did not produce is
reported — both as warnings that do not turn a green run red, because the build
itself was fine. With no `WAREHOUSE_URL` configured nothing is recorded and the
job says what it produced and did not keep: a row promising an artifact conveyor
cannot produce would be worse than no row.

### Triggers

`on` takes `push`, `pull_request` and `tag`, each a list of glob patterns
matched against the bare ref name (`master`, `release/*`). `*` crosses slashes.

- Omitting `on` entirely builds **every push**.
- Naming any event turns the others **off** — `on = { pull_request = ["*"] }`
  builds no pushes.
- A tag push matches `tag` only. It does not fall back to `push`, so
  `push = ["*"]` does not fire twice on a release tag.
- A manual run is always allowed, whatever the patterns say.

### `when`

Deliberately small: `branch`, `tag`, `event` and `sha`, compared with `==` or
`!=` against a quoted literal, joined by `&&` and `||`. `&&` binds tighter.
There are no parentheses and no negation operator — anything that wants them
belongs in a shell step, where it shows up in the log.

A ref is a branch or a tag, never both, so on a tag build `branch` is empty and
`tag != ''` is how you say "only on a tag".

Skipping propagates: a stage whose `when` is false is skipped, and so is
everything that `needs` it. Both still appear in the run report, with the reason
— a report that hides them cannot answer "why did this not deploy".

## Executors

Selected with `CONVEYOR_EXECUTOR`:

| | |
|---|---|
| `native` | Child processes, in a checkout on local disk. The default. |
| `kubernetes` | One `batch/v1` Job per conveyor job. |
| `mock` | Scripted results, for tests. |

### Kubernetes

One `batch/v1` Job per conveyor job, labelled
`app.kubernetes.io/managed-by=conveyor`. An init container fetches the commit
into an `emptyDir` and the work container runs the steps in it, so nothing is
copied in from conveyor's own disk.

- `backoffLimit: 0` — conveyor owns retries. A silent second attempt inside the
  cluster would report as one run that took twice as long.
- `activeDeadlineSeconds` from the job's timeout, `restartPolicy: Never`.
- Secrets go in a Kubernetes Secret referenced by `envFrom`, not inline in the
  pod spec where anyone who can read pods can read them. It is deleted with the
  Job.
- Cancelling deletes the Job with background propagation, so the pod goes with
  it rather than being orphaned.
- Steps run as one script that announces each step on stderr, which is how the
  log follower knows which step is running and which one failed.

If the cluster is unreachable at startup, conveyor **refuses to run anything**
and every job fails saying so. It does not quietly fall back to the native
executor — this deployment asked for isolation, and running a repository's
pipeline inside conveyor's own container instead is the one substitution that
must never happen silently.

| | |
|---|---|
| `CONVEYOR_K8S_NAMESPACE` | Where Jobs are created. Defaults to conveyor's own. |
| `CONVEYOR_K8S_DEFAULT_IMAGE` | For a job whose pipeline names no `image`. |
| `CONVEYOR_K8S_GIT_IMAGE` | The init container's image. |
| `CONVEYOR_K8S_SERVICE_ACCOUNT` | What the pods run as. |
| `CONVEYOR_K8S_TTL_SECONDS` | How long a finished Job lingers if conveyor never cleans it up. |

Conveyor's own service account needs `create`, `get`, `list` and `delete` on
`jobs`, `pods`, `pods/log` and `secrets` in that namespace.

**This has not been run against a real cluster.** Every decision about what gets
submitted is unit-tested in `tests/unit/executors_manifest_tests.rs`, and the
unreachable-cluster path is verified, but the round trip — pod scheduled, log
followed, verdict read back — has not been. Try it on something disposable
first.

## Running arbitrary code

Under the **native** executor, whoever writes a `.conveyor.toml` gets this
service's privileges — its database and its secret key included. That is why
repositories are registered explicitly rather than inferred from whatever
webhook arrives, and why pull requests from forks are rejected unless
`CONVEYOR_ALLOW_FORK_PR` is set.

Under the **kubernetes** executor the pipeline runs in a pod with whatever
service account you give it and nothing of conveyor's, which is what makes that
flag defensible to turn on.

## Configuration

See `.env` for the full set. The ones worth knowing:

| Variable | Default | |
|---|---|---|
| `CONVEYOR_EXECUTOR` | `native` | Where a job's steps run. |
| `CONVEYOR_WORK_DIR` | `/tmp/conveyor` | Root for per-run checkouts. |
| `CONVEYOR_MAX_CONCURRENT_RUNS` | `2` | Runs in flight on this replica. |
| `CONVEYOR_JOB_TIMEOUT_SECS` | `3600` | Ceiling on a job with no timeout of its own. |
| `CONVEYOR_CHECKOUT_TIMEOUT_SECS` | `600` | Ceiling on the checkout. |
| `CONVEYOR_CLAIM_STALE_AFTER_SECS` | `300` | When a silent worker's run is requeued. |
| `CONVEYOR_ALLOW_FORK_PR` | `false` | Whether a fork's pipeline may run. |
| `CONVEYOR_SECRET_KEY` | — | 32 bytes, hex or base64, sealing the secret store. |
| `CONVEYOR_WEBHOOK_SECRET` | — | Estate-wide signing secret, for repositories with no `WEBHOOK_SECRET`. |
| `CONVEYOR_GITHUB_TOKEN` | — | Token with `repo:status`. Without it, builds happen but are not reported. |
| `CONVEYOR_GITHUB_API` | `https://api.github.com` | For GitHub Enterprise. |
| `CONVEYOR_STATUS_CONTEXT` | `conveyor` | The name conveyor's mark appears under. |
| `CONVEYOR_PUBLIC_URL` | — | Where conveyor is reachable, for linking a mark back to the run. |
| `WAREHOUSE_URL` | — | Where artifacts go. Unset means they are not kept. |
| `WAREHOUSE_TECH_USERNAME` / `_PASSWORD` | — | The service account artifacts are uploaded as. |
| `CONVEYOR_ARTIFACT_STORAGE` | `artifacts` | Which warehouse storage to put them in. |

## Secrets

Sealed with XChaCha20-Poly1305 under `CONVEYOR_SECRET_KEY`, so the database
holds ciphertext rather than tokens. Each value is bound to the scope and name
it lives under, so a row copied between repositories — or renamed in place by
someone with write access to the database but not the key — fails to open
rather than quietly granting a secret to a repository that was never given one.

| | |
|---|---|
| `PUT /repos/{id}/secrets/{name}` · `PUT /secrets/{name}` | Write, replacing what was there. |
| `GET /repos/{id}/secrets` · `GET /secrets` | Names only. |
| `DELETE /repos/{id}/secrets/{name}` · `DELETE /secrets/{name}` | Remove. |

**Nothing reads a value back out.** A stolen session can overwrite a secret,
which is visible, but cannot read the estate's tokens.

A job sees a secret only if it named one — that is the whole access model:

```toml
[[stage.job]]
secrets = ["DEPLOY_TOKEN"]
steps   = ["./deploy.sh"]
```

A repository's own value wins over the estate's, so a shared default can be set
once and overridden where it matters. A declared secret that is set nowhere
**fails the job**, rather than running a deploy step with a blank token that
fails further on in a way that takes much longer to understand.

Names must be usable as environment variables. Values shorter than 4 characters
are refused: they cannot be kept out of a build log without destroying it.

### Redaction

Injected values are stripped from output as it is emitted, so both the stored
log and the live stream are redacted. This is a backstop, not a guarantee — a
step that transforms a secret before printing it gets past it, and nothing short
of not injecting the secret would stop that.

### Webhook secrets

A repository with a `WEBHOOK_SECRET` signs its deliveries with that; one without
falls back to `CONVEYOR_WEBHOOK_SECRET`. Per repository is the better
arrangement: one compromised hook does not let somebody forge deliveries for
every other repository conveyor builds.

## Webhooks

`POST /conveyor/api/v1/webhooks/{github|generic}` — outside the realm's auth,
because a provider has no realm token. A delivery is authenticated by its
signature instead, and the endpoint **refuses to serve at all** without
`CONVEYOR_WEBHOOK_SECRET`: accepting unverified deliveries would let anyone on
the network start a build.

The order matters, and is the same for every provider:

1. read the event, to learn which repository it claims to be about — a ping, a
   branch deletion or a pull request being labelled is accepted and does
   nothing;
2. find that repository, which must already be registered;
3. verify the signature, with that repository's own secret if it has one, over
   the raw bytes;
4. refuse a fork's pull request unless `CONVEYOR_ALLOW_FORK_PR` is set;
5. queue the run, once per delivery id.

Reading comes before verifying because the secret is per repository and the body
is the only thing that says which repository this is. That is safe: step 1 is
deserialisation and step 2 is a read, and nothing is acted on until the
signature checks out. It does mean an unauthenticated caller can tell a
registered repository from an unregistered one — a deliberate trade for
per-repository secrets, and the same one every multi-tenant receiver makes.

A redelivery answers `200` with the run it already made rather than queueing a
second one — providers retry deliveries they did not get a prompt answer for,
and a second run of the same commit would double every side effect.

### GitHub

Signs with `X-Hub-Signature-256`. Builds `push` and the pull-request actions
that change code (`opened`, `reopened`, `synchronize`, `ready_for_review`). A
fork's pull request is built from `refs/pull/N/head`, since its branch does not
exist in the base repository; a same-repository one uses its branch, so
`branch == '…'` in a `when` still reads the way it looks.

Results go back as commit statuses. `Skipped` reports as success — nothing ran,
so nothing is wrong — and `Cancelled` as an error rather than a failure, since
the code was never shown to be broken.

### Generic

For a repository on a host conveyor has no integration with. Signs with
`X-Conveyor-Signature-256` over a small payload the sender writes, and nothing
is reported back:

```bash
BODY='{"delivery_id":"'$(git rev-parse HEAD)'","owner":"me","name":"thing",
       "ref":"refs/heads/master","sha":"'$(git rev-parse HEAD)'"}'
SIG="sha256=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -r | cut -d' ' -f1)"
curl -X POST "$CONVEYOR/api/v1/webhooks/generic" \
     -H "X-Conveyor-Signature-256: $SIG" -d "$BODY"
```

## API

All under `/conveyor/api/v1`, behind the realm's auth. Every POST/PUT/DELETE
below needs `conveyor:write` (or a wildcard role); every GET needs only
`conveyor:read`, enforced by `RequireWrite` (`quench-auth`).

| | |
|---|---|
| `POST /repos` | Register a repository. |
| `GET /repos` · `GET /repos/{id}` | List, inspect. |
| `POST /repos/{id}/enabled` | Turn one on or off. |
| `DELETE /repos/{id}` | Remove it and its history. |
| `POST /repos/{id}/runs` | Trigger a run. Without a `sha`, conveyor resolves the ref against the remote. |
| `GET /runs` · `GET /runs/{id}` | List, inspect (with jobs). |
| `POST /runs/{id}/cancel` | Ask a run to stop. |
| `GET /jobs/{id}/logs` | Output for a finished job. |
| `GET /jobs/{id}/stream` | The same output as server-sent events, live while the job runs. `?format=html` for the page. |

## Pages

`/conveyor/ui/home` lists recent runs and registered repositories;
`/conveyor/ui/runs/{id}` shows one run — its jobs, why any were skipped, its
artifacts, and each job's output.

Both keep themselves current without a reload. The home page swaps its two
panels every five seconds; a run page asks for its own state every two, and
**stops when the run rests** — a terminal run's fragment carries no
`hx-trigger`, so the swap that reports it finished is the same one that ends the
polling. Nothing else has to notice.

Polling rather than another stream, because a run's state is in the database
rather than in the worker holding it, so any replica can answer — which the log
stream cannot claim.

What the poll may replace is the part of the page that is not a log. The job
bodies are left alone: re-rendering those every two seconds would tear down
whatever stream is open inside them, so each job's pill and duration sit in
their own `.job-state` element that arrives as an out-of-band swap around the
`<details>` rather than through it. The whole job list is sent only when the
browser's count disagrees with the database's — the moment a queued run is
planned and goes from no jobs to all of them, and the only moment at which
replacing the list can cost nothing.

Logs stream over server-sent events, the same way switchboard and sage stream
theirs: htmx's SSE extension (`hx-ext="sse"`, `sse-connect`, `sse-swap`), with
the server sending HTML fragments. No hand-written JavaScript.

Two details differ from those two, both because a log is append-only and
unbounded where their payloads are whole-state replacements:

- frames are appended with `hx-swap="beforeend"` rather than replacing the
  element, so a ten-thousand-line log is not re-sent on every line;
- the viewer is fetched with `hx-get` when a job is opened, inside a native
  `<details>`. Inlining it would mean `sse-connect` opening one stream per job
  on page load — eight jobs, eight held connections, whether or not anyone
  looked.

The same endpoint serves `conveyor logs --follow` as plain text; `?format=html`
selects the fragment form. That is a query parameter rather than content
negotiation because `EventSource` sends `Accept: text/event-stream` and gives
the page no way to ask for anything else.

Build output is written by whoever owns the repository, so HTML frames are
escaped through the same helper the rest of the estate's pages use. Text frames
are deliberately not — they are read by a terminal.

One caveat, and it is the log-persistence trade showing through: only the
replica actually running a job holds its live output, so with several replicas
behind one address a browser can land on one that is not running the job and
will see nothing until it finishes. A finished job's log comes from the
database and is complete wherever you ask.

## The queue

Runs *are* the queue: `runs.status`, `claimed_by` and `claimed_at` are what the
claim loop reads and writes. That means a run cannot be queued twice, a restart
loses nothing, and "what is running" is a select rather than a question for a
broker that has already forgotten.

Workers claim with `SELECT … FOR UPDATE SKIP LOCKED`, so several replicas share
one queue without coordinating. A repository never has two runs in flight: the
claim query skips a busy one, and a partial unique index makes that a guarantee
rather than a hope, since the check and the claim are not one atomic act.

A worker refreshes `claimed_at` while it works. One that stops — killed, or
gone with its pod — has its run put back by the janitor; without that, a single
dead worker would take its repository out of service permanently.

Conveyor **requires Postgres**. Against the estate's in-memory database it
refuses to start the scheduler rather than looking healthy and losing every
queued run on restart.

## Schema

Installed by foundry from `docker/foundry-service/migrations/conveyor/`, into
the `conveyor` schema. Conveyor never migrates at boot.

## Tests

The database-backed tests are skipped unless `CONVEYOR_TEST_DATABASE_URL` is
set, the way the estate skips its Redis-backed cache tests. Point it at a
throwaway database — every test truncates conveyor's tables first.

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

## Requirements

- `postgres`
- `git` on `PATH`, under either executor - conveyor checks out the commit before
  it can read the pipeline that commit declares
- whatever the pipelines themselves call: `cargo`, `docker`, `kubectl`, `anvil`,
  `riveter`

## Runtime image

Built from `docker/Dockerfile.alpine`, the same template as every other service.
Conveyor's only difference is declared in `.anvil.toml`:

```toml
build_args = { RUNTIME_PACKAGES = "git" }
```

Nothing a *pipeline* needs is in this image, deliberately. Under the kubernetes
executor each job names its own image and the toolchain lives there. Under the
native executor a step runs in conveyor's own container, so an operator who
wants `cargo` or `kubectl` available should build on top of this image and say
what goes in it - conveyor guessing on their behalf produced a several-hundred
megabyte image that was still wrong for anything that was not a Rust repository.
