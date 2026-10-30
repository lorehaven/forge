---
title: Permissions
parent: Gantry
grand_parent: Services
nav_order: 6
---

What a caller needs for each route and action. Catalog entry (`services/gatehouse/service/config/permissions.toml`): service `gantry`, actions `read`, `deploy`, `scale`, `activate`, `rollback`, resource type `target` (a package). Token mechanics are in [Authentication and permissions](../../guides/authentication.md).

## Actions

| Action | Allows |
|---|---|
| `read` | list and read packages, resources, deployments, plans; make plans; refresh an inventory |
| `deploy` | install, upgrade or re-apply a package; edit a resource's YAML; submit a raw plan; cancel an operation |
| `scale` | stop and start a deployment; delete a resource; apply a resource or everything missing |
| `activate` | confirm a swap (a start that stops something else) |
| `rollback` | confirm a plan that goes to an older version |

Grants: a wildcard role (admin, or a client-credentials `service` token) holds everything; `gantry:<action>` is blanket; `gantry:target:<package>:<action>` is that action on one package. The templates give `viewer` and `editor` only `gantry:read` ("what Gantry can do to the cluster is granted deliberately, never by a template").

`can(action)` is true with auth off, else when the token has the blanket grant. `can_on_target(package, action)` is true with auth off, or the blanket grant, or `target:<package>:<action>`.

## Per route

| Route | Check |
|---|---|
| `GET /info` | none beyond authentication; `can` lists blanket grants only |
| `GET /targets`, `/resources`, `/deployments` | filtered to packages with `read` on the package |
| `GET /targets/{name}` | `read` on the package |
| `POST /targets/{name}/plan`, `POST /deployments/{name}/plan` | `read` on the package (the deployment's package) |
| `GET /plans/{id}` | `read` on the plan's target |
| `POST /plans/{id}/confirm` | the action's permission on **every** package in `plan.touches` (or the plan's target when empty): `rollback` for `downgrade`, `scale` for `start` and `stop`, `activate` for `swap`, otherwise `deploy` |
| `GET /resources/yaml` | `read` on the package (`editable` = `deploy`) |
| `POST /resources/delete`, `/resources/apply` | `scale` on the package |
| `POST /resources/edit` | `deploy` on the package |
| `POST /resources/refresh` | `read` on the package |
| `POST /operations`, `POST /operations/{id}/cancel` | blanket `deploy` |
| `GET /operations`, `GET /operations/{id}` | blanket `read` |

Consequences: a user with only `gantry:target:media:deploy` can plan and confirm upgrades of `media`, see only `media`, but cannot list or read operations (blanket read), cancel one, or submit a raw plan; the client tolerates the missing operation list (the "change in progress" banner is a hint). The permission for an action is checked only at confirmation, so planning is open to any reader. `refresh` needs only `read` yet queues a real operation (`pull` and a dry-run render).

## Machine identities

* `gantry-warehouse` (client credentials, audience `warehouse` only): how Gantry reads the package registry, with `WAREHOUSE_CLIENT_SECRET`.
* The runner Job authenticates to the registry with `RIVETER_CLIENT_ID`/`RIVETER_CLIENT_SECRET` from the credentials Secret (`GANTRY_RUNNER_CREDENTIALS_SECRET`), not with the service's identity.
* In the cluster the service account `gantry-sa` may create and read Jobs in its namespace and `get`/`list` a fixed set of kinds cluster-wide through the `gantry-observer` ClusterRole, never Secrets; the broad `gantry-runner` account is bound only to the runner Job. *Unverified here:* these manifests live in the `homecloud` repository (`overlays/forge/gantry/`), which this book does not read; the names come from the service's defaults and its previous documentation.
