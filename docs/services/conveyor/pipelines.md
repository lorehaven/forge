---
title: Pipelines
parent: Conveyor
grand_parent: Services
nav_order: 9
---

How the service consumes a `.conveyor.toml`. The syntax itself (keys, step kinds, `when` language, cron) is specified in the library pages: [Pipeline file format](../../libs/conveyor-pipeline/format.md) and [conveyor-pipeline](../../libs/conveyor-pipeline/index.md). This page covers what conveyor does with the parsed result.

## Where the file comes from

The worker checks out the commit recorded in the run (`runs.sha`, never the ref, which can move while the run is queued) into `<CONVEYOR_WORK_DIR>/run-<run id>/` and reads `.conveyor.toml` at its root. Nothing about a pipeline is stored in the database except what a run records of it: each planned job (stage, name, `needs`, status, reason), steps, logs, artifacts, and the schedules (see below). A branch can therefore change its own build.

Failures before any job exists end the run as `failed` with the text in `runs.error`:

| Text | Cause |
|---|---|
| `checkout failed: <reason>` | git failed or timed out (`CONVEYOR_CHECKOUT_TIMEOUT_SECS`); tokens are masked in the reason. |
| `this commit has no .conveyor.toml` | the file is missing or unreadable. |
| `.conveyor.toml is not valid: <reason>` | `conveyor_pipeline::parse` failed (unknown key, unknown step kind, empty command, stage without jobs, unknown `needs`, cycle, bad `when`, bad service, bad cron...). Also used when a run's stage selection names a stage that does not exist (the reason then lists the stages the pipeline has). |
| `repository <id> no longer exists` | deleted while queued. |

## Events and whether the pipeline wants the run

The run's `trigger` is the pipeline's event: `push`, `pull_request`, `manual` or `schedule`. `PipelineSpec::allows(event, ref)` decides:

* `manual` always passes.
* `schedule` passes only if the file declares at least one `on.schedule` entry. A run queued for a schedule that was removed since is therefore skipped.
* `push` and `pull_request` are matched against `on.push` / `on.pull_request` globs on the branch name (`*` matches any run of characters including `/`). A ref under `refs/tags/` is a tag push: it matches only `on.tag` and never `on.push`. `on` omitted means `push = ["*"]`.

A run the pipeline does not want ends with status `skipped` and the text `this pipeline does not run on <event> of <ref>`. It is not a failure: registering a repository means conveyor watches it, the pipeline picks the events. The checkout is deleted.

Tag pushes are dropped earlier, by the webhook handler, unless `CONVEYOR_BUILD_TAGS` is true, because the pipeline can only be read from a checkout and a tag no pipeline wants would still cost a queued run, a clone and a `skipped` row. A manual run of a tag still works.

Pull requests from forks are dropped by the webhook handler unless `CONVEYOR_ALLOW_FORK_PR` is true (under the native executor a fork's pipeline runs with the service's own privileges; a warning is logged at start when both are on).

## Planning

After the file is accepted, the worker builds an `EvalContext` from the run (`event`, ref, sha: `branch` is the ref minus `refs/heads/`, `tag` the ref minus `refs/tags/`, one of them empty) and calls `conveyor_pipeline::plan`. Every stage and job gets a decision:

| Decision | Recorded as | Reason text |
|---|---|---|
| runs | job `queued` | |
| excluded (its own or its stage's `when` is false) | job `skipped` | ``excluded by a `when` condition`` |
| blocked (a stage it needs did not run) | job `skipped` | `stage '<name>' did not run` |

The whole plan is written to the `jobs` table before anything executes, so the run page shows what was decided as well as what happened. `needs` is on the stage and is copied onto each job row.

### Stage selection (run with parameters)

When `runs.stages` is set, the parsed spec is first reduced with `PipelineSpec::only`: the named stages are kept, **each with its own `when` cleared**, `needs` on stages that were not named are dropped, declaration order is kept. A job's own `when` still applies inside a ticked stage. An empty selection or an unknown name is `.conveyor.toml is not valid: ...` listing the available stages. This is how the nightly-only stage can be run by hand. A restart keeps the selection.

## What a job sees

Environment of every step, in order of precedence (later wins): conveyor's own variables, then the job's `env`, then the resolved secrets (so a pipeline cannot shadow a secret with a plain `env` entry):

| Variable | Value |
|---|---|
| `CI`, `CONVEYOR` | `true` |
| `CONVEYOR_RUN_ID` | the run id |
| `CONVEYOR_REPO` | `owner/name` |
| `CONVEYOR_STAGE`, `CONVEYOR_JOB` | the stage and job names |
| `CONVEYOR_EVENT` | the trigger (`push`, `pull_request`, `manual`, `schedule`) |
| `CONVEYOR_REF` | the full ref |
| `CONVEYOR_SHA` | the commit |
| `CONVEYOR_BRANCH`, `CONVEYOR_TAG` | as for `when`; one of them is empty |
| `CONVEYOR_WORKSPACE` | the checkout's directory on the worker (meaningful for the native executor) |

Under the native executor the step process also inherits the service's own environment (the executor does not clear it). See [Executors](executors.md#native).

`secrets = [...]` names are resolved per job: the repository's secret of that name first, then the estate-wide one. A name that exists in neither fails the job before it starts (`this pipeline needs secrets that are not set: A, B`); no key (`CONVEYOR_SECRET_KEY` unset) fails it with the key error. Every resolved value, and the repository's git token, is registered with a redactor that replaces occurrences in the step command text and in each log line with `••••••` (values shorter than 4 characters are never redacted, which is why they cannot be stored). The redaction is a backstop: an encoded secret evades it.

`timeout` is the job's seconds, default `CONVEYOR_JOB_TIMEOUT_SECS` (3600). A job that exceeds it ends `failed` with `job exceeded its timeout of <n>s`.

`image` is honoured by the kubernetes executor only; under native a warning is logged and it is ignored. `service` containers need the kubernetes executor; native refuses the job.

## Dependencies and concurrency inside a run

Jobs start the moment every stage they need has finished in full (all of that stage's jobs done), not in declaration order. All jobs of a stage run concurrently with each other, and independent stages run concurrently. The native executor runs every job against the same checkout, so two jobs that write the same files can race; the kubernetes executor gives each job its own clone. A stage that has a failed (or cancelled) job marks its dependents' jobs `skipped` with `stage '<name>' failed`; stages that do not depend on it keep running. After a cancellation, jobs that had not started are `skipped` with `the run was cancelled`.

The run's final status is the roll-up of all job statuses (see [Runs and the scheduler](scheduler.md#run-status)).

## Artifacts

A job's `artifacts = [paths]` are uploaded to warehouse **only when the job succeeded**, because the checkout is deleted when the run ends. Each path is resolved inside the checkout (symlinks followed first; a path that escapes is refused with `` `<path>` is outside the checkout ``) and must be an existing regular file (a directory or a missing path is `` `<path>` was declared as an artifact but the job did not produce it ``); the limit is 512 MiB (`is <n> bytes, over the 536870912-byte limit`). The file is uploaded with `PUT {WAREHOUSE_URL}/api/v1/files/{CONVEYOR_ARTIFACT_STORAGE}/file?path=conveyor/<run id>/<file name>` and recorded as an `artifacts` row of kind `file` with `name` = the file's name, `uri` = that URL and `digest` = `sha256:<hex>`. Two jobs of one run that declare files with the same name write the same remote path. Without `WAREHOUSE_URL` nothing is kept and a warning is logged per job. Upload problems are warnings, never a failed job.

Separately, whatever the job's outcome, conveyor looks for the test reports a `quench-test` run leaves (`target/debug/quench-test` and `target/release/quench-test`: `summary.json`, `index.html`; and `target/security`: `summary.json`, `index.html`) and stores them (path `conveyor/<run id>/<job id>/<name>`, so jobs do not collide) as artifacts named `summary.json` and `index.html`, or, for the security directory, `security-summary.json` and `security.html`. See [Scans](scans.md).

## Schedules

A run of the repository's **default branch** with trigger `push`, `manual` or `schedule` synchronises the `schedules` table with the file's `on.schedule` (and `on.timezone`): new entries are inserted with `next_run_at` = the next firing after now, withdrawn ones are deleted, unchanged ones are left alone. Other branches and `pull_request` runs never change schedules. Details of the firing loop are on the [scheduler page](scheduler.md#the-schedule-clock).

## Validation of refs, shas and URLs

Checked at registration (URL), at manual trigger and at webhook receipt (ref, sha), and again before the checkout:

* sha: 7 to 64 hex digits.
* ref: not empty; not starting with `-`; no whitespace or control characters; none of `~ ^ : ? * [ \`; no `..`, `@{`, `//`; not starting or ending with `/`; not ending with `.` or `.lock`.
* clone URL: not empty, not starting with `-`, no control characters (the URL is administrator-supplied; only shapes that confuse `git` are refused).

## Publishing and tool steps

Steps of kind `anvil`, `riveter` and `warehouse` are parsed and their command word checked by the library (`steps::validate`); the executor turns a step into an argv (`run` goes through `sh -c`, tool steps are split with `shlex` and prefixed with the tool's binary). The pipeline of the `homecloud` repository (a `check` stage on every push and a `publish` stage with one job per overlay on master, using the secrets `RIVETER_WAREHOUSE_URL`, `RIVETER_GATEHOUSE_URL`, `RIVETER_CLIENT_ID`, `RIVETER_CLIENT_SECRET`) is the worked example of `riveter publish`; the client it uses is `conveyor-publish` (audience `warehouse` only).
