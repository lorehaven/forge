---
title: Executors
parent: Conveyor
grand_parent: Services
nav_order: 8
---

Where the steps of a job run. One executor is chosen at start from `CONVEYOR_EXECUTOR` and shared by every worker (`src/executors/`). All implement the same trait: `start(spec, workspace)` returns a handle at once; `poll(handle)` returns the job's state (status, exit code, error, one entry per step, always); `logs(handle)` returns the lines so far plus a subscription to the rest; `cancel(handle)`; `forget(handle)` is called after the worker has stored the outcome in the database.

A log chunk is `{seq, stream, line, at}`; `seq` is contiguous per job. Redaction of secrets is applied by the executor to every line and to the command text it reports, so it also covers the live stream.

## native

The default. Each step is a child process of the conveyor service, started in the run's checkout (`<CONVEYOR_WORK_DIR>/run-<run id>`), with:

* stdin closed, stdout and stderr piped, line by line, into the log; `kill_on_drop` and its own process group, so a cancel or timeout kills the step and what it started;
* the argv from `conveyor_pipeline::steps::argv` (`run` steps through `sh -c`, tool steps split with `shlex` and prefixed with `anvil`, `riveter`, `warehouse-cli`);
* the job's environment set on top of the **service's own environment** (the process environment is not cleared). That includes `DATABASE_URL`, `CONVEYOR_SECRET_KEY`, `CONVEYOR_CREDENTIAL_KEY` and every other variable of the service. This is why repositories must be registered explicitly and why fork pull requests are refused by default.

Steps run in order and stop at the first failure; the remaining steps are `skipped`. Exit code of the job is the first failing step's code (or the last step's). A deadline of `timeout` seconds covers the whole job: on expiry the step is killed, the job is `failed` with `job exceeded its timeout of <n>s` and the log gets the line `timed out after <n>s` on stderr. A cancel marks the step `cancelled`, the log gets `step cancelled`, and the job ends `cancelled` with error `cancelled`.

Refusals: a job with no steps; a job that declares `service` containers (``native executor cannot run the services (<names>) of job <stage/job>: they run in the job's pod, which this executor does not have; use the kubernetes executor``). A job's `image` is ignored with a warning.

Limits: the log channel holds 1024 lines per job, the line queue 256; a subscriber that falls behind gets a `lagged` event (see [Events](events.md)). At start the service logs whether `git` is on `PATH` and warns about missing `cargo`, `docker`, `kubectl`, `anvil`, `riveter`.

## kubernetes

One `batch/v1` Job per conveyor job, created in `CONVEYOR_K8S_NAMESPACE` (default: the namespace file of the pod's service account, else `default`). Connection: `Client::infer` (in-cluster service account, or a kubeconfig). If it fails at start the service logs an error and installs an executor on which **every job fails** (`kubernetes executor cannot run anything: could not reach the cluster: ...`); it never falls back to `native`.

Objects, for a job with id `J`:

| Object | Name | Content |
|---|---|---|
| `Job` | `conveyor-<job id>` (lowercased, non-alphanumerics become `-`, at most 54 characters of the id) | labels `app.kubernetes.io/managed-by=conveyor`, `app.kubernetes.io/name=<name>`, `conveyor.forge/job-id=<J>`; `backoffLimit: 0`; `activeDeadlineSeconds` = the job's timeout (at least 1); `ttlSecondsAfterFinished` = `CONVEYOR_K8S_TTL_SECONDS`; pod `restartPolicy: Never`; `serviceAccountName` = `CONVEYOR_K8S_SERVICE_ACCOUNT` when set. |
| `Secret` | same name | created only when the job's environment is not empty (it always has the `CONVEYOR_*` variables, so in practice always): `stringData` = the whole job environment; referenced from the steps container with `envFrom`. Deleted with the job. |
| init container `checkout` | | image `CONVEYOR_K8S_GIT_IMAGE` (default `alpine/git:latest`); a script that does `git init`, `git remote add origin <url>`, `git fetch --depth 1 origin <sha>` (falling back to `git fetch origin <ref>`) and `git checkout --detach <sha>` into the shared `emptyDir` mounted at `/workspace`. The git credential, if any, is given only to this container as `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=http.extraheader`, `GIT_CONFIG_VALUE_0=Authorization: Basic ...`. |
| init containers for `service`s | the service's `name` | `restartPolicy: Always` (native sidecars, Kubernetes 1.29 or later), after the checkout; `startupProbe` is the service's `ready` command (`sh -c <command>` exec) or a TCP check on `port`, period 2 s, timeout 5 s, failure threshold `CONVEYOR_K8S_SERVICE_READY_SECS / 2` (at least 1). The service's `env` and `args` are written into the pod as given, not into the job's Secret. |
| container `steps` | | image = the job's `image`, else `CONVEYOR_K8S_DEFAULT_IMAGE` (default `alpine:3.22`); `sh -c <script>`, working directory `/workspace`; `securityContext.allowPrivilegeEscalation=false`. |

The script prints `##conveyor-step:<n>` on stderr before step `n`, runs the step quoted as one argv, and exits with the first non-zero status. Those markers are how the log follower learns which step is running and which failed; they are not shown in the log. When the job has files to hand back (declared artifacts and the test-report paths), a shell `trap ... EXIT` prints each existing file of at most 24 MiB as lines `##conveyor-file:<path>\t<base64>` (4096 base64 characters per line) on stdout, whatever the outcome, and the executor writes them into the conveyor workspace before the worker collects them. Those lines are consumed and never shown.

Lifecycle: create the Secret, then the Job; wait for the pod to leave `Pending` (up to `CONVEYOR_K8S_POD_START_SECS`, default 120, plus `CONVEYOR_K8S_SERVICE_READY_SECS` when the job has services; a stuck pod fails the job with Kubernetes' own reason); follow the pod's log; read the verdict from the Job's own status (`Succeeded` -> `success`, exit 0; `Failed` -> `failed` with the reason; the Job vanishing -> `cancelled` with `the cluster job was deleted`). On failure, and also when the steps fail, the tail of each service container's log is appended to the job's log under a `--- service <name> ---` heading. Cancel deletes the Job with background propagation; `forget` deletes the Job and the Secret (a failure to delete the Secret is logged at error level).

Required RBAC of conveyor's own service account in that namespace: `create`, `get`, `list`, `delete` on `jobs`, `pods`, `pods/log`, `secrets`.

*Unverified:* the repository's previous documentation states that the full round trip (pod scheduled, log followed, verdict read back) has not been exercised against a real cluster; the manifest construction is covered by `tests/unit/executors_manifest_tests.rs` and the unreachable-cluster path is tested.

## mock

`CONVEYOR_EXECUTOR=mock`: records what it was asked to do and returns scripted results; jobs report success without running anything. A warning is logged at start. For tests.

An unrecognised `CONVEYOR_EXECUTOR` value logs a warning and falls back to `native`; `k8s` is an alias for `kubernetes`; the value is case-insensitive.
