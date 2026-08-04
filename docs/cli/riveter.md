# Riveter

Riveter is a Kubernetes manifest tool, powered by Rust and `minijinja` templates. Overlays (per-environment YAML) declare resources by kind; Riveter renders them into full Kubernetes manifests and can apply, diff, delete, or prune them against a cluster with `kubectl`. It exists so that Kubernetes environments in this workspace are declared once, in a compact `snake_case` overlay format, rather than as hand-written YAML duplicated per environment.

## Features

- Jinja2-style templating (`minijinja`) covering most core Kubernetes kinds plus Traefik, cert-manager, Gateway API, and RBAC resources, with a `raw` escape hatch for anything else.
- Environment management (`env list|set|show`), with the active environment recorded in `.riveter.toml` and overridable per invocation with `--env`/`-e` or `$RIVETER_ENV`.
- Scoped operations (`mutable`/`immutable`/`all`) so `render`/`apply`/`delete` can skip resources marked immutable by default.
- Targeting: any command can act on a `kind[/name]` subset, with `*`/`?` wildcards and kind aliases (`sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, ...).
- `kubectl` integration for `apply`, `diff`, and `delete`, including per-rollout readiness waiting on `apply`.
- `prune`, which finds cluster resources labelled `app.kubernetes.io/managed-by: riveter` for the environment that the overlay no longer declares, and removes them.
- `images`, which scans overlay templates for `image:` tags and checks the registry for newer compatible tags, optionally rewriting the templates in place.
- Variable substitution from an environment's own `.env` file (`${NAME}`, with `$${NAME}` as an escape).
- An interactive REPL (the default when run with no arguments) with the same command set, aliases, and help text as the CLI.

## Requirements

- `kubectl` on `PATH` for `apply`, `diff`, `delete`, and `prune`.
- A `kubectl` context available for whichever cluster an environment targets (or `kube_context` pinned in the overlay).

## Usage

```bash
riveter env set prod
riveter list
riveter apply deployment/api
riveter diff
riveter apply --scope all       # include immutable resources, e.g. to create a namespace
riveter prune --dry-run
riveter images
```

### Commands

| Command | Aliases | Purpose |
|---|---|---|
| `env <list\|set\|show>` | | Manage environments |
| `list [--scope ...] [target...]` | `ls` | List the resources an environment declares |
| `render [--scope ...] [target...]` | `r` | Render manifests into `manifests/` |
| `apply [--dry-run] [--no-wait] [--timeout <s>] [--scope ...] [target...]` | `a` | Render and apply via `kubectl` |
| `diff [--scope ...] [target...]` | `df` | Show what `apply` would change via `kubectl diff` |
| `delete [--scope ...] [target...]` | `d`, `del` | Render and delete via `kubectl` |
| `prune [--dry-run]` | | Delete cluster resources the overlay no longer declares |
| `images [--update] [--overlays-dir <dir>] [--registry-auth ...]` | | Check/update deployment image tags |
| `repl` | | Enter the interactive shell (CLI-only; also the default with no arguments) |
| `help [command]` | `h` | Show the command tree, or detail for one command |

`render`, `apply`, and `delete` default to `--scope mutable`, which skips resources marked immutable — so a `render` previews exactly what an `apply` would send. Use `--scope all` to include everything. `list` always shows every resource and marks each one's lifecycle. `--env`/`-e` beats `$RIVETER_ENV`, which beats whatever `env set` last recorded; both check the overlay exists before doing anything. Whatever a scope leaves out is reported rather than silently dropped:

```
ok    rendered 3 resource(s) to manifests/prod-manifests.mutable.yaml
warn  1 resource(s) outside this scope: namespace/prod-ns — `--scope all` includes them
```

### Resource authoring

Resources live under `overlays/<env>/overlay.yaml` as a `resources:` list, each with a `kind` and (except `namespace`, which takes the overlay's `namespace_name`) a `name`. No two resources may share a `kind/name` — both a nameless resource and a duplicate are rejected before anything is rendered, since a nameless resource is otherwise refused by the cluster with a far less obvious message, and a duplicate would silently overwrite its twin on apply.

```yaml
namespace_name: prod
kube_context: prod-cluster

resources:
  - kind: statefulset
    name: pg
    image: postgres:17
    replicas: 3
    port: 5432
    env_vars:
      POSTGRES_DB: forge
    env_refs:
      - name: POSTGRES_PASSWORD
        secret: { name: pg-secret, key: password }
      - name: POD_IP
        field: status.podIP
    probes:
      readiness:
        tcp_socket: { port: 5432 }
    resources:
      limits:
        cpu: "2"
        nvidia.com/gpu: 1
    volume_claim_templates:
      - name: data
        storage: 20Gi
```

Pod-based kinds (`deployment`, `statefulset`, `daemonset`, `replicaset`, `pod`, `job`, `cronjob`) share one pod-spec implementation, support single- or multi-container shorthand (or `containers:`/`init_containers:` for more than one), and accept an overlay-level `defaults:` block for `container_name`, `service_account`, `pull_policy`, `restart_policy`, and `host_path_type`. Mark a resource `immutable: true` (or `lifecycle: immutable`, `static`) to exclude it from the default scope and protect it from `delete`.

Overlay keys are `snake_case` and map onto Kubernetes' `camelCase` fields; fields with no fixed shape (`tolerations`, `affinity`, `topology_spread_constraints`, HPA `metrics`/`behavior`, NetworkPolicy `ingress`/`egress`, CRD `versions`, webhook `webhooks`) are passed straight through in Kubernetes' own `camelCase` spelling.

### Targeting individual resources

Without targets, every command acts on the whole environment. Pass one or more `kind[/name]` targets to act on a subset instead:

```bash
riveter list                            # what is in this environment?
riveter apply deployment/api            # just that deployment
riveter apply deployment/api service/api
riveter apply statefulset               # every statefulset
riveter apply '*/api'                   # everything named api
riveter render 'deployment/api-*'       # glob on either half
riveter delete --scope all namespace/prod
```

Both halves accept `*` and `?` wildcards, matching is case-insensitive, and kind aliases work — `sts/pg` selects a `kind: statefulset` resource. Quote patterns so the shell does not expand them. Targets are still filtered by `--scope`: naming an immutable resource while the scope excludes it is an error telling you to pass `--scope all`, rather than silently skipping it. A target that matches nothing is an error listing the available resources, so a typo cannot quietly become a no-op apply. Targeted renders are written to `manifests/<env>-manifests.selection.yaml`, so they never overwrite the full `manifests/<env>-manifests.yaml`.

### Binding an environment to a cluster

An overlay can name the kubectl context it deploys to (`kube_context` in the example above), so which cluster gets hit is a property of the environment rather than of whatever `kubectl config use-context` was run last. `apply` and `delete` pass `--context <name>` to kubectl and report the target before acting (`context  prod -> prod-cluster`). An overlay that pins nothing still works, but warns and names the cluster it is about to use instead. `kube_context` accepts `${VAR}` like any other value, and is consumed by riveter rather than rendered into the manifests.

### Seeing and finishing a change

`render` shows what would be *sent*; `diff` shows what would *change*, by handing the rendered file to `kubectl diff`. `apply` then waits for every Deployment, StatefulSet and DaemonSet it touched to become ready before reporting success — kubectl accepting a manifest only means the API server stored it, so without this a rollout that never starts a healthy pod still looks like a successful deploy. Use `--no-wait` to return as soon as kubectl accepts the manifests, and `--timeout <seconds>` to change the per-rollout budget (default 300s).

### Removing what the overlay dropped

`delete` only removes what the overlay still declares, so removing an entry from an overlay would otherwise leave the live resource running forever, invisible to riveter. `prune` closes that gap: every template stamps `app.kubernetes.io/managed-by: riveter` alongside `env: <env>`, so `riveter prune [--dry-run]` can ask the cluster what it owns and compare. Prune compares against the **whole** overlay, not the current scope, so a resource left out by `--scope` is never mistaken for one the overlay dropped. Two kinds are never pruned: `namespace` (deleting one takes everything inside it) and `raw` (its labels come from the overlay rather than from riveter). Resources created before the managed-by label existed are invisible to `prune` until they are applied once more.

### Bootstrapping an environment

Marking `namespace` immutable protects it from `delete`, but it also means the default `--scope mutable` will not create it. Rather than let every namespaced resource fail with its own `namespaces "prod-ns" not found`, `apply` checks the namespace first and, if it's missing, errors telling you to run `riveter apply --scope all` to create it. The check is skipped for `--dry-run`, and a cluster riveter cannot reach is not treated as a missing namespace — the apply proceeds and reports the real error.

### Variables

Overlay values may reference variables from the environment's own `.env` (`overlays/<env>/.env`) with `${NAME}`. A reference with no definition is an error naming every missing variable and where it appears — an undefined `${VAR}` left as-is would reach the cluster as that literal string, which for a Secret means shipping the placeholder as the value. Write `$${NAME}` for a reference riveter should leave alone (renders as literal `${NAME}`), for when something later — a shell in a container `command`, another templating pass — is meant to expand it instead. An environment reads only its own `.env`: there is deliberately no fallback to a shared file, since that would let an environment resolve a variable from a file belonging to a different environment, quietly rendering production with development's credentials. If several environments share a value, define it in each `.env`.

### Secrets on disk

Rendering writes plaintext: a `secret`'s `string_data` lands in `manifests/<env>-manifests.yaml` as typed. A manifest that carries a Secret — including one emitted through `raw` — is therefore written `0600`, readable only by the user who rendered it; manifests without Secrets keep the usual mode. File permissions protect against other users on the machine and do nothing against `git add -A`, so riveter also writes `manifests/.gitignore` ignoring everything in the directory (an existing `.gitignore` there is left alone). Prefer `env_refs` pointing at a Secret managed outside riveter over putting live credentials in an overlay.

### Checking for image updates

`riveter images` scans every `deployment*.yaml.j2` overlay template for `image:` lines and checks each registry for a newer tag with the same prefix/suffix and at least as many version components. A floating tag (`latest`, `stable`, `edge`, `main`, `master`, `dev`, `nightly`) is reported but never compared:

```bash
riveter images             # list available updates
riveter images --update    # rewrite templates in place to the newest compatible tag
```

Registry credentials are collected in ascending precedence: Docker's own config, then `RIVETER_REGISTRY_AUTH` (or `RIVETER_REGISTRY_USERNAME`/`RIVETER_REGISTRY_PASSWORD`), then repeatable `--registry-auth REGISTRY=USER:PASS` flags.

### Interactive REPL

Run `riveter` or `riveter repl` with no arguments to enter the interactive shell. REPL commands accept `--scope mutable` and `--scope=mutable` alike, matching what clap accepts on the CLI; anything else beginning with `-` is rejected rather than ignored, so a typo such as `apply --dry-runn` is an error instead of a live apply. `help` prints the full command tree with each command's subcommands and options nested beneath it; `help <command>` adds prose, the scope reference, target syntax and worked examples for one command; `help targets` prints the target syntax on its own. `riveter --help` prints that same tree — generated from one table shared by both surfaces, minus `exit` and plus `repl` — and the aliases work on the CLI too (`riveter ls`, `riveter a --dry-run deployment/api`, `riveter h apply`).

## Templates

Templates live in `src/templates/` and are embedded into the binary at compile time. A resource's `kind` is lowercased and matched against `<kind>.yaml.j2`, so `kind: statefulset` and `kind: StatefulSet` both render `statefulset.yaml.j2`.

### Supported kinds

| Group | Kinds |
| --- | --- |
| Workloads | `deployment`, `statefulset`, `daemonset`, `replicaset`, `pod`, `job`, `cronjob` |
| Config & storage | `configmap`, `secret`, `pv`, `pvc`, `storageclass` |
| Networking | `service`, `ingress`, `ingressclass`, `networkpolicy`, `endpoints`, `endpointslice`, `gateway`, `httproute` |
| Traefik | `ingressroute`, `middleware` |
| Scaling & scheduling | `horizontalpodautoscaler`, `poddisruptionbudget`, `priorityclass`, `runtimeclass` |
| Policy & quota | `namespace`, `resourcequota`, `limitrange` |
| RBAC | `serviceaccount`, `role`, `rolebinding`, `clusterrole`, `clusterrolebinding` |
| API extension | `customresourcedefinition`, `apiservice`, `mutatingwebhookconfiguration`, `validatingwebhookconfiguration` |
| cert-manager | `certificate`, `issuer`, `clusterissuer` |
| Escape hatch | `raw` |

Shorthand aliases: `sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, `persistentvolume`, `persistentvolumeclaim`.

For a kind riveter has no template for (vendor CRDs, a brand-new API), `raw` emits its `manifest` block verbatim — `${VAR}` substitution still applies:

```yaml
- kind: raw
  name: allow-dns
  manifest:
    apiVersion: cilium.io/v2
    kind: CiliumNetworkPolicy
    metadata:
      name: allow-dns
      namespace: ${NAMESPACE}
    spec:
      endpointSelector: {}
```

### Pod-based kind defaults

A container is named after its resource, and no `serviceAccountName` is emitted unless something asks for one. Three kinds carry a default beyond that:

| Kind | Default |
| --- | --- |
| `deployment` | `imagePullPolicy: Always` |
| `job` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type: File` |
| `cronjob` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type:` unset |

Every other pod-based kind uses `imagePullPolicy: IfNotPresent` and hostPath `type: DirectoryOrCreate`.

An overlay can set its own fallbacks for every pod-based resource with a top-level `defaults:` block, which a resource may still override individually:

```yaml
defaults:
  service_account: my-app-{{ env }}-sa   # `env` is the environment's name
  container_name: app
  pull_policy: IfNotPresent

resources:
  - kind: deployment
    name: api
    image: nginx           # -> serviceAccountName: my-app-prod-sa
  - kind: deployment
    name: worker
    image: nginx
    service_account: worker-sa   # wins over the default
```

Recognised `defaults:` keys are `container_name`, `service_account`, `pull_policy`, `restart_policy` and `host_path_type`; precedence runs resource field, then `defaults:`, then the kind's own default.

`job` and `cronjob` only emit a pod-template `metadata` block when the overlay sets `pod_labels` or `pod_annotations` — a Job's `spec.template` is immutable, so adding labels unconditionally would break `apply` on existing Jobs.

> Rendering strips the blank lines the templates leave between keys. Blank lines *inside* a block scalar — a `configmap` value, a multi-line `secret` entry — are content and are preserved.

## Configuration

- `overlays/<env>/overlay.yaml` — the environment's resource declarations, optionally pinning `kube_context`.
- `overlays/<env>/.env` — variables referenced with `${NAME}` inside that environment's overlay only (no cross-environment fallback).
- `.riveter.toml` — records the environment set by `env set`; shared state in the working directory, so a second terminal running `env set` retargets the first.
- Templates live in `src/templates/`, embedded into the binary at compile time; a resource's lowercased `kind` maps to `<kind>.yaml.j2`.

Rendered manifests are written to `manifests/<env>-manifests.<scope>.yaml` (or `-manifests.selection.yaml` for targeted renders); a manifest containing a Secret is written `0600`. Riveter also writes `manifests/.gitignore` so rendered output is never committed.

## Testing

```bash
cargo test -p riveter
cargo clippy -p riveter --all-targets
cargo fmt -p riveter
```

Golden tests in `tests/golden/<name>.overlay.yaml` / `.expected.yaml` check that every kind has fixture coverage and that rendered output matches what was committed (a change detector, not a correctness check — it tells you output differs from what was recorded, not that either is right; the expectations are only trustworthy because they were read when they were committed). After an intentional template change, regenerate and read the diff before committing it:

```bash
UPDATE_GOLDEN=1 cargo test -p riveter
git diff cli/riveter/tests/golden
```

To check the output against real Kubernetes schemas — which the tests above do not do — run [kubeconform](https://github.com/yannh/kubeconform) over the rendered fixtures. Kinds backed by a CRD (Traefik, cert-manager, Gateway API) have no schema available offline and are skipped:

```bash
kubeconform -strict -ignore-missing-schemas cli/riveter/tests/golden/*.expected.yaml
```

[Home](../README.md)
