# Report: conveyor and gantry service pages

Pages written: `docs/services/conveyor/` (index, api, openapi.yaml, pipelines, scheduler, executors, events, scans, permissions, configuration, data, operations, client) and `docs/services/gantry/` (index, api, openapi.yaml, resources, deployments, reconciler, permissions, configuration, data, operations, client). Old pages `docs/docker/conveyor-service.md` and `docs/docker/gantry-service.md` deleted. `python3 scripts/docs-check.py conveyor gantry` passes (conveyor 47 API + 24 `/ui` routes; gantry 18 API + 21 `/ui` routes; all are `#[get]`/`#[post]`/... macros, so the checker's regex covers every form used). The three `quench-starter` routes (`/health`, `/health/live`, `/health/ready`) cannot be in an `openapi.yaml` while the checker reports "documented but no route macro in source"; they are documented on the Operations pages and on the common-runtime page.

## 1. Stale or wrong statements in the old docs

Conveyor (`docs/docker/conveyor-service.md`):

* `services/conveyor/service/README.md` says the old page holds "the API and configuration, secrets, webhooks". The page had no API section, no configuration table (only the `CONVEYOR_K8S_*` variables inline), no webhook section and no secrets section. Now covered by `api.md` and `configuration.md`.
* "the sliders icon beside a repository's run button ... opens `/ui/repos/{owner}/{name}/run`": the client shows a text link/button "Run with parameters" (`ui_launch_link`, id `launch-<repo id>` on the home table and `launch-link` on the scan page); there is no icon.
* "`mock` ... Tests only": `CONVEYOR_EXECUTOR=mock` is accepted by the production binary (`executors/mod.rs:31`), with a warning.
* The page does not mention that under the native executor steps inherit the service's whole environment (it says only that the pipeline gets "this service's privileges").

Gantry (`docs/docker/gantry-service.md`):

* "(the API's way of previewing; the UI does not use it)" about plans: the client's Start, Stop and Upgrade/Sync all call `POST .../plan` and then `POST /plans/{id}/confirm` (`web/src/client.rs`, `plan_and_run`), and `/ui/plans/{id}` is a screen.
* `GANTRY_RUNNER_IMAGE` is listed with no default; the code default is `forge/gantry/runner:latest` (`settings.rs`).
* The resource states are given as `synced`, `edited`, `missing`, `not in package`, `secret`; the API values are `synced`, `edited`, `missing`, `extra`, `hidden` (the UI words are translations of those).
* The configuration table omits `GANTRY_RUNNER_BIN`, `GANTRY_KUBE_CONTEXT` is only mentioned in the text, `GANTRY_VALUES_DIR` has default `values` for the service and `/values` for the runner program, `GANTRY_STATE_DIR` default `<temp dir>/gantry`.
* The API table lists `GET /operations` as `read`; it requires the **blanket** `gantry:read` (a package-scoped grant is refused), as do `GET /operations/{id}`; cancel and raw submit need the blanket `deploy`.
* "Stop ... Delete each workload": correct, but `libs/rivet-package/src/manifest.rs:57` (`DefaultState::Stopped`: "Installed but scaled to zero") and `domain/deployments.rs:7` ("Stopping is scaling to zero, never deleting") still say scaling. See open questions.

## 2. In the code but undocumented before

Conveyor: every route's request/response and error texts; the project tree API (create/rename/move/delete); estate-wide and per-repository secret routes; git credential routes (project and repository scope, resolution order); subscriptions routes; `GET /access`, `GET /overview/runs`, `GET /overview/history`, `GET /scan`, `GET /repos/{id}/scan*`, the stored-report route and its sandbox headers; `GET /repos/{id}/schedules`; `GET /repos/{id}/stages`; the environment variables `CONVEYOR_SECRET_KEY`, `CONVEYOR_CREDENTIAL_KEY`, `CONVEYOR_WEBHOOK_SECRET`, `CONVEYOR_GITHUB_TOKEN`, `CONVEYOR_GITHUB_API`, `CONVEYOR_STATUS_CONTEXT`, `CONVEYOR_PUBLIC_URL`, `CONVEYOR_ARTIFACT_STORAGE`, `CONVEYOR_WORK_DIR`, `CONVEYOR_MAX_CONCURRENT_RUNS`, `CONVEYOR_JOB_TIMEOUT_SECS`, `CONVEYOR_CHECKOUT_TIMEOUT_SECS`, `CONVEYOR_CLAIM_STALE_AFTER_SECS`, `CONVEYOR_HOME_RECENT_RUNS`, `CONVEYOR_HOME_MAX_RUNS_PER_REPO`, `CONVEYOR_RUNS_PAGE_SIZE`, `WAREHOUSE_*` (including the basic-auth fallback); the webhook check order and every ignore reason; the `CONVEYOR_*` job environment; the notification outbox (limits, backoff, giving up); the data model; the SSE frame format and the `lagged` event; the six scan kinds and their parsers; the Kubernetes pod layout (names, labels, probes, markers).

Gantry: every route; the operation state machine and the reconciler's behaviour (adoption, restart of a missing runner, cancellation text); the full step set with the exact commands; plan validation rules; the Job manifest; the local runner's files; the fake-cluster seed format (`declared`); `GANTRY_STATE_DIR`, `GANTRY_RUNNER_BIN`; the sync-state rules; the deployment observed/drift rules; the data model.

## 3. Open questions and suspected bugs (not fixed; documentation only)

Conveyor:

1. **Duplicate jobs after a requeue.** `queue::requeue_stale` returns a stale run to `queued`; the next claim runs `perform` again and `queue::create_jobs` (`scheduler/queue.rs:316`, called from `scheduler/worker.rs:442`) inserts a fresh job set without removing the first attempt's rows (no unique key on `(run_id, stage, name)`). The run page would list both sets and the first attempt's jobs stay `running`/`queued` forever. Confirm intended behaviour.
2. **`Last-Event-ID` is not implemented.** `routers/api/stream.rs:84` says `id:` lets "a reconnect send `Last-Event-ID` instead of replaying the whole log"; the handler never reads the header, and a reconnect replays from line 0. The browser avoids it by closing the source on `done`, but a dropped connection mid-run would replay.
3. **Webhook answers before the signature is checked.** `routers/api/webhooks.rs`: parse errors (`400`), validation (`400`) and "not registered" (`404 <owner>/<name> is not registered with conveyor`) are returned before `provider.verify` (line 74), so an unauthenticated sender can learn which repositories are registered. The comment says the order is deliberate for the 503 case, but the 404 leaks.
4. **Native executor environment.** `executors/native.rs:327` spawns steps without clearing the environment, so every pipeline step inherits `DATABASE_URL`, `CONVEYOR_SECRET_KEY`, `CONVEYOR_CREDENTIAL_KEY`, `GATEHOUSE_CLIENT_SECRET` and the rest. Intended ("whoever writes a `.conveyor.toml` gets this service's privileges") but worth a `env_clear` plus an allow-list.
5. **Artifact name collisions.** `artifacts/mod.rs:276` stores a declared artifact at `conveyor/<run id>/<file name>`; two jobs of one run declaring files with the same name overwrite each other (test reports are job-scoped and unaffected).
6. **`GET /api/v1/projects` has no permission check** (`routers/api/projects.rs:77`): any authenticated caller sees the whole tree's names and structure, though `GET /projects/{id}` needs read. Also `PATCH /projects/{id}` renames, then moves, in two statements (a refused move leaves the rename applied).
7. **Credential routes map every database error to 500**, unlike the other routes (`routers/api/credentials.rs:206`): an unknown `project_id` with the blanket grant is a foreign-key error answered `500`, not `400`/`404`.
8. **Stale comment.** `routers/api/runs.rs:338` ("Repo gone but run row survives (no cascading delete)") contradicts `runs.repo_id ... ON DELETE CASCADE` (migration `0001`); the branch is reachable only by a race.
9. **Cancel before the first job polls.** The cancel flag is read only inside `run_one_job` (every 8th poll); a run claimed but still in `checkout` or planning is not stopped until a job is running.
10. `attempt` is never reset after a requeue and is shown on the run page as "Attempt"; there is no limit on how often a poison run can be requeued.
11. Run-result mail is silently dropped when `CONVEYOR_PUBLIC_URL` is empty (the only trace is a warning per finished run, `notifications/mod.rs`); the start-up log does not mention the dependency.
12. The Kubernetes executor round trip is *unverified* here (and, per the old page, was never run against a real cluster).
13. `services/conveyor/service/dist/assets` and `services/gantry/service/dist/assets` contain static files of the previous server-rendered UI; no route in `src/` serves them.
14. READMEs outside `forge/docs` that still point at the deleted pages (not edited): `services/conveyor/service/README.md`, `services/gantry/service/local/README.md`, `tests/forge-sectest/README.md:71`.

Gantry:

15. **Plan stuck in `claiming`.** `planner::confirm` marks the plan `claiming` (`claim_plan`, `planner.rs:417`) and only then submits; if the process dies between the claim and `settle_plan`, the plan's `operation_id` stays `claiming` and it can never be confirmed (`this plan has already been confirmed`).
16. **`POST /resources/refresh` needs only `read`** (`routers/api/resources.rs:193`) but queues a real operation (a Job that downloads a package and renders it). Any reader can queue runner Jobs.
17. **Scoped users cannot follow their own operation.** `GET /operations/{id}` needs the blanket `gantry:read`, but confirming a plan or starting a resource action only needs the package-scoped grant and answers `201` with the operation; the UI then navigates to the operation page, which receives `403`.
18. **UI Start performs a swap without a question.** The manifest comments (`libs/rivet-package/src/manifest.rs`, `conflicts_with`) say starting "asks for confirmation; it never stops them silently"; the client's `plan_and_run` confirms the plan immediately.
19. **Stale comments on stop.** `domain/deployments.rs:7` and `rivet-package` `DefaultState::Stopped` say "scaling to zero"; the code deletes the workloads.
20. **Inconsistent status for "no Postgres"**: `GantryError::NotPostgres` is `503` when it reaches the handler directly (operations list) and `500` when wrapped by `PlanError::Store` (plans, deployments).
21. The client registers `/ui/operations/` (trailing slash, `app/src/live.rs:382`) but the server registers only `/ui/operations` (`routers/ui/pages/client.rs`): a direct load of the slash form is a server 404.
22. `tests/unit/targets_api_tests.rs:138` is named `a_target_shows_its_versions_and_overrides` although overrides were dropped (migration `0005`).
23. The guides report `GANTRY_RUNNER_IMAGE` in the estate overlay at `0.1.0` while the service is `0.2.2` (`docs/_reports/apps-and-guides-guides2.md`); the runner and the service are meant to be released together.
24. `GET /api/v1/resources` lists every discovered kind on every request (8 at a time, only discovery is cached for 60 s) and reads the registry catalog and workloads each time; the home page and the package page both call it.
