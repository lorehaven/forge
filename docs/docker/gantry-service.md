# Gantry Service

Gantry installs, upgrades, stops, starts and swaps what runs in the cluster, from a UI and an API, as versioned [`.rivet` packages](../libs/rivet-package.md) from Warehouse. It does not touch a workload itself: it records what was asked, has a short-lived **runner Job** do the work with `riveter` and `kubectl`, and reads the outcome back. Binary and crate: `gantry-service` (`docker/gantry-service`), which also builds the `gantry-runner` program the Jobs run. Design and decisions: [`plans/GANTRY_SERVICE.md`](../../plans/GANTRY_SERVICE.md).

## What it does

- **Applications.** The home page is one card per package, Argo CD style: its sync state (`in sync`, `out of sync`, `missing`, `not installed`, `unpublished`), the running version and the newer one on offer, and counts of what is missing, edited or not ready. Filter by sync state or name; applications needing attention come first. A card opens the application's own page.
- **Resources, grouped by package.** An application's page (`/ui/apps/<package>`) lists every resource of every kind each package put in the cluster (Deployments, Services, Ingresses, ConfigMaps, RBAC, CRDs ...), found by the `riveter.forge/package` label, beside what the package *declares* - so a resource that was deleted still shows, as `missing`, and can be applied again. States: `synced`, `edited` (changed from here since the install), `missing`, `not in package`, and `secret` (never looked at). Objects another object owns (a cert-manager Certificate made from an Ingress, which copies its labels) are not the package's and are not listed. A `not in package` resource is shown and counted, but does not make the application `out of sync`; only an edit, a missing resource or a newer version does.
- **Direct actions, no plan step.** *Edit* (live YAML), *Delete*, *Apply* (from the package), *Upgrade* and a deployment's *Start*/*Stop* run when clicked. Destructive ones ask first, naming the thing. The operation they create, with its log, is the record. (The API still has plan → confirm for anything that wants to look before running.)
- **Stop is delete, start is apply - the Argo way.** Nothing is scaled to zero and left half alive. A stopped deployment's workloads are gone from the cluster, and Gantry can bring them back because it remembers what the package declares.
- **Targets** - every package Warehouse publishes, beside the version the cluster runs: installed, newest published, and a status - `not_installed`, `current`, `update_available`, `ahead`, `mixed` (an install that did not finish) or `unlisted`.
- **No variables box.** A package carries its own configuration (`values.yaml`) and its secrets, encrypted (`secrets.yaml`), baked in when it is built, so a version is the whole of what it installs. Gantry opens the secrets with an age key it is given; it never stores or shows a value. To change one, change the overlay and publish a new version.

## Resources

The **inventory** is what makes this work. Every install (and every *Refresh*) has `riveter` print the complete list of what the package declares - `kind`, `name`, `namespace`, `apiVersion`, rendered with the variables given - as one `riveter-inventory:` JSON line, and the reconciler stores it per package once the operation succeeds. It holds names and kinds only: no manifest, no Secret value, nothing a variable supplied. Without one (a package installed before this existed) the page shows only what is live and offers *Refresh*, which reads the package and changes nothing.

The list merges the inventory with the cluster. The cluster side is **discovered**: the service asks the API server which kinds exist and lists each with the package label (concurrently, cached for a minute), so a CRD a package installs appears with no code change. What it may list is decided by its `gantry-observer` Role, not here - a kind it may not list is simply absent.

| Action | What it runs |
|---|---|
| **Edit** | The live YAML (status and server fields removed) in a text area. *Apply* writes it to a file and `kubectl apply`s it **as written**, with an `gantry.forge/edited` annotation naming who. A workload is then waited on (`rollout status`, which also covers the rollout the edit causes); for a **ConfigMap** the runner reads which Deployments, StatefulSets and DaemonSets in the namespace use it (an `env`, `envFrom` or volume naming it) and does `rollout restart` on exactly those, waiting for each. |
| **Delete** | `kubectl delete <kind.group>/<name>` (the group disambiguates a CRD from a core kind of the same name). Refused for Namespaces, CRDs, PVs, PVCs and StorageClasses - those take data or a whole namespace with them - and for the [protected](#deployments) workloads. |
| **Apply** | `riveter install <package at its installed version> <kind/name>`: only that resource, re-rendered with its variables. *Apply missing* does all that are missing at once. |
| **Upgrade / Sync** | The whole package to a version (the newest by default); going to an older one is a `rollback` permission. |

**Drift is allowed and shown.** An edit is not saved on top of the package: the package's next install overwrites it, and until then the resource is marked `edited`. A YAML edit is capped at 128 KiB (it travels in the Job's environment). A **Secret** is never read, shown or edited: it is listed from the inventory as `secret`, and its values are set through the package's values Secret (`gantry-values-<package>`, made by `riveter secrets sync`).

## Deployments

Shown on the application's page, as a labelled strip above the resources of the package that declares them: a dot, the name, what it conflicts with, and one *Start* or *Stop* button. *History* (header) lists every operation with its log; a banner on the home page links to whatever is running. The resource list filters by kind, state and a piece of the name.

Declared in the package's `rivet.toml` (`[[deployment]]`, see [rivet-package](../libs/rivet-package.md)), so the declaration travels with, and is versioned with, what it describes. A package that declares none is one deployment named after it.

| Action | What it does |
|---|---|
| **Stop** | **Delete** each workload in the listed order (what keeps another alive goes first - Sage before Switchboard), delete the `also_stops` pods by label, then wait until every pod is actually gone. What the package declares stays known, so nothing is lost. |
| **Start** | Apply *only that deployment's* workloads from the package at its installed version (`riveter install ... <targets>`) and wait for them. First stops anything it `conflicts_with` that is running - in either direction of the declaration. |
| **Swap** | A start that stops something first: one operation, everything stopped (and gone) before anything starts. |
| **Update** | An ordinary package upgrade. **A stopped deployment stays stopped**: its workloads are left out of the install (`--except`), so the update changes what it *will* run - the inventory is the new version's - and starts nothing. |

What each deployment is *meant* to be (`running`/`stopped`) is recorded in `gantry.deployment_states`, but only when the operation that changes it **succeeds** - a failure records nothing, and the list shows *meant to be* beside *is*, flagging any difference as drift (someone scaled it by hand). With nothing recorded, the package's `default` applies, which is how a first install brings B in at zero.

Two honest limits. The cluster has no GPU device plugin, so Kubernetes will not stop two GPU workloads colliding: `conflicts_with` is Gantry's own knowledge and something started outside Gantry can still collide. And a `hostPath` GPU mount means "stopped" is confirmed by the pods being gone, not by the GPU reporting idle. Waiting for a deployment's pods uses the `app.kubernetes.io/name=<workload>` label Riveter puts on every pod template; for a workload not made that way the wait is weaker (it ends at once).

**Protected workloads.** `GANTRY_PROTECTED` (default `forge/gantry`, `forge/forge-db`, `forge/redis`, `forge/gatehouse`, as `namespace/name`) can never be stopped - not by the planner and not by a raw plan posted to the API; both are refused. They can be upgraded.

## Upgrading Gantry itself

If a package's units include Gantry's own Deployment (`GANTRY_SELF_DEPLOYMENT`, default `forge/gantry`) the plan is ordered so a broken Gantry cannot strand the cluster: everything else is installed first (`--except deployment/gantry`, waiting at each gate), then Gantry **alone and last** (`--no-wait`), then a **guarded rollout** - `kubectl rollout status`, and on failure `kubectl rollout undo`, after which the operation is `rolled_back` with the reason. The runner Job outlives the pod it replaces, and the old pod keeps serving until the new one is ready, which is what makes this safe. The service that restarted cannot roll itself back; the runner does. The wait is `GANTRY_SELF_ROLLOUT_TIMEOUT_SECS` (default 300).

## Running an operation

```
UI / API ──► gantry-service ──creates──► runner Job ──► riveter install | kubectl scale ...
                 │  ▲                          │
                 │  └──── reads status/log ────┘
                 └──► Postgres (operations, plans, inventory, deployment states)
                 └──► Warehouse (what is published)
```

- **The plan is data, not a script.** Each step is one of a closed set (`pull`, `check`, `install`, `scale`, `delete`, `apply_yaml`, `restart_users`, `delete_pods`, `wait_gone`, `rollout`); every value that reaches a command line is validated and passed as its own argument, never through a shell, and a name that could read as a flag is refused.
- **Download first.** The runner's first step fetches every package version from Warehouse and verifies its digest; everything after runs from local files, so an operation never depends on a registry it may be restarting.
- **Check before change.** A `check` step renders the whole package as a client-side dry run, so a missing `${VAR}` - or a package with encrypted values and no key - fails the operation naming it, with the cluster untouched. Where the values come from: the package itself (`values.yaml`, then `secrets.yaml` opened with the key mounted from the `gantry-age-key` Secret at `/keys/age.key`, found by `RIVETER_AGE_KEY_FILE`). The mount is optional - a package with no encrypted values needs no key. The service never reads a Secret: its account cannot, and the key goes to the Job only as a mounted file.
- **One at a time.** At most one operation runs per scope (`cluster`), enforced by a partial unique index, not by the code. Others queue.
- **Survives a restart.** The reconciler (every 3 s and on every submit) adopts a `running` row whose runner is still going, records one whose runner finished, and starts again one whose runner is missing (the service died between marking it and creating the Job); steps are idempotent. A cluster that cannot be reached leaves an operation running rather than failing it. Cancelling a queued operation is immediate; a running one has its Job stopped (steps already run are not undone, and it says so).
- **The log is kept.** The runner's last 64 KiB is stored on the row, since the Job is cleaned up. The runner's final line, `GANTRY-RESULT: ok | failed <why> | rolled_back <why>`, is how a failed Job is told from a rolled-back one.
- **Waiting for pods to go** lists them and polls (`kubectl wait --for=delete` errors when nothing matches, and nothing matching is the goal).

## Running it locally

Everything that reaches outside the service is behind a trait and chosen by environment, so Gantry runs on a laptop with no cluster, and with one against a scratch namespace.

| Setting | Values | Meaning |
|---|---|---|
| `GANTRY_CLUSTER` | `in-cluster` (default in a pod), `kubeconfig`, `none` | How it reads the cluster. `none` is an in-memory fake. `GANTRY_KUBE_CONTEXT` pins a context. |
| `GANTRY_RUNNER` | `job` (default in a pod), `local`, `dry-run`, `simulate` | How operations run. `local` is a child process with your own `kubectl` and `riveter`; `dry-run` prints every mutating command and changes nothing; `simulate` (fake cluster only) applies each step to the in-memory cluster so you can watch start, stop, swap and upgrade take effect. |
| `GANTRY_FAKE_CLUSTER` | JSON file | What the fake cluster starts with: `{"workloads": [...], "resources": [...], "declared": [...]}` - live workloads, live resources of other kinds, and resources a package declares that are not live (stopped). Each package's declared set is what *apply* puts back, and is recorded as its inventory at start. |
| `GANTRY_NAMESPACES` | list | Allow-list: Gantry refuses to plan or run anything outside it. Empty means any. |
| `GANTRY_PACKAGES_DIR` | directory | `.rivet` files (e.g. `riveter pack --out`) instead of Warehouse. |
| `GANTRY_VALUES_DIR` | directory | Local runs read `<dir>/<secret>/env`. |
| `GANTRY_STATE_DIR` | directory | Where local runners keep their log and exit code; the files are what lets a restarted service find a runner. |

The default is the safe one: with nothing set a run is `none` + `dry-run`, and writing to a real cluster has to be said twice - `GANTRY_CLUSTER=kubeconfig` **and** `GANTRY_RUNNER=local`. `local` with anything but `kubeconfig` is refused (the fake cluster would report one thing while `kubectl` changed another), and `job` with no cluster is refused. Still required: Postgres (the `gantry` Foundry module) and Redis, like every service.

**To click through it:** `docker/gantry-service/local/` has a `compose.yaml` that runs Postgres, Redis, the migration and the service (your built binaries, no image build) with `GANTRY_CLUSTER=none GANTRY_RUNNER=simulate`, a seeded cluster (inference up, training down) and demo packages. `docker compose up`, then <http://localhost:11443/gantry/ui/home>; its README lists what to try.

```text
SERVICE_AUTH_ENABLED=false BASE_PATH=/gantry ... DATABASE_URL=... \
GANTRY_CLUSTER=kubeconfig GANTRY_RUNNER=local GANTRY_NAMESPACES=scratch \
GANTRY_PACKAGES_DIR=./packages gantry-service
```

## Configuration

| Variable | Default | |
|---|---|---|
| `WAREHOUSE_URL` | - | Warehouse including its base path. With `GATEHOUSE_URL` and `WAREHOUSE_CLIENT_SECRET` it authenticates as the `gantry-warehouse` client (client credentials, audience `warehouse` only); without them it asks anonymously, which only a Warehouse with auth off answers. |
| `WAREHOUSE_CLIENT_ID` | `gantry-warehouse` | |
| `GANTRY_NAMESPACE` | pod's own, else `forge` | Where runner Jobs go. |
| `GANTRY_RUNNER_IMAGE` | | The runner image (`riveter`, `kubectl`, `gantry-runner`), built from `ci/gantry-runner/Dockerfile` and tagged with Gantry's version. |
| `GANTRY_RUNNER_SERVICE_ACCOUNT` | `gantry-runner` | |
| `GANTRY_RUNNER_CREDENTIALS_SECRET` | `gantry-runner-credentials` | Registry credentials the Job fetches packages with (`RIVETER_WAREHOUSE_URL`, `RIVETER_GATEHOUSE_URL`, `RIVETER_CLIENT_ID`, `RIVETER_CLIENT_SECRET`). |
| `GANTRY_AGE_KEY_SECRET` | `gantry-age-key` | The Secret holding the age key (`age.key`) that opens a package's encrypted values. Not made by the overlay - it is the one thing that is not in git: `kubectl -n forge create secret generic gantry-age-key --from-file=age.key=$HOME/.config/riveter/age.key`. |
| `GANTRY_JOB_DEADLINE_SECS` | 3600 | A Job running longer is stopped. |
| `GANTRY_SELF_DEPLOYMENT`, `GANTRY_SELF_ROLLOUT_TIMEOUT_SECS`, `GANTRY_PROTECTED` | see above | |
| `DB_SCHEMA` | `gantry` | |

## API

All under `{BASE_PATH}/api/v1`, behind the realm's `Auth`.

| Method | Path | Needs | |
|---|---|---|---|
| `GET` | `/info` | - | service, version, caller, what they may do |
| `GET` | `/targets`, `/targets/{name}` | `read` | with versions |
| | `POST` | `/targets/{name}/plan` | `read` | `{version?}` - install, upgrade, reinstall or downgrade; stored |
| `GET` | `/resources` | `read` | every resource of every kind, grouped by package, with its state |
| `GET` | `/resources/yaml?package=&kind=&namespace=&name=` | `read` | the live YAML (never a Secret) |
| `POST` | `/resources/delete`, `/resources/apply`, `/resources/edit`, `/resources/refresh` | `scale`, `scale`, `deploy`, `read` | `{package, kind, name, namespace, apiVersion}`; apply with no resource applies everything missing; edit takes `yaml`. They run at once and return the operation |
| `GET` | `/deployments` | `read` | state, drift, conflicts |
| `POST` | `/deployments/{name}/plan` | `read` | `{action: start\|stop, stop?: [names]}` |
| `GET` | `/plans/{id}` | `read` | |
| `POST` | `/plans/{id}/confirm` | by action, below | creates the operation; `409` if stale or already confirmed |
| `GET`/`POST` | `/operations`, `/operations/{id}`, `/operations/{id}/cancel` | `read` / `deploy` | live runner output while running |
| `POST` | `/operations` | `deploy` (blanket) | a raw plan; validated, allow-listed and protected like any other |

The resource actions above run directly. A *plan* (the API's way of previewing; the UI does not use it) changes nothing, so reading is enough to make one; what they do is checked at confirmation, against **every package the plan touches**: `deploy` (install, upgrade, reinstall), `rollback` (to an older version), `scale` (start, stop), `activate` (a swap). Each is either blanket (`gantry:deploy`) or scoped to one package (`gantry:target:media:deploy`), and lists are filtered by what the caller may `read`. UI routes under `/ui` (the resource list, a package page, the YAML editor, History and one operation - the last refreshing itself while it runs) delegate login to Gatehouse and render in the realm's five locales.

## Security

Managing every overlay means the runner needs, in effect, cluster-wide write access. That is accepted and contained: the **service** account (`gantry-sa`) can create/read Jobs in its namespace and *get/list* a fixed set of kinds cluster-wide (workloads, ConfigMaps, Services, Ingresses, RBAC, cert-manager and Traefik resources, ... - the `gantry-observer` ClusterRole) - **never Secrets**, and nothing that changes anything - and the broad `gantry-runner` account is bound only to the Job, which exists only while an operation runs. Every operation records who asked, the plan and the outcome. A package's variables - and its secrets, which are age-encrypted inside the package - are opened only inside the runner Job, with a key mounted from a Secret; they never pass through the service, its database or its API.

## Tests

Unit tests run against in-memory parts (a fake cluster that scripts Jobs and workloads, a memory store, a memory registry): the planner, the deployments logic, the resource list and its direct actions (delete, apply, edit, refresh, the inventory), the reconciler including adoption after a simulated restart, the runner loop with a recording executor, the local executor with stand-in runner scripts, the API and the UI. `tests/cluster_it.rs` talks to a real cluster and is `#[ignore]`d (its module docs say how to run it): it checks the API server accepts the Job manifest, sees it fail, reads and deletes it. The real-Postgres behaviour (the one-running-per-scope index, a kill -9 mid-operation) and the real-cluster behaviour (install with the real `--inventory` line, every kind found by discovery, delete and apply back from the package, a live ConfigMap edit that restarted exactly the one Deployment using it, stop/start as delete/apply, update-while-stopped, stale-plan refusal, self-upgrade with rollback) were verified by hand in scratch namespaces, not in CI.
