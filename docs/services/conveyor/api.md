---
title: API
parent: Conveyor
grand_parent: Services
nav_order: 1
---

The HTTP API of `conveyor-service`, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1, checked against the route macros by `scripts/docs-check.py conveyor`). All paths are under `BASE_PATH` (`/conveyor` under foreman). 47 API routes and 24 `/ui` routes are registered; the three routes added by `quench-starter` (`/health`, `/health/live`, `/health/ready`) are on the [Operations](operations.md) page.

## Conventions

* **Authentication.** `Auth` wraps the prefix `{BASE_PATH}/api/v1` (unmapped paths under it answer `401` rather than `404`) and accepts `Authorization: Bearer <access token>` or the session cookie; the audience must include `conveyor`. The only exemption is the prefix `{BASE_PATH}/api/v1/webhooks`. Failure: `401`; a browser request is redirected. See [Common service runtime](../common-runtime.md).
* **Permissions.** No route is gated by a blanket write middleware. Each handler checks `conveyor:<action>` (`read` or `write`) or the same action granted on a project node (`conveyor:project:<id>:<action>`), on the project the target belongs to or on any ancestor of it. See [Permissions](permissions.md). With auth disabled every check passes.
* **Filter, do not refuse.** `GET /repos`, `GET /runs` (without `repo_id`), `GET /credentials`, `GET /scan` and the two `/overview` reads return only what the caller may read; they do not answer `403`.
* **Errors.** `{"error": "<text>"}`. Database problems are mapped by one conversion: foreign-key violation `23503` becomes `400` (`the account making this request is not in the realm; sign in through gatehouse, which owns the estate's users` when the constraint is `repos_registered_by_fkey` or `subscriptions_username_fkey`, otherwise `the id given for a related record (project, repository, ...) does not exist`); unique violation `23505` becomes `409 a record with that identity already exists`; an in-memory database is `503`; unknown repository or run is `404`; everything else is `500` with the database error text.
* **Times** are RFC 3339 strings, ids are UUID v4 strings.
* **Bodies** are `application/json`. A body that is not valid JSON is rejected by the `Json` extractor before the handler (status and text come from `quench-http`; not specified here).

## projects

The organisational tree. A node may have children, an attached repository, both or neither.

### POST /api/v1/projects

Create a node. Permission: write on `parent_id`, or the blanket `conveyor:write` for a root node.

```json
{"name": "lorehaven", "parent_id": null}
```

`201` with a `Project`. `400 name is required` (empty after trimming). `403 no write access here`. `409` when a sibling has that name (unique `(parent_id, name)`; roots are unique among roots).

### GET /api/v1/projects

Query `parent_id` (string) and `all` (bool). With `all=true` every node at any depth; otherwise the children of `parent_id`, or the roots. Any authenticated caller; no project permission is checked. `200` with `[Project]`.

### GET /api/v1/projects/{id}

Permission: read on the node. `200` with a `Project` plus `path` (names from the root joined with `/`). `403 no read access here`, `404 no such project`.

### PATCH /api/v1/projects/{id}

Body `{name?, parent_id?, to_root}`. Permission: write on the node, and write on the destination for a move.

1. `name` (if present) is trimmed and applied first: `400 name cannot be empty`, `404 no such project`.
2. If `to_root` is true or `parent_id` is present, the node is moved (`to_root` wins): `403 no write access to the destination`, `404 no such project`, `400 a project cannot move under itself or one of its own descendants`.

The two steps are separate statements: a refused move leaves the rename applied. `200` with the `Project` after the change.

### DELETE /api/v1/projects/{id}

Permission: write on the node. `204`. `404 no such project`. `409 this project still has child projects; move or remove them first` or `409 a repository is still attached to this project; move or remove it first`.

## repos

### POST /api/v1/repos

Register a repository. Body `RegisterRepo`:

| Field | Type | Required | Notes |
|---|---|---|---|
| `provider` | `github` \| `generic` | no | Default `github`; anything else is `400 unknown provider '<x>'`. |
| `owner`, `name` | string | yes | Trimmed; empty is `400 owner and name are required`. This is the provider's identity of the repository (what a webhook names). |
| `clone_url` | string | yes | Not empty, not starting with `-`, no control characters; otherwise `400 invalid clone url ...`. |
| `default_branch` | string | no | Default `master`. |
| `project_id` | string | yes | `400 project_id is required` when empty; `400` (foreign key) when it does not exist. |

Permission: write on `project_id`. `201` with a `Repo`. `409` for a second registration of `(provider, owner, name)`.

```json
{"owner": "lorehaven", "name": "forge", "clone_url": "https://github.com/lorehaven/forge.git", "project_id": "0d3c2f0e-3f0e-4d6e-9d84-0a5b1f8a77c1"}
```

### GET /api/v1/repos

`200` with the `Repo`s the caller may read (all of them with the blanket `conveyor:read` or auth off; otherwise those under a project the caller holds `project:<id>:read` on, descendants included).

### GET /api/v1/repos/{id}

Permission: read on the repository's project. `200` `Repo`; `403 no read access to this repository`; `404 no such repository`.

### PATCH /api/v1/repos/{id}

Body `UpdateRepo` (`owner`, `name`, `clone_url`, `default_branch`, `project_id`, `enabled`, all optional; `provider` cannot change). Empty `owner`/`name`/`project_id` is `400`. Permission: write on the current project, and on the destination when `project_id` changes (`403 no write access to the destination project`). `200` with the `Repo`. `409` when the new `(provider, owner, name)` exists.

### POST /api/v1/repos/{id}/enabled

Body `{"enabled": bool}`. A disabled repository keeps its history and accepts no triggers. Permission: write. `200` `Repo`.

### DELETE /api/v1/repos/{id}

Permission: write. `204`. The foreign keys remove its runs (and through them jobs, steps, logs, artifact rows), secrets, credential, schedules and subscriptions.

## runs

### POST /api/v1/repos/{repo_id}/runs

Start a run by hand. The body is optional (empty means all defaults):

```json
{"git_ref": "refs/heads/master", "sha": null, "stages": ["security"]}
```

* `git_ref` defaults to `refs/heads/<default_branch>`; it is validated like a git ref (see [Pipelines](pipelines.md#validation-of-refs-shas-and-urls)).
* `sha` defaults to what `git ls-remote --exit-code <clone_url> <ref>` returns, using the repository's git credential if one is stored. A ref that does not exist is `502 <ref> does not exist in that repository`; other failures are `502 could not resolve <ref>: <stderr>`.
* `stages`: names trimmed, blanks and repeats dropped, each at most 128 characters without control characters, at most 64. An empty selection is `400 select at least one stage`; a bad name `400 \`<name>\` is not a stage name`; more than 64 `400 too many stages selected`.
* A manual run has no delivery id, so the same commit can be queued twice.

Permission: write. `202` with the queued `Run`. `403 no write access to this repository`, `404 no such repository`, `409 this repository is disabled`.

### GET /api/v1/runs

Query `repo_id`, `limit` (default 50, clamped to 1..500). With `repo_id`: that repository's runs (`403`/`404` as for the repository). Without: every run the caller may read. Newest first (`queued_at DESC`). `200` `[Run]`.

### GET /api/v1/runs/{id}

`200` with the run's fields at the top level plus `jobs` and `artifacts`:

```json
{
  "id": "5b0f2f2a-3f9e-4c57-8f36-1a2d0e2a9d11",
  "repo_id": "2c4f7b14-0a8e-4e55-9b4f-6a3e1d9c0b22",
  "trigger": "push", "git_ref": "refs/heads/master", "sha": "0123456789abcdef0123456789abcdef01234567",
  "message": "Fix the thing", "delivery_id": "8a1f0c4e-aaaa-bbbb-cccc-000000000001",
  "status": "success", "queued_at": "2026-10-30T10:15:00Z", "started_at": "2026-10-30T10:15:02Z", "finished_at": "2026-10-30T10:19:41Z",
  "claimed_by": null, "claimed_at": null, "attempt": 1, "error": null, "resumed_from": null, "stages": null,
  "jobs": [{"id": "...", "run_id": "...", "stage": "build", "name": "cargo", "needs": [], "status": "success", "exit_code": 0,
            "started_at": "2026-10-30T10:15:03Z", "finished_at": "2026-10-30T10:17:30Z", "error": null, "reused_from_run": null}],
  "artifacts": [{"id": "...", "run_id": "...", "job_id": "...", "kind": "file", "name": "summary.json", "version": null,
                 "uri": "https://warehouse.example/...", "digest": null, "created_at": "2026-10-30T10:17:31Z", "href": "https://..."}]
}
```

Jobs are ordered by `started_at` (unstarted last), then stage, then name. `href` is `uri` with the in-cluster warehouse address replaced by `WAREHOUSE_PUBLIC_URL` when set. Permission: read; a run whose repository was deleted is `403 no read access to this run`, not `404`. `404 no such run`.

### POST /api/v1/runs/{id}/cancel

Permission: write. `202` with no body: a `queued` run is `cancelled` at once; a `running` run is flagged and its worker stops it within about two seconds. `409 this run is not queued or running; there is nothing to cancel`.

### POST /api/v1/runs/{id}/restart

Permission: write. Only a `failed` or `cancelled` run: otherwise `409 only a failed or cancelled run can be restarted`. `202` with the new `Run` (same trigger, ref, sha, message and `stages`; `resumed_from` = the old run; no delivery id). Passed stages are carried over, see [Runs and the scheduler](scheduler.md#restarts).

### GET /api/v1/repos/{repo_id}/schedules

Permission: read. `200` `[StoredSchedule]` (`id`, `repo_id`, `expression`, `timezone`, `next_run_at`, `last_run_at`), ordered by `next_run_at`, then expression. Empty until a run of the default branch has read a pipeline that declares `on.schedule`.

### GET /api/v1/repos/{repo_id}/stages

Permission: read. `200` `[{"name": "build", "on_by_default": true}, ...]`, from the newest 30 runs of the repository that were not themselves limited to some stages, ordered so a stage follows what it `needs` (ties by name). `on_by_default` is false for a stage that the newest non-scheduled whole run left out because of its own `when`. Empty for a repository that never ran.

## logs

See [Events](events.md) for the stream formats.

### GET /api/v1/jobs/{id}/logs

Permission: read on the job's repository. `200` `[LogChunk]` (`seq`, `stream` `stdout|stderr`, `line`, `at`) as persisted when the job finished. `404 no such job`.

### GET /api/v1/jobs/{id}/stream

Query `format=text|html` (default `text`). `200 text/event-stream`. Frames: `id: <seq>`, `event: stdout|stderr`, `data: <line>`; then `event: done`, `data: end of log`. Headers `Cache-Control: no-cache`, `X-Accel-Buffering: no`.

### GET /api/v1/jobs/{id}/raw

`200 text/plain; charset=utf-8`, one line per log line.

## overview

### GET /api/v1/access

`200` `{"username": "<sub or dev>", "read": [project ids], "write": [project ids]}`. A project is listed when the caller may act on it directly or through an ancestor; with the blanket grant (or auth off) every project is listed.

### GET /api/v1/overview/runs

Query `project`. The newest 200 runs in scope, reduced to at most `CONVEYOR_HOME_MAX_RUNS_PER_REPO` (default 1) per repository and `CONVEYOR_HOME_RECENT_RUNS` (default 5) in total. `project` scopes to the node and everything under it; `404 no such project` when it does not exist. `200` `[Run]`.

### GET /api/v1/overview/history

Query `project`, `page` (from 1). Page size `CONVEYOR_RUNS_PAGE_SIZE` (default 25). `200` `{"runs": [...], "page": 1, "pages": 3, "total": 61}` (`pages` is at least 1).

## scan

See [Scans](scans.md).

### GET /api/v1/scan

`200` `[{"repo_id": "...", "checks": [{"kind": "lint", "passed": true, "findings": 0, "metric": null}]}]` for every readable repository. A check that does not exist for a repository is absent from its `checks`.

### GET /api/v1/repos/{id}/scan

Permission: read. `200` `{"run": <newest run or null>, "checks": [CheckDetail]}`. A `CheckDetail` has `kind`, `job_name` (`stage/job`), `passed`, `headline`, `findings` (`title`, `id`, `severity`, `date`, `location`, `extra`; at most 50), `metric`, `has_report`, `origin` (the run the result came from).

### GET /api/v1/repos/{id}/scan/findings

Permission: read. `200` `{"findings": [{severity, check, service, method, path, detail}], "run_id": "...", "created_at": "..."}`, most severe first. `404 no scanner findings`.

### GET /api/v1/repos/{id}/scan/{category}/report

Permission: read. `category` is `tests` or `security` (others: `404 no such report`). `200 text/html` with `content-security-policy: sandbox allow-scripts`, `x-content-type-options: nosniff`, `cache-control: private, max-age=60`; with `?download=<anything>` also `content-disposition: attachment; filename="<owner>-<name>-<category>-report.html"`. `404 this check has no report`; `502` text `the report could not be read`.

## secrets

Write-only sealed values. The value is never returned by any endpoint. Names hold letters, digits and underscores and do not start with a digit (`400 secret names hold letters, digits and underscores: '<name>' does not`); a value under 4 characters is `400 a secret shorter than 4 characters cannot be kept out of build logs, so conveyor will not store one`; no key is `503 CONVEYOR_SECRET_KEY is not set: conveyor cannot read or write secrets. Generate one with \`openssl rand -hex 32\`.`; a malformed key is `503 CONVEYOR_SECRET_KEY is not 32 bytes of hex or base64: <reason>`.

| Route | Permission | Result |
|---|---|---|
| `GET /api/v1/secrets` | blanket `conveyor:read` | `[SecretRef]` of estate-wide secrets. |
| `PUT /api/v1/secrets/{name}` | blanket `conveyor:write` | body `{"value": "..."}`; `200 SecretRef`; replaces. |
| `DELETE /api/v1/secrets/{name}` | blanket `conveyor:write` | `204`, or `404 no such secret`. |
| `GET /api/v1/repos/{repo_id}/secrets` | read on the project | `[SecretRef]`. |
| `PUT /api/v1/repos/{repo_id}/secrets/{name}` | write on the project | as above, per repository. |
| `DELETE /api/v1/repos/{repo_id}/secrets/{name}` | write on the project | `204` / `404`. |

`SecretRef` = `{id, repo_id (null for estate-wide), name, created_by, created_at, updated_at}`. A repository secret overrides an estate-wide one of the same name for that repository's jobs. The repository secret named `WEBHOOK_SECRET` is the secret that repository's webhooks are signed with.

## credentials

Git credentials for checkouts. At most one per project and one per repository; the repository's wins, otherwise the nearest ancestor project's. Body of `PUT`:

```json
{"name": "github_read", "kind": "http_token", "username": "ci-bot", "token": "ghp_exampletoken"}
```

`kind` defaults to `http_token`, the only accepted value (`400 unsupported credential kind '<x>': only 'http_token' today`). `name` follows the secret-name rule. `username` and `token` must contain no control character (`400 a username or token cannot contain a newline or other control character`); a token under 4 characters is `400 a token shorter than 4 characters is not worth storing encrypted`. The key is `CONVEYOR_CREDENTIAL_KEY` (`503` when missing or malformed, same texts as above with that name). The answer is a `Credential`: `{id, project_id, repo_id, name, kind, username, preview, created_by, created_at, updated_at}` where `preview` is `••••…` and the last four characters of the token.

| Route | Permission | Result |
|---|---|---|
| `PUT /api/v1/projects/{project_id}/credentials` | write on the project | `200 Credential` |
| `GET /api/v1/projects/{project_id}/credentials` | read on the project | `200 Credential` or `200 null` |
| `DELETE /api/v1/projects/{project_id}/credentials` | write on the project | `204` / `404 no credential set` |
| `PUT /api/v1/repos/{repo_id}/credentials` | write on the project of the repository | `200 Credential` |
| `GET /api/v1/repos/{repo_id}/credentials` | read | `200 Credential` or `200 null` |
| `DELETE /api/v1/repos/{repo_id}/credentials` | write | `204` / `404 no credential set` |
| `GET /api/v1/credentials` | filtered | `200 [Credential]` of those the caller may read (`503 could not list credentials` on a database error) |

A project route with an unknown project id (and a caller holding the blanket grant) reaches the database and fails on the foreign key; the credential routes map any database error to `500` with its text. A repository route with an unknown id is `404 no such repository`. Credential errors: bad name, bad material or a short token are `400`; a missing or malformed key and an in-memory database are `503`.

## subscriptions

Following a repository or project. A project covers every repository under it. What is sent and when is on the [Scheduler](scheduler.md#run-result-mail) page.

| Route | Permission | Result |
|---|---|---|
| `GET /api/v1/subscriptions` | any | `200 [Subscription]` of the caller (`id`, `repo_id`, `project_id`, `created_at`). |
| `PUT /api/v1/repos/{id}/subscription` | read on the project | `201 {"subscribed": true}` when created, `200` when already followed; `404 no such repository`. |
| `DELETE /api/v1/repos/{id}/subscription` | none | `200 {"subscribed": false}` always. |
| `PUT /api/v1/projects/{id}/subscription` | read on the project | `201` / `200`; `404 no such project`. |
| `DELETE /api/v1/projects/{id}/subscription` | none | `200 {"subscribed": false}` always. |

## webhooks

### POST /api/v1/webhooks/{provider}

`provider` is `github` or `generic` (`404 unknown provider '<x>'`). The checks run in this order, which is also the order of the answers:

1. The provider parses the delivery. GitHub needs the headers `X-GitHub-Event` and `X-GitHub-Delivery` (`400 missing <header>`). `push` events are read; a deleted branch (`deleted` true or `after` all zeros) is ignored. `pull_request` events with action `opened`, `reopened`, `synchronize` or `ready_for_review` are read; other actions and other events (`ping`, ...) are ignored. A body that cannot be read is `400 <provider> sent an event conveyor cannot read: <reason>`. An ignored event is `202 {"ignored": "nothing to build for this event"}`.
2. The event is validated: ref and sha as for a manual run, a non-empty delivery id, owner and name (`400`).
3. The repository is looked up by `(provider, owner, name)`: `404 <owner>/<name> is not registered with conveyor`.
4. The secret: the repository's `WEBHOOK_SECRET` (needs `CONVEYOR_SECRET_KEY`), else `CONVEYOR_WEBHOOK_SECRET`. Without one: `503 conveyor is not configured to accept webhooks for this repository`.
5. The signature: HMAC-SHA256 of the raw body, hex, header value `sha256=<hex>`. GitHub: `X-Hub-Signature-256`. Generic: `X-Conveyor-Signature-256`. Wrong or missing: `401 {"error": "bad signature"}`.
6. A disabled repository: `202 {"ignored": "this repository is disabled"}`.
7. A pull request from a fork (head repository differs from the base, or is gone) unless `CONVEYOR_ALLOW_FORK_PR`: `202 {"ignored": "pull requests from forks are not built"}`. A fork's ref is recorded as `refs/pull/<number>/head`.
8. A tag push (`refs/tags/...` on a `push`) unless `CONVEYOR_BUILD_TAGS`: `202 {"ignored": "tag pushes are not built (CONVEYOR_BUILD_TAGS is off)"}`.
9. The run is queued with `delivery_id` = GitHub's delivery id (or the generic payload's). A new run is `202` with the `Run`; a redelivery is `200` with the run it already made.

Generic payload (`GenericEvent`):

```json
{"delivery_id": "b7e2-0001", "owner": "lorehaven", "name": "forge", "ref": "refs/heads/master", "sha": "0123456789abcdef0123456789abcdef01234567", "message": "Fix the thing", "event": "push"}
```

`event` is `push` (default) or `pull_request`; any other value is `400`. A generic delivery is never a fork. Only the subject line (first line) of a GitHub head commit message is kept. For a GitHub pull request the message is the pull request title and the ref is `refs/heads/<head ref>` (or `refs/pull/<n>/head` for a fork).

## auth and ui

The six SSO routes and the page and asset routes of the browser client. The SSO routes behave as described in [Common service runtime](../common-runtime.md); conveyor's `GET /ui/status` is its own handler with the same output. The page routes (`/ui/home`, `/ui/home/`, `/ui/projects/{id}`, `/ui/runs`, `/ui/runs/{id}`, `/ui/credentials`, `/ui/repos`, `/ui/repos/{owner}/{name}/edit`, `.../run`, `.../scan`, `.../scan/{category}`) all return the same HTML page for a valid session and `302` to the sign-in without one; `GET /ui` and `GET /ui/` redirect to `{BASE_PATH}/ui/home`. The five asset routes (`/ui/assets/quench.js`, `app.css`, `shell.js`, `favicon.png`, `app.wasm`) are public; see [The browser-client pattern](../../guides/browser-client.md).
