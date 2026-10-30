---
title: Gantry
parent: Services
nav_order: 5
has_children: true
permalink: /services/gantry/
---

Gantry installs, upgrades, stops, starts and swaps what runs in the cluster, from a UI and an API, as versioned `.rivet` packages (see the `rivet-package` library). It never touches a workload itself: it records what was asked as an **operation**, has a short-lived **runner** do the work with `riveter` and `kubectl`, and reads the outcome back. Service crate `gantry-service` 0.2.2, which also builds the `gantry-runner` program the runner Jobs execute.

## Crates

All under `services/gantry/`.

| Crate | Version | Role |
|---|---|---|
| `gantry-service` | 0.2.2 | The HTTP service, the reconciler, the planner, the cluster and registry adapters. Library `gantry_service`; binaries `gantry-service` and `gantry-runner` (what a runner Job runs; it ships in the runner image beside `riveter` and `kubectl`). |
| `gantry-core` | 0.1.0 | No I/O. The wire models the client reads (`Group`, `Row`, `DeploymentView`, `Operation`, `StoredPlan`, `TargetView`, ...), the request bodies, the resource filter and the YAML edit draft. |
| `gantry-app` | 0.1.0 | The screens (`quench-ui`). Does no I/O: `Request` out, `Msg` in. |
| `gantry-web` | 0.1.0 | The wasm build over `fetch`. |

## Service facts

* Port 11443 and base path `/gantry` under foreman (HTTP redirect listener `0.0.0.0:11080`); needs gatehouse.
* Postgres (foundry module `gantry`; see [Data](data.md)) and Redis (sessions), like every service. With an in-memory database the process starts but store calls fail with `gantry needs Postgres; ...` (`503` when the store error reaches the handler directly, `500` when a planner wraps it).
* A package source: Warehouse (`WAREHOUSE_URL` with the client `gantry-warehouse`), or a directory of `.rivet` files (`GANTRY_PACKAGES_DIR`). With neither, the target list is empty and a warning is logged.
* A Kubernetes cluster to read (`GANTRY_CLUSTER`) and a way to run operations (`GANTRY_RUNNER`): by default a bare run has **no cluster and a dry-run runner** and can change nothing. See [Operations](operations.md#running-it-locally).
* Authentication: `Auth` wraps the whole prefix `{BASE_PATH}/api/v1`; every handler then checks `gantry:<action>` or `gantry:target:<package>:<action>`. See [Permissions](permissions.md).

## Concepts

| Term | Meaning |
|---|---|
| Package / target / application | The same thing seen three ways: a `.rivet` package (registry), a target (API: the package beside what runs of it), an application (UI: a card). |
| Inventory | What a package declares (kind, name, namespace, apiVersion), recorded from the `riveter-inventory:` line of the last successful install or refresh. It is why a deleted resource can still be listed and applied again. See [Resources](resources.md). |
| Deployment | A group of workloads of a package that can be started and stopped together, declared in the package's `[[deployment]]`. See [Deployments](deployments.md). |
| Plan | Steps stored for someone to look at and confirm. See [Plans and operations](reconciler.md). |
| Operation | One requested change, with state `queued`, `running`, `succeeded`, `failed`, `rolled_back` or `cancelled`. |
| Runner | A Kubernetes Job (or a local child process) that runs the steps of one operation. |

## Pages

| Page | Content |
|---|---|
| [API](api.md) and [openapi.yaml](openapi.yaml) | Every route. |
| [Resources](resources.md) | The five resource states, sync states, the cluster side (discovery, labels), the direct actions. |
| [Deployments](deployments.md) | Start, stop, swap, update while stopped, protected workloads. |
| [Plans and operations](reconciler.md) | Steps, validation, the reconciler, the runner, outcomes, self-upgrade. |
| [Permissions](permissions.md) | Grants and what each route and action needs. |
| [Configuration](configuration.md) | Every environment variable. |
| [Data](data.md) | Tables and migrations. |
| [Operations](operations.md) | Local-run modes, the simulated cluster, running in the cluster, security, tests. |
| [Client](client.md) | The browser client. |
