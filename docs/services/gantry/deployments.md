---
title: Deployments
parent: Gantry
grand_parent: Services
nav_order: 7
---

A deployment is a named set of workloads of a package that start and stop together (inference as one, training as another). Sources: `src/domain/deployments.rs`, `libs/rivet-package` (`Deployment`, `AlsoStops`, `DefaultState`). Routes: `GET /api/v1/deployments`, `POST /api/v1/deployments/{name}/plan` then `POST /api/v1/plans/{id}/confirm`.

## Declaration

In the package's `rivet.toml` as `[[deployment]]` (unknown keys are rejected):

| Key | Meaning |
|---|---|
| `name` | DNS-1123 label, unique across **every** package Gantry manages (a second package declaring the same name is ignored with a warning: the first, in package-name order, wins) |
| `description` | one line for the UI |
| `resources` | `kind/name` workloads (`deployment` or `statefulset`) in stop order: stopping goes down the list, starting applies them all; something that keeps another alive goes first |
| `default` | `running` (default) or `stopped`: the state when nobody has started or stopped it |
| `conflicts_with` | deployments that must not run at the same time |
| `also_stops` | `[{selector, namespaces}]`: pods nothing owns that hold what the deployment holds (switchboard creates vLLM pods directly), deleted by label and waited on |

A package that declares none is one deployment named after the package, made of its Deployments and StatefulSets (from the inventory and the running workloads); a package with none of those has no deployment.

## The view

`GET /api/v1/deployments` returns, per deployment:

| Field | Meaning |
|---|---|
| `observed` | from the cluster, over the listed workloads: none present -> `stopped` if the package is installed (something of it is running, or it has an inventory) else `absent`; some present but fewer than listed -> `partial`; all present with desired > 0 -> `running`; all present at zero -> `stopped`; all present but a mix -> `partial` |
| `desired` | what it is meant to be: the recorded state of the last **successful** start/stop (`deployment_states`), else `default` |
| `drift` | `observed` differs from `desired`; always true for `partial`; false for `absent` |
| `conflicts_with`, `conflicting_running` | the declared conflicts, and those (declared in either direction) that are `running` or `partial` now |
| `resources` | each listed workload: `present`, `desired`, `ready` |

## Actions

All go through a plan (the UI plans and confirms in one go; see [Client](client.md)).

| Action | Steps | Notes |
|---|---|---|
| **Stop** | `delete` each present workload in the listed order; `delete_pods` for each `also_stops`; `wait_gone` (selector `app.kubernetes.io/name=<workload>`, 300 s) for each workload and for each `also_stops` | Stopping is **deleting**, the Argo way: what the package declares stays known (the inventory), so nothing is lost and a start applies it again. Effect on success: `desired = stopped`. Refused: not installed (`absent`), already stopped (`observed` stopped and `desired` stopped), a protected workload |
| **Start** | `pull`, `check`, `install` of only this deployment's workloads (positional `kind/name` targets) at the installed version, waiting for them (`timeout_secs` 600) | Effect: `desired = running`. Needs the package at exactly one version (`<p> is installed at more than one version (or none can be read); bring the package to one version first`) and a namespace. Refused: absent; already running (and nothing to stop) |
| **Swap** | the Stop steps of every conflicting or named deployment that is not already stopped/absent, then the Start steps | A start with something to stop first: one operation, everything gone before anything starts. `touches` lists every package involved, so confirming needs `activate` on each |
| **Update** (a package upgrade) | an ordinary upgrade plan | A deployment whose `desired` is `stopped` (or, without a record, whose `default` is `stopped`) is left out of the install (`--except`), so it stays stopped; the inventory becomes the new version's, so a later start applies the new version |

The plan's `basis` is a signature `name=observed,...` of the involved deployments; confirming a plan whose signature has changed is `409 ... plan it again`. The plan's action is `stop`, `start` or `swap`; permissions: `scale` for the first two, `activate` for a swap.

State is recorded only when the operation **succeeds**, so after a failed or cancelled operation the recorded `desired` still describes what was last actually done, and `drift` flags the difference.

## Protected workloads

`GANTRY_PROTECTED` (default `forge/gantry,forge/forge-db,forge/redis,forge/gatehouse`, entries `namespace/name`) can never be stopped: not by the planner (`422 <ns>/<name> is protected and cannot be stopped: Gantry, its database and its login depend on it`), and not by a raw plan posted to `/api/v1/operations` or a direct delete, which are refused by `submit` for any step that deletes the workload, scales it to 0, or installs it with `replicas` 0. They can be upgraded.

## Limits

* The cluster has no GPU device plugin, so Kubernetes does not stop two GPU workloads colliding: `conflicts_with` is Gantry's knowledge, and something started outside Gantry can still collide.
* A `hostPath` GPU mount means "stopped" is confirmed by the pods being gone, not by the GPU reporting idle.
* `wait_gone` selects pods by `app.kubernetes.io/name=<workload>`, which riveter puts on every pod template; for a workload not built that way the wait ends at once.
