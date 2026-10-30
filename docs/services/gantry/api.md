---
title: API
parent: Gantry
grand_parent: Services
nav_order: 1
---

The HTTP API of `gantry-service`, rendered from [openapi.yaml](openapi.yaml) (OpenAPI 3.1, checked against the route macros by `scripts/docs-check.py gantry`). All paths are under `BASE_PATH` (`/gantry` under foreman). 18 API routes and 21 `/ui` routes are registered; the three routes of `quench-starter` (`/health`, `/health/live`, `/health/ready`) are on the [Operations](operations.md#health) page.

## Conventions

* **Authentication.** `Auth` wraps the whole prefix `{BASE_PATH}/api/v1` (there is no exemption, and unmapped paths under it answer `401`). Bearer token or session cookie; audience `gantry`. See [Common service runtime](../common-runtime.md).
* **Permissions** are checked by each handler: `gantry:<action>` (blanket) or `gantry:target:<package>:<action>`; actions `read`, `deploy`, `scale`, `activate`, `rollback`. See [Permissions](permissions.md). List endpoints are filtered to the packages the caller may read instead of refusing.
* **Errors.** `{"error": "<text>"}`. By cause:

| Status | Source | Meaning |
|---|---|---|
| 401 | `Auth` | no valid token or session |
| 403 | handler | missing grant: `no read access to this target`, `no read access to this deployment`, `no <action> access to <package>`, `no deploy access here`, `no read access here`, or at confirmation `no <needed> access to <package>` |
| 404 | `PlanError::UnknownTarget` / `ActionError::NotFound` / handler | `no such operation`, `no such plan`, `no such target`, `no such deployment`, `no package named '<x>' is published`, `no package named '<x>'`, `<kind>/<name> is not part of <package>` |
| 409 | `PlanError::Stale`, or a unique violation | `this plan has already been confirmed`; `<package> is now at <v>, not <v> as when this was planned; plan it again`; `a record with that identity already exists` |
| 422 | `PlanError::Refused`, `ActionError::Refused`, `SubmitError::Invalid` | the request is understood and refused (invalid plan, protected workload, kind not deletable, missing field, already running/stopped, ...) |
| 502 | `PlanError::Registry` / `Cluster`, `ActionError::Planner` / `Cluster` | the package registry or the cluster could not be read or refused: `the package registry: <reason>`, `kubernetes: <reason>` |
| 503 | `GantryError::NotPostgres` | `gantry needs Postgres; this service is running against an in-memory database, where every operation and its history would be lost on restart` |
| 500 | store/serialisation | database error text; `a stored plan could not be read: <reason>` |

* **Times** are RFC 3339 strings; ids are UUID v4 strings.
* A body that is not valid JSON is rejected by the `Json` extractor before the handler (status and text come from `quench-http`).

## info

### GET /api/v1/info

`200` `{"service": "gantry", "version": "0.2.2", "actor": "dev", "can": ["read", "deploy", "scale", "activate", "rollback"]}`. `actor` is the token's `sub` (a person's username, or a client id for a machine) or `dev` with auth off. `can` lists only the actions held as **blanket** grants, in the order `read`, `deploy`, `scale`, `activate`, `rollback`; with auth off all five. The client reads it once, at start.

## targets

### GET /api/v1/targets

`200` `[Target]`: for every package that is published or has labelled workloads running, `name`, `description`, `namespace` (the default namespace of the newest version), `installed` (the one version all units run, else null), `offered` (the newest non-yanked published version), `status` and `units`. Filtered to packages the caller may read.

`status` (`TargetStatus`): `not_installed` (no units), `mixed` (units on different versions, or a single unit without a version), `unlisted` (running, not published), `current` (installed = offered), `update_available` (installed older), `ahead` (installed newer than the newest non-yanked version). Version order is semver precedence, then build metadata as text.

```json
{"name": "ml", "description": "Inference and training", "namespace": "ml", "installed": "1.0.0", "offered": "1.1.0",
 "status": "update_available",
 "units": [{"kind": "deployment", "name": "sage", "namespace": "ml", "version": "1.0.0", "desired": 1, "ready": 1}]}
```

### GET /api/v1/targets/{name}

Permission: read on the package. `200` the target plus `versions`: every published version, newest first, yanked ones included (`name`, `version`, `description`, `namespace`, `size_bytes`, `sha256`, `yanked`, `uploaded_by`, `manifest`). `404 no such target`.

### POST /api/v1/targets/{name}/plan

Body `{"version": "1.1.0"}` (version optional). Permission to plan: read on the package. Creates a **stored plan** and changes nothing. `201` with the `StoredPlan`.

The action follows the running version: `install` when nothing runs, else `upgrade` (chosen version newer), `reinstall` (equal) or `downgrade` (older). The steps are:

1. `pull` the package at the version; `check` it (a client-side dry run of the whole render);
2. `install` it with `timeout_secs: 600`, leaving out (`except`) the resources of every deployment that is meant to be `stopped` (so a stopped deployment stays stopped) and Gantry's own Deployment if the package contains it;
3. if the package contains Gantry's own Deployment (`GANTRY_SELF_DEPLOYMENT`): an `install` of that one resource alone with `no_wait: true`, then a `rollout` of it with `rollback_on_failure: true` and `timeout_secs` = `GANTRY_SELF_ROLLOUT_TIMEOUT_SECS`.

`touches` = `[name]`. The namespace comes from the version's manifest, else from the first running unit; neither is `422 <name> <version> declares no namespace, and nothing of it is running to learn one from`. Errors: `404 no package named '<name>' is published` (no versions); `422 version <v> is not published`; `422 version <v> has been yanked and cannot be installed` (unless it is the installed one); `422 every published version has been yanked; name one explicitly`; validation failures of the plan (`422`).

### GET /api/v1/plans/{id}

Permission: read on the plan's target. `200` the stored plan plus `step_lines` (the steps in words) and `can_confirm` (whether the caller could confirm it now).

### POST /api/v1/plans/{id}/confirm

Queues the operation. Order: plan exists (`404 no such plan`); the caller holds the permission its action needs on every package in `plan.touches` (`403 no <needed> access to <package>`; `needed` is `rollback` for `downgrade`, `scale` for `start` and `stop`, `activate` for `swap`, otherwise `deploy`); the plan was not confirmed (`409 this plan has already been confirmed`); the basis still holds (`409 <target> is now at <v>, not <v> as when this was planned; plan it again`; for a deployment plan the basis is the observed state of the involved deployments); then the plan is claimed once (a second concurrent confirm gets the same `409`), the operation is submitted (validation as for `POST /operations`; on refusal the claim is released and the answer is `422`) and the plan records the operation id. `201` with the `Operation`.

## resources

See [Resources](resources.md) for the meaning of the states.

### GET /api/v1/resources

`200` `[GroupView]`, one per package that is published, has labelled objects in the cluster or has an inventory. A `GroupView` is `package`, `description`, `installed` (the running version, else the version of the last recorded install), `offered`, `status` (`TargetStatus`), `inventoried`, `rows`, `summary`, `can`. A `Row` is `apiVersion`, `kind`, `name`, `namespace`, `state`, `ready`, `workload`, `editable`. Rows: workloads first, then by kind and name. `summary`: `sync`, `total`, `synced` (synced plus hidden), `edited`, `missing`, `extra`, `workloads`, `not_ready`, `update_to`. `can` is the subset of `read`, `deploy`, `scale` the caller holds for the package.

### GET /api/v1/resources/yaml

Query `package`, `kind`, `name` (required), `namespace`, `apiVersion` (optional). Permission: read on the package. `200` `{"yaml": "...", "editable": bool}` where `editable` means `deploy` on the package. The YAML has the server-added fields removed (`status`; in `metadata`: `managedFields`, `resourceVersion`, `uid`, `creationTimestamp`, `generation`, `selfLink`, `ownerReferences`, and the annotations `kubectl.kubernetes.io/last-applied-configuration` and `deployment.kubernetes.io/revision`). Errors: `404 no package named '<p>'`, `404 <kind>/<name> is not part of <p>`; `422 a Secret is never shown; its values are set through the package's values Secret`; `422 <kind>/<name> is not in the cluster; apply it first`; `502` (cluster: `the cluster has no kind <k>`).

### POST /api/v1/resources/delete

Body `ResourceBody` (`package`, `apiVersion?`, `kind`, `name`, `namespace?`). Permission: `scale` on the package. `201` with the queued `Operation` (title `Delete <Kind>/<name>`, kind `delete`, one `delete` step with `resource` = `<kind lower>.<api group>/<name>` and the namespace). `422`: missing `kind`/`name` (`which resource? (kind, name)`); Namespace, CustomResourceDefinition, PersistentVolume, PersistentVolumeClaim, StorageClass (`<Kind>/<name> is not deleted from here: it would take data or a whole namespace with it`); state `missing` (`... is already gone`); state `hidden` (`a Secret is not deleted from here`); a protected Deployment (`<ns>/<name> is protected and cannot be stopped: Gantry, its database and its login depend on it`).

### POST /api/v1/resources/apply

Same body. Permission: `scale`. With `kind` and `name`: that resource; without: every row of the package in state `missing`. `201` with the `Operation` (kind `apply`; steps `pull`, `check`, `install` of only those `kind/name` targets at the installed version, `timeout_secs: 600`). `422`: no recorded install (`<p> has no recorded install to apply from; install it first`), nothing missing (`nothing of <p> is missing`), not part of the package, no inventory yet (`<p> has no inventory yet, so it cannot tell what to apply`), no namespace.

### POST /api/v1/resources/edit

Body with `kind`, `name`, `yaml`. Permission: `deploy`. The YAML is parsed, must describe the same kind (case-insensitively), name and namespace as the resource named, may not be a Secret, at most 128 KiB, parseable with a `kind` and `metadata.name`. Gantry adds the annotation `gantry.forge/edited: "<actor> at <YYYY-MM-DD HH:MM:SS> UTC"` and queues an `apply_yaml` step, followed for a workload (Deployment, StatefulSet, DaemonSet) by a `rollout` wait of 300 s, or for a ConfigMap by `restart_users` (300 s). `201` with the `Operation` (kind `edit`).

### POST /api/v1/resources/refresh

Body `{"package": "ml"}`. Permission: **`read`** on the package. Queues `pull` and `check` of the installed version (kind `refresh`, title `Refresh <p>`); when it succeeds the `riveter-inventory:` line of the log replaces the package's inventory. `422 <p> is not installed, so there is nothing to look at`.

## deployments

### GET /api/v1/deployments

`200` `[DeploymentView]` for packages the caller may read: `name`, `declared`, `target`, `description`, `resources` (`kind`, `name`, `namespace`, `desired`, `ready`, `present`), `default`, `desired`, `observed` (`running`, `stopped`, `partial`, `absent`), `drift`, `conflicts_with`, `conflicting_running`. See [Deployments](deployments.md).

### POST /api/v1/deployments/{name}/plan

Body `{"action": "start" | "stop", "stop": ["other"]}`. Permission to plan: read. `201` with the `StoredPlan` (`action` is `stop`, `start` or `swap`). `404 no such deployment`; `422 '<x>' is not start or stop`; the refusals listed on the Deployments page. Confirm with `/api/v1/plans/{id}/confirm`.

## operations

See [Plans and operations](reconciler.md) for steps and states.

### POST /api/v1/operations

Body `{"kind"?: "custom", "title": "...", "plan": {"steps": [...], ...}}`. Permission: blanket `gantry:deploy`. `201` with the `Operation` (state `queued`). `422` for an empty title (`a title is required`), an invalid plan (`step <n> (<step in words>): <reason>`), or a stop of a protected workload.

```json
{"title": "Scale sage down", "plan": {"steps": [{"step": "scale", "namespace": "forge", "kind": "deployment", "name": "sage", "replicas": 0}]}}
```

### GET /api/v1/operations

Query `limit` (default 50, clamped 1..200). Permission: blanket `gantry:read`. `200` `[Operation]`, newest first.

### GET /api/v1/operations/{id}

Permission: blanket read. `200` the `Operation` plus `live_log` (the runner's output so far when the operation is not over and has a runner; else null) and `step_lines`. `404 no such operation`.

### POST /api/v1/operations/{id}/cancel

Permission: blanket `gantry:deploy`. `200` with the `Operation` after the request; an operation that is already over is returned unchanged. `404 no such operation`.

## auth and ui

The SSO routes behave as in [Common service runtime](../common-runtime.md); gantry uses the shared `/ui/status` handler of `quench-auth`. The page routes (`/ui/home`, `/ui/home/`, `/ui/apps/{name}`, `/ui/targets/{name}`, `/ui/operations`, `/ui/operations/{id}`, `/ui/plans/{id}`, `/ui/resource`) return the same HTML page for a valid session and `302` to the sign-in otherwise; `GET /ui` and `GET /ui/` redirect to `{BASE_PATH}/ui/home`; the five asset routes (`/ui/assets/quench.js`, `app.css`, `shell.js`, `favicon.png`, `app.wasm`) are public. See [The browser-client pattern](../../guides/browser-client.md).
