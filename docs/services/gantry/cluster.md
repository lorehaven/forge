---
title: Cluster
parent: Gantry
grand_parent: Services
nav_order: 10
---

The cluster as it is, not as a package left it: nodes, pods, services, volumes, accounts and the rest, listed and shown the same way, with the few things that may be done to them. This is what the stand-alone Palantir dashboard did; Gantry replaces it. Sources: `service/src/domain/{browse,cluster_actions,exec}.rs`, `service/src/routers/api/cluster.rs`, `app/src/screens/cluster.rs`. Routes: see [API](api.md#cluster) and [openapi.yaml](openapi.yaml).

The packages side of Gantry ([Resources](resources.md)) looks at what carries the `riveter.forge/package` label. This side looks at **everything**, by kind.

## What is read

| Kinds, by heading | |
|---|---|
| Cluster | Node, Namespace, Event |
| Workloads | Pod, Deployment, StatefulSet, DaemonSet, ReplicaSet, Job, CronJob |
| Network | Service, Ingress, NetworkPolicy |
| Config | ConfigMap |
| Storage | PersistentVolumeClaim, PersistentVolume, StorageClass |
| Accounts | ServiceAccount, Role, RoleBinding, ClusterRole, ClusterRoleBinding |
| Custom | Certificate (cert-manager), Middleware and IngressRoute (Traefik) |

`GET /api/v1/cluster/kinds` returns the ones the cluster serves and the service account may list, in that order; a kind that is missing (no cert-manager, no Traefik) is simply absent. **Secrets are never listed, read or shown**: Gantry's Role cannot read one, by design, and a list would hand the values to the service.

Each list row (`Entry`) has the one word a person scans for (`status`), a second column (`detail`), the creation time, whether the status `attention` is a problem, and, for nodes and pods, `usage`:

| Kind | `status` | `detail` |
|---|---|---|
| Pod | the container's waiting or terminated reason (`CrashLoopBackOff`, `ImagePullBackOff`) over the phase; `Terminating` | `ready/total · restarts · node` |
| Deployment, StatefulSet, ReplicaSet | `ready/wanted` (a short one needs attention) | first image |
| DaemonSet | `ready/desired` | first image |
| Job | `Running`, `Complete`, `Failed` | first image |
| CronJob | `Active`, `Suspended` | the schedule |
| Node | `Ready` or `NotReady` | kubelet version and roles |
| Service | its type | cluster IP and ports |
| Ingress | none | the hosts |
| ConfigMap | none | the number of keys |
| PersistentVolumeClaim, PersistentVolume | the phase | capacity and class, or the claim |
| Certificate | `Ready` or `NotReady` | its Secret's name |
| Event | `Normal` or `Warning` | reason and message |

`usage` (`134m · 1.2Gi`) comes from metrics-server (`metrics.k8s.io`). Without it the column is empty and the overview says so.

### One object

`GET /api/v1/cluster/object` gives its YAML (status included, `managedFields` and `last-applied-configuration` removed), its events (newest first; failing to read events is not an error), a pod's container names (init containers first), `edit_yaml` (the YAML as it would be edited: status, ids and timestamps removed; empty for Nodes and Events) and `related`: the pods a Deployment, StatefulSet, DaemonSet, ReplicaSet, Job or Service selects (by `spec.selector`), or the pods on a Node, each with usage.

### Overview

`GET /api/v1/cluster/overview`: nodes ready of all, CPU and memory in use against what the nodes can allocate (`metrics` says whether usage was measured), pods running of all, the pods that need attention and the ten latest warning events.

### Services

`GET /api/v1/cluster/facade`: one link per host of every Ingress, at the path a person would open, `https` where the Ingress lists the host under `tls`, and `available` when a running pod matches the selector of the Service behind it. An Ingress with several paths gives one link: a path ending in `/ui` if there is one, else the shortest; paths that are machinery (a segment `api`, `assets`, `ws`, `index`, `share`, `admin`, `v2`, `token`, `health`, `static`, `favicon.ico`, or starting with `_`) are skipped, and a regular-expression path is cut back to its literal start (`/kindle(/|$)(.*)` is `/kindle`). An Ingress with only machinery paths gives none. The UI page is **Services**. (Palantir's version listed NodePort ports whose name contained "web ui" and an `ADDITIONAL_SERVICES` list; neither is carried over.)

### Logs

`GET /api/v1/cluster/logs?namespace&pod&container&tail&previous`: the last `tail` lines (default 500, at most 5000) with timestamps, of the first container unless one is named; `previous=true` is the container before its last restart.

## What may be done

Gantry's own service account is **read-only**. Changes are operations like any other (a plan of one or two steps, run by a runner Job, the log kept in History), so each one is on record.

| Route | Permission | Steps | Refused when |
|---|---|---|---|
| `POST /api/v1/cluster/delete` `{kind, namespace?, name}` | blanket `scale` | `delete` | Namespace, CustomResourceDefinition, PersistentVolume, PersistentVolumeClaim, StorageClass, Node, Event, Secret, ClusterRoleBinding; the object is not there; a protected workload |
| `POST /api/v1/cluster/scale` `{kind, namespace, name, replicas}` | blanket `scale` | `scale`, then `rollout` (300 s) | not a Deployment or StatefulSet; more than 50 replicas; zero for a protected workload |
| `POST /api/v1/cluster/edit` `{kind, namespace?, name, yaml}` | blanket `deploy` | `apply_yaml`, then `rollout` for a Deployment, StatefulSet or DaemonSet | a Secret; the YAML does not parse; it describes another kind, name or namespace than the one opened |

All three answer `201` with the operation. With `GANTRY_NAMESPACES` set, the plan is validated against that allow-list like every other. These routes need the **blanket** grant: a grant scoped to one package does not open the cluster. An edit is applied as written; a package's next install overwrites it.

## Shells

A shell in a pod is **off** unless the operator turns it on, and then only for the namespaces they name.

1. List the namespaces in `GANTRY_EXEC_NAMESPACES` (comma separated; empty is off).
2. Give `gantry-sa` the right to `create` `pods/exec` **in those namespaces only**, with a Role and RoleBinding each (an example is commented out at the end of `homecloud/overlays/forge/gantry/role.yaml.j2`). The service's cluster-wide role does not include it.

Then a caller with blanket `deploy` can open one:

| Route | Does |
|---|---|
| `GET /api/v1/cluster/exec/enabled` | the namespaces a shell may be opened in, for this caller (empty without `deploy`) |
| `POST /api/v1/cluster/exec` `{namespace, pod, container?}` | opens `/bin/sh` with a tty; `{"id"}`. `403` when the namespace is not listed |
| `POST /api/v1/cluster/exec/input` `{id, data}` | types into it |
| `GET /api/v1/cluster/exec/output?id&since` | what it printed from offset `since`: `{data, next, ended, closed}` |
| `POST /api/v1/cluster/exec/close` `{id}` | ends it |

How it works: the service opens the Kubernetes exec WebSocket itself (protocol `v4.channel.k8s.io`, bearer token or client certificate, TLS through `quench-tls`) and keeps what the pod prints in a buffer of 256 KB. The browser client has no WebSocket, so the page asks for new output every 700 ms. Offsets are absolute: a reader that fell behind resumes where the buffer starts; a multi-byte character cut in two by the end of the buffer waits for its other half. At most **8** shells are open at once; one nobody has touched for **10 minutes** is closed; a shell belongs to the person who opened it (anyone else gets `404`). Opening one is logged as a warning with the person and the pod. Limits: line-based like a terminal in a text box: no resize, no raw key presses; colour and cursor codes are removed for display.

## In the browser

| Route | Screen |
|---|---|
| `{base}/ui/cluster` | (the sidebar shows a submenu under "Cluster" on every cluster page: Overview, then every kind under its heading, the open one marked) overview cards (refreshed every 5 s), what needs attention, the latest warnings, the kinds by heading |
| `{base}/ui/cluster/{kind}` | the list, with a namespace select and a name filter, refreshed every 5 s |
| `{base}/ui/cluster/{kind}/view?namespace=&name=` | related pods, events, a pod's log and shell, delete and scale, the YAML (a form for whoever holds `deploy`) |
| `{base}/ui/services` | the links, refreshed every 10 s |
| `{base}/ui/profile` | who the caller is to Gantry, what they may do, the version |

## Role

The `gantry-observer` ClusterRole (in `homecloud/overlays/forge/gantry/role.yaml.j2`) lists what is read: besides the kinds a package installs, `nodes`, `persistentvolumes`, `events`, `pods/log`, `replicasets`, `storage.k8s.io/storageclasses` and `metrics.k8s.io` `nodes` and `pods`. Never `secrets`.
