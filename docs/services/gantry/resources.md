---
title: Resources
parent: Gantry
grand_parent: Services
nav_order: 9
---

How Gantry decides what a package put in the cluster and what state each thing is in. Sources: `src/domain/{resources,actions,cluster,targets}.rs`. Routes: `GET /api/v1/resources`, `GET /api/v1/resources/yaml`, `POST /api/v1/resources/{delete,apply,edit,refresh}` (see [API](api.md)).

## Two sources

1. **The cluster** says what exists: every object that carries the label `riveter.forge/package` (riveter stamps it on what it installs). The version is the annotation `riveter.forge/package-version` (an annotation, because a label cannot hold `+`). An edit made through Gantry adds `gantry.forge/edited`.
2. **The inventory** says what the package declares: the list `kind`, `name`, `namespace`, `apiVersion` recorded from the last successful install or refresh. It holds names and kinds only, never a manifest or a value.

### Reading the cluster

`KubeCluster::list_resources` asks the API server which resource kinds it serves (discovery, kept for 60 seconds), keeps those that support `list`, drops `Event`, `Endpoints`, `EndpointSlice`, `Pod`, `ReplicaSet`, `ControllerRevision`, `Lease`, `Node`, `ComponentStatus` and `Secret`, and lists each remaining kind with the label selector (cluster-wide for namespaced kinds), up to 8 lists at a time. A kind the service account may not list (403) or that fails is simply absent: what Gantry can see is decided by its Role (`gantry-observer`), not by the code. With `GANTRY_NAMESPACES` set, namespaced objects outside the list are dropped; cluster-scoped objects are kept (so a package's roles are visible), though acting on them is refused while an allow-list is set. The CRD of a package therefore appears with no code change. Every request to `GET /api/v1/resources` repeats these lists (only discovery is cached), plus the registry catalog and the workload lists.

Per live object: `ready` is `readyReplicas/replicas` (replicas default 1) for Deployment and StatefulSet, `numberReady/desiredNumberScheduled` for DaemonSet, `done`, `failed` or `running` for a Job, nothing for the rest; `edited` is the presence of `gantry.forge/edited`; `owned` is a non-empty `ownerReferences` (a cert-manager Certificate made from an Ingress copies the Ingress's labels but is not the package's).

Workloads for the target list (`GET /api/v1/targets`) are read separately: Deployments, StatefulSets and DaemonSets with the label, per allowed namespace (or everywhere), carrying `version`, `desired` and `ready`.

## Merging: the five resource states

For each package (published, labelled objects present, or inventoried) the rows are built from the inventory first, then from live objects the inventory does not name.

| State | Meaning | How it arises |
|---|---|---|
| `synced` | there, as the package left it | declared and live, not edited; also a live object when the package has **no** inventory yet |
| `edited` | there, but changed through Gantry since the install; the next install overwrites it | live with `gantry.forge/edited` |
| `missing` | declared but not in the cluster | in the inventory, not live (deleted, or stopped). A `Job` of the inventory that is not live is **not** listed: a finished Job is cleaned up, and applying it again would only run it again |
| `extra` | in the cluster with the package label but not declared | live, not in the inventory, not owned by another object, and the package has an inventory (UI text: "not in package") |
| `hidden` | a Secret | declared Secret kinds, always: never looked at, never shown, never edited (UI text: "secret") |

A row is `editable` when it is live and not a Secret, and `workload` when its kind is Deployment, StatefulSet or DaemonSet. Rows are sorted workloads first, then by kind and name.

### Sync state of a package

`summary.sync`, computed from the rows and the target status:

| `sync` | When (first match) |
|---|---|
| `not_installed` | no row is present and the package has no recorded install |
| `missing` | at least one row is `missing` |
| `unlisted` | the package is running but no longer published |
| `out_of_sync` | at least one row is `edited`, or the target status is `update_available` or `mixed` |
| `synced` | otherwise |

`extra` rows are shown and counted but never make a package `out_of_sync`. `summary.not_ready` counts present workloads whose ready count is lower than the desired count; `update_to` is the offered version when the target status is `update_available`.

`installed` of a group is the version the running workloads agree on, else the version of the last recorded install; `offered` is the newest non-yanked version.

## The inventory

* **Recorded** when an operation ends `succeeded` and its plan contains an `install` or `check` step: the first such step's package and version, and the last line of the runner's log that starts `riveter-inventory:` followed by a JSON array of `{apiVersion, kind, name, namespace}`. An empty array or an unreadable line records nothing. A failed operation records nothing. A `check` (the dry-run render) prints the complete list, so a `refresh` replaces the inventory without changing the cluster.
* Stored per package in `gantry.inventory` (`version`, `resources` JSON); see [Data](data.md).
* Without one (a package installed before inventories existed) the page lists only live objects, as `synced`, and offers **Refresh**.
* With the simulated runner, the seed's `declared` items form the first inventory (see [Operations](operations.md#the-simulated-cluster)).

## Direct actions

They create an operation immediately (no plan to confirm); the operation and its log are the record. Each is one of the routes above.

| Action | Permission | Steps queued | Refused when |
|---|---|---|---|
| Edit | `deploy` | `apply_yaml`; then `rollout` (workload, 300 s) or `restart_users` (ConfigMap, 300 s) | the row is a Secret; the YAML names another kind, name or namespace than the row; the YAML is over 128 KiB, not parseable, has no `kind` or `metadata.name`, is a Secret, or its namespace differs from the step's |
| Delete | `scale` | `delete` of `<kind>.<group>/<name>` (the group disambiguates a CRD from a core kind of the same name) | Namespace, CRD, PersistentVolume, PersistentVolumeClaim, StorageClass; already `missing`; a Secret; a protected workload |
| Apply (one) / Apply missing | `scale` | `pull`, `check`, `install` of only those `kind/name` | no recorded install; nothing missing; not part of the package; no inventory yet |
| Refresh | `read` | `pull`, `check` | the package is not installed |
| Upgrade / Sync | `deploy` (`rollback` for an older version) | via a plan: see [Plans and operations](reconciler.md) | |

The edit applies the YAML **as written** with `kubectl apply -f <file>`; the file is named `edit-<first 12 hex of the SHA-256 of the YAML>.yaml` in the runner's package directory, so the same edit is the same file. Drift is allowed and shown: nothing keeps an edit; the package's next install overwrites it, and until then the row is `edited`. A ConfigMap edit restarts exactly the Deployments, StatefulSets and DaemonSets of its namespace whose pod template names the ConfigMap in an `env` (`valueFrom.configMapKeyRef`), an `envFrom` (`configMapRef`) or a volume, and waits for each (`rollout restart`, `rollout status`).

A Secret is never read, shown or edited by Gantry: a package's values are set through the package's own values (`values.yaml`, and `secrets.yaml` encrypted with age) and a new version.
