---
title: Runs and the scheduler
parent: Conveyor
grand_parent: Services
nav_order: 11
---

The queue, the workers that drain it, the status machines of runs, jobs and steps, cancellation, restarts, the schedule clock, commit statuses and run-result mail. Sources: `src/scheduler/{queue,worker,schedules,stages,projects,repos}.rs`, `src/notifications/`, `src/providers/`.

## The queue is the `runs` table

There is no broker. A queued run is a `runs` row with `status = 'queued'`. A worker claims the oldest one with a single `UPDATE ... WHERE id = (SELECT ... FOR UPDATE SKIP LOCKED LIMIT 1)`:

* candidates are `status = 'queued'` rows whose repository is `enabled` and has **no run in `running`**;
* ordered by `queued_at`;
* the claim sets `status = 'running'`, `claimed_by = <worker id>`, `claimed_at = NOW()`, `started_at = COALESCE(started_at, NOW())`, `attempt = attempt + 1`.

The "one running run per repository" rule is enforced by the partial unique index `runs_one_running_per_repo_idx`. The claim query checks it too but not atomically; when two workers race, the loser gets a unique violation (`23505`), which the claim treats as "nothing claimable right now". Several replicas therefore share one queue with no coordination. Runs of different repositories run concurrently; a second queued run of a busy repository waits.

Enqueueing is `INSERT ... ON CONFLICT (delivery_id) WHERE delivery_id IS NOT NULL DO NOTHING`. A run with a delivery id that exists is not queued again and the existing run is returned (the webhook answers `200` instead of `202`). Manual, restart and (separately keyed) schedule runs: manual and restart runs have no delivery id; schedule runs use `schedule:<schedule id>:<due unix seconds>`.

## Workers

At start `spawn_pool_notifying` (only if the database is Postgres) starts:

* `CONVEYOR_MAX_CONCURRENT_RUNS` workers (default 2) in this process, each with id `<HOSTNAME or "conveyor">-<index>-<uuid>`; the setting is per replica.
* One janitor.
* (From `main`) the schedule clock and, if configured, the notification delivery loop.

A worker loops: claim; if nothing (or an error) sleep 2 s; otherwise execute the run. While it executes, a heartbeat task sets `claimed_at = NOW()` every `max(CONVEYOR_CLAIM_STALE_AFTER_SECS / 3, 5)` seconds (only while `claimed_by` is this worker and the run is `running`).

The **janitor** wakes every `max(CONVEYOR_CLAIM_STALE_AFTER_SECS / 2, 5)` seconds and sets back to `queued` (clearing `claimed_by` and `claimed_at`) every `running` run whose `claimed_at` is older than `CONVEYOR_CLAIM_STALE_AFTER_SECS` (default 300): a worker that died mid-run does not take its repository out of service. The `attempt` counter is not reset.

### What a worker does with a run

1. Load the repository (missing: the run ends `failed`, `repository <id> no longer exists`). Report `pending` / `build started` to the provider.
2. Check out the commit (see [Pipelines](pipelines.md)); read and parse `.conveyor.toml`; synchronise schedules; decide whether the pipeline wants the event; apply the stage selection; plan; write all jobs; copy reused jobs (restart).
3. Execute the jobs (concurrently, as their stages become ready), recording per job: steps, then the full log, then the status.
4. Delete the checkout. Finish the run: `status`, `error`, `finished_at = NOW()`, `claimed_by = NULL`.
5. Report the final status to the provider; queue notifications.

If anything between 1 and 4 returns an error the run is finished `failed` with the error text. A run is never retried automatically: a failed run stays failed until a person restarts it.

## Run status

`Status` is one vocabulary for runs, jobs and steps: `queued`, `running`, `success`, `failed`, `cancelled`, `skipped`. Terminal states are `success`, `failed`, `cancelled`, `skipped`. `failed` and `cancelled` are "failures" (a restart is offered for them); `skipped` is not.

| From | To | When |
|---|---|---|
| (created) | `queued` | webhook, manual trigger, restart, schedule firing |
| `queued` | `running` | a worker claims it |
| `queued` | `cancelled` | `POST /runs/{id}/cancel` (immediate; `finished_at` set) |
| `running` | `queued` | the janitor found the claim stale |
| `running` | `success` | roll-up of the jobs (below) |
| `running` | `failed` | roll-up, or an error before/between jobs (checkout, no or invalid pipeline, ...) |
| `running` | `cancelled` | a cancellation was requested and a job ended `cancelled`; error text `cancelled` |
| `running` | `skipped` | the pipeline does not want the event, or every job was skipped |

Roll-up of the job statuses of a run (`Status::rollup`): no jobs, or all skipped: `skipped`; any `queued` or `running`: `running`; else any `failed`: `failed`; else any `cancelled`: `cancelled`; else any `success`: `success`. A run whose only outcome is a cancelled job is `cancelled`.

### Cancellation

`POST /runs/{id}/cancel` on a `running` run sets `cancel_requested = true` and leaves `status` alone (a cancelled-but-still-running run is both things). The worker holding the run checks the flag every 8th poll of a running job (2 s at the 250 ms job poll) and calls the executor's `cancel`; the job ends `cancelled`, later steps are `skipped`, jobs that had not started are `skipped` with `the run was cancelled`, and the run finishes `cancelled`. A run claimed but still checking out only notices when its first job is polling. Steps already executed are not undone.

## Job and step status

A job row is created by the plan with one of:

* `queued`: it will run;
* `skipped` with a reason: excluded by a `when`, or blocked by a stage that did not run;
* `success` with `reused_from_run`: carried over from the run being restarted.

During execution: `queued` -> `running` (`started_at` set) -> `success` | `failed` | `cancelled`, or `queued` -> `skipped` (a stage it needs failed; the run was cancelled). A job fails when: its secrets cannot be resolved (error text from the store), the executor cannot start it (error from the executor, for example a service on the native executor), a step exits non-zero (`exit_code` is the first failing step's code, or the last step's if all passed), a step cannot be spawned, or the timeout is exceeded.

Steps are written when the job ends (not live): all steps of the job in order; a step after the failing one is `skipped`. Step rows are not exposed by the API; they feed the scan reader and the restart copy.

Logs are likewise written when the job ends; until then the live output is only in the executor of the replica running the job (see [Events](events.md)). A worker that dies mid-job loses that job's log.

## Restarts

`POST /runs/{id}/restart` (or the Restart button) creates a **new** run. The old row is untouched. The new run copies trigger, ref, sha, message and `stages` from the old run and has `resumed_from` = the old run id.

When the worker plans the new run it reads the old run's jobs and treats a stage as passed when **every** job of that stage is `success`. For each job of a passed stage the new job row is created as `success` with `reused_from_run` = the old run id, and then the old job's steps, log lines and artifacts are copied onto it (artifacts get new ids and the new run/job ids; `exit_code` is copied). Stages that failed, were cancelled, were skipped or never ran execute normally. If the old run's jobs cannot be read, nothing is carried over and the run proceeds as a full run.

## The schedule clock

`schedules::spawn` starts a loop that, every 15 seconds, selects the rows with `next_run_at <= now` and, for each:

1. Reads the repository. Missing: skipped.
2. Computes the next firing from **now** (not from the missed instant): a long outage yields one catch-up run, not a burst.
3. Enqueues a `schedule` run on the default branch's current commit (ref `refs/heads/<default_branch>`, sha resolved by `git ls-remote`) with `delivery_id = schedule:<id>:<due unix seconds of the old next_run_at>`.
4. Advances the row with `UPDATE ... WHERE id = $1 AND next_run_at = <old>` (compare and set), setting `last_run_at` = the old due time.

Because the run is enqueued before the row advances, a crash in between, or two replicas racing, produces one run. A repository that no longer exists is skipped (its schedule rows go with it through the foreign key). A repository that is disabled counts as skipped too: no run is queued and the row is advanced like a started one. An unreachable remote or a bad ref leaves the firing due and it is retried on the next tick. A database error aborts the pass and is logged.

A run triggered this way has `trigger = schedule`; the pipeline uses `when = "event == 'schedule'"` to select stages.

## Commit statuses

Before the pipeline starts and when the run ends the worker reports the commit to the repository's provider:

| Run status | State sent |
|---|---|
| `queued`, `running` | `pending` (description `build started`) |
| `success`, `skipped` | `success` |
| `failed` | `failure` |
| `cancelled` | `error` |

The final description is the run's error text when there is one, else `all stages passed` / `a stage failed` / `cancelled` / `nothing to build for this event`. Only GitHub repositories get a report: `POST {CONVEYOR_GITHUB_API}/repos/{owner}/{name}/statuses/{sha}` with bearer `CONVEYOR_GITHUB_TOKEN`, body `{state, description (140 characters at most, ending `…` when cut), context (CONVEYOR_STATUS_CONTEXT, default conveyor), target_url?}`. `target_url` is `{CONVEYOR_PUBLIC_URL}/ui/runs/{run id}` and is omitted when `CONVEYOR_PUBLIC_URL` is empty. Without the token the report is skipped silently; any other failure is a warning. The generic provider reports nothing.

## Run-result mail

A person follows a repository or a project (`PUT .../subscription`). When a run ends, after the commit status:

* `failed`: everyone following gets `conveyor.run.failed`.
* `success`: only if the previous run of the same repository **and ref** that ended `success` or `failed` (others ignored; queued before this one) had failed: `conveyor.run.recovered`.
* `cancelled`, `skipped`: nothing.

Followers are the users subscribed to the repository or to any project on the chain from the repository's project to the root. With auth enabled, a follower who can no longer read the repository (disabled account, or no `conveyor:read` or `conveyor:project:<id>:read` along the chain) is skipped. Nothing is queued when `CONVEYOR_PUBLIC_URL` is empty (a link is required) or when mail is switched off.

Messages go through an outbox, never inline: a row in `notification_outbox` per `(run, user, template)` (duplicates are ignored). A loop sleeps 5 s when idle, otherwise claims up to 20 due rows (leasing each for 120 s by pushing `next_attempt_at`) and posts them to gatehouse (`POST {GATEHOUSE_URL}/api/v1/notify`, bearer token of the client `conveyor-gatehouse`, body `{username, template, vars, dedupe_key: <run id>, requested: true}`; `vars` = `project` (full path), `run` (`<first 8 of run id> (<short sha>)`), `ref` (ref name), `url`). Outcomes: any success is final and the row is deleted (gatehouse decides the language and whether the person still wants it); `HTTP 400`, or `HTTP 502` with `"retryable":false`, are rejected and the row is kept with `failed_at` set; anything else is retried with delay 30 s, 60 s, 120 s, ... doubling to a cap of one hour (`min(30 * 2^(n-1), 3600)`), and after 10 attempts the row is kept with `failed_at` set and `last_error`. Mail is off (a log line at start) unless `GATEHOUSE_URL` and `CLIENT_SECRET_CONVEYOR_GATEHOUSE` are both set.
