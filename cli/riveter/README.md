# Riveter 🏗️

Riveter is a specialized tool for managing Kubernetes manifests, powered by Rust and the `minijinja` templating engine. It simplifies the process of rendering, applying, and managing resource definitions across different environments.

## Features

- **Template Rendering**: Uses Jinja2-style templates (via `minijinja`) to generate Kubernetes YAML manifests.
- **Environment Management**: Easily switch between and manage different environments (e.g., dev, staging, prod).
- **Kubectl Integration**: Direct support for `apply` and `delete` operations via `kubectl`.
- **Dry-run Support**: Preview changes before applying them to the cluster.
- **Interactive REPL**: A built-in REPL for quick environment management and resource operations.

## Installation

```bash
cd forge/riveter
cargo install --path .
```

## Usage

### Environment Management

- `riveter env list`: List all available environments.
- `riveter env set <env>`: Set the current active environment.
- `riveter env show`: Show the currently active environment.

`env set` records the environment in `.riveter.toml` — shared state in the working
directory, so a second terminal running `env set` retargets the first. For anything that
must not be retargeted out from under it (scripts, CI, two terminals on different
environments), name the environment per invocation instead:

```bash
riveter --env prod apply       # or -e prod
RIVETER_ENV=prod riveter apply
```

`--env` wins, then `$RIVETER_ENV`, then whatever `env set` recorded. Both check the
overlay exists before doing anything.

### Manifest Operations

- `riveter list [--scope ...] [target...]`: List the resources an environment declares.
- `riveter render [--scope ...] [target...]`: Render templates for the current environment.
- `riveter diff [--scope ...] [target...]`: Show what applying would change.
- `riveter apply [--dry-run] [--no-wait] [--scope ...] [target...]`: Apply resources to the cluster.
- `riveter delete [--scope ...] [target...]`: Delete resources from the cluster.
- `riveter prune [--dry-run]`: Remove resources the overlay no longer declares.

By default, `render`, `apply` and `delete` use `--scope mutable`, which skips resources
marked immutable — so a `render` previews exactly what an `apply` would send. To include
everything, use `--scope all`. `list` always shows every resource and marks each one's
lifecycle.

Whatever a scope leaves out is reported rather than silently dropped:

```
ok    rendered 3 resource(s) to manifests/prod-manifests.mutable.yaml
warn  1 resource(s) outside this scope: namespace/prod-ns — `--scope all` includes them
```

### Targeting individual resources

Without targets, every command acts on the whole environment. Pass one or more
`kind[/name]` targets to act on a subset instead:

```bash
riveter list                            # what is in this environment?
riveter apply deployment/api            # just that deployment
riveter apply deployment/api service/api
riveter apply statefulset               # every statefulset
riveter apply '*/api'                   # everything named api
riveter render 'deployment/api-*'       # glob on either half
riveter delete --scope all namespace/prod
```

Both halves accept `*` and `?` wildcards, matching is case-insensitive, and kind aliases
work — `sts/pg` selects a `kind: statefulset` resource. Quote patterns so the shell does
not expand them.

A target that matches nothing is an error listing the available resources, so a typo
cannot quietly become a no-op apply:

```
Error: no resource matches `deployment/typo`

available resources:
  namespace/forge-test
  statefulset/pg
  ...
```

Targets are still filtered by `--scope`. Naming an immutable resource while the scope
excludes it is an error telling you to pass `--scope all`, rather than silently skipping it.

Targeted renders are written to `manifests/<env>-manifests.selection.yaml`, so they never
overwrite the full `manifests/<env>-manifests.yaml`.

Every resource needs a `name` (except `namespace`, which takes the overlay's
`namespace_name`), and no two may share a `kind/name` — both are rejected before
anything is rendered, since a nameless resource is refused by the cluster with a far
less obvious message and a duplicate would silently overwrite its twin on apply.

You can mark immutable resources inside `resources` entries:

```yaml
- kind: namespace
  immutable: true

- kind: ingress
  lifecycle: immutable # `static` also works
```

### Binding an environment to a cluster

An overlay can name the kubectl context it deploys to, so which cluster gets hit
is a property of the environment rather than of whatever `kubectl config
use-context` was run last:

```yaml
namespace_name: prod
kube_context: prod-cluster
resources: ...
```

`apply` and `delete` then pass `--context prod-cluster` to kubectl and report the
target before acting:

```
context  prod -> prod-cluster
```

An overlay that pins nothing still works, but says so, naming the cluster it is
about to use:

```
warn  prod pins no kube_context, so this uses kubectl's current context
      `dev-cluster` — add `kube_context: <name>` to overlays/prod/overlay.yaml
      to bind the environment to its cluster
```

`kube_context` accepts `${VAR}` like any other value, and is consumed by riveter
rather than rendered into the manifests.

### Seeing and finishing a change

`render` shows what would be *sent*; `diff` shows what would *change*, by handing the
rendered file to `kubectl diff`:

```bash
riveter diff                    # against live cluster state
riveter diff deployment/api
```

`apply` waits for every Deployment, StatefulSet and DaemonSet it touched to become ready
before reporting success. kubectl accepting a manifest only means the API server stored
it, so without this a rollout that never starts a healthy pod still looks like a
successful deploy:

```
wait  deployment/api (up to 300s)
Error: deployment/api did not become ready within 300s — the manifests were applied,
but the rollout has not completed
```

Use `--no-wait` to return as soon as kubectl accepts the manifests, and
`--timeout <seconds>` to change the per-rollout budget.

### Removing what the overlay dropped

`delete` only removes what the overlay still declares, so deleting an entry from an
overlay would otherwise leave the live resource running forever, invisible to riveter.
`prune` closes that: every template stamps
`app.kubernetes.io/managed-by: riveter` alongside `env: <env>`, so riveter can ask the
cluster what it owns and compare:

```bash
riveter prune --dry-run   # what has been orphaned
riveter prune             # remove it
```

```
warn  2 resource(s) the overlay no longer declares: configmap/stale, deployment/removed-worker
```

Prune compares against the **whole** overlay, not the current scope, so a resource left
out by `--scope` is never mistaken for one the overlay dropped. Two things are never
pruned: `namespace`, because deleting one takes everything inside it, and `raw`, whose
labels come from the overlay rather than from riveter.

> Resources created before this label existed are invisible to `prune` until they are
> applied once more.

### Bootstrapping an environment

Marking `namespace` immutable protects it from `delete`, but it also means the
default `--scope mutable` will not create it. Rather than let every namespaced
resource fail with its own `namespaces "prod-ns" not found`, `apply` checks the
namespace first and says what to run:

```
Error: namespace `prod-ns` does not exist, and this scope excludes the
`namespace` resource prod declares

run `riveter apply --scope all` to create it first
```

The check is skipped for `--dry-run`, and a cluster riveter cannot reach is not
treated as a missing namespace — the apply proceeds and reports the real error.

### Variables

Overlay values may reference variables from the environment's own `.env`
(`overlays/<env>/.env`) with `${NAME}`:

```yaml
- kind: secret
  name: db
  string_data:
    password: ${DB_PASSWORD}
```

A reference with no definition is an error, naming every missing variable and
where it appears — an undefined `${VAR}` left as-is would reach the cluster as
that literal string, which for a Secret means shipping the placeholder as the
value:

```
Error: overlay references undefined variable(s):
  ${DB_PASSWORD} at resources[3].string_data.password

define them in overlays/prod/.env, or write `$${NAME}` to keep a literal `${NAME}` in the manifest
```

Write `$${NAME}` for a reference riveter should leave alone, when something
later — a shell in a container `command`, another templating pass — is the one
meant to expand it:

```yaml
- kind: pod
  name: shell
  image: busybox
  command: ["sh", "-c", "echo $${HOME}"]   # renders as: echo ${HOME}
```

An environment reads only its own `.env`. There is deliberately no fallback to a
shared file: one would let an environment resolve a variable from a file
belonging to a different environment, quietly rendering production with
development's credentials. If several environments share a value, define it in
each `.env` — the error above says exactly which file is missing it.

### Secrets on disk

Rendering writes plaintext: a `secret`'s `string_data` lands in
`manifests/<env>-manifests.yaml` as typed. A manifest that carries a Secret —
including one emitted through `raw` — is therefore written `0600`, readable only
by the user who rendered it. Manifests without Secrets keep the usual mode.

File permissions protect against other users on the machine and do nothing against
`git add -A`, so riveter also writes `manifests/.gitignore` ignoring everything in the
directory — the manifests are build output. An existing `.gitignore` there is left
alone. Prefer `env_refs` pointing at a Secret managed outside riveter over putting live
credentials in an overlay.

### Interactive REPL

Simply run `riveter` or `riveter repl` to enter the interactive shell:

```bash
riveter
```

REPL commands:

| Command | Aliases | |
| --- | --- | --- |
| `env <list\|set\|show>` | | Manage environments |
| `list [options] [target...]` | `ls` | List the environment's resources |
| `render [options] [target...]` | `r` | Render manifests to `manifests/` |
| `apply [options] [target...]` | `a` | Apply manifests via kubectl |
| `delete [options] [target...]` | `d`, `del` | Delete manifests via kubectl |
| `help [command]` | `h` | Show help, or detail for one command |
| `exit` | `quit`, `q` | Leave the REPL |

REPL commands accept `--scope mutable` and `--scope=mutable` alike, matching what
clap accepts on the CLI. Anything else beginning with `-` is rejected rather than
ignored, so a typo such as `apply --dry-runn` is an error instead of a live apply.

`help` prints the full command tree with each command's subcommands and options nested
beneath it. `help <command>` adds prose, the scope reference, target syntax and worked
examples for one command; `help targets` prints the target syntax on its own.

`riveter --help` prints that same tree — it is generated from one table shared by both
surfaces, minus `exit` and plus `repl`. The aliases work on the CLI too (`riveter ls`,
`riveter a --dry-run deployment/api`, `riveter h apply`), and `riveter help <command>` /
`riveter <command> --help` give the per-command detail.

## Templates

Templates live in `src/templates` and are embedded into the binary at compile time. A
resource's `kind` is lowercased and matched against `<kind>.yaml.j2`, so `kind: statefulset`
and `kind: StatefulSet` both render `statefulset.yaml.j2`.

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

Shorthand aliases: `sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, `persistentvolume`,
`persistentvolumeclaim`.

### Anything else: `raw`

For a kind riveter has no template for (vendor CRDs, a brand-new API), `raw` emits its
`manifest` block verbatim — `${VAR}` substitution still applies:

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

### Pod-based kinds

`deployment`, `statefulset`, `daemonset`, `replicaset`, `pod`, `job` and `cronjob` all
share one pod-spec implementation in `_macros.yaml.j2`, so a feature added there is
available to every one of them. The single-container shorthand puts container fields
directly on the resource; use `containers:` (and `init_containers:`) for more than one:

```yaml
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

Overlay keys are `snake_case` and map onto the Kubernetes `camelCase` fields. Fields with
no fixed shape — `tolerations`, `affinity`, `topology_spread_constraints`, HPA `metrics`
and `behavior`, NetworkPolicy `ingress`/`egress`, CRD `versions`, webhook `webhooks` — are
passed straight through as YAML, so they use Kubernetes' own `camelCase` spelling.

#### Defaults

A container is named after its resource, and no `serviceAccountName` is emitted
unless something asks for one. Three kinds carry a default beyond that:

| Kind | Default |
| --- | --- |
| `deployment` | `imagePullPolicy: Always` |
| `job` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type: File` |
| `cronjob` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type:` unset |

Every other pod-based kind uses `imagePullPolicy: IfNotPresent` and hostPath
`type: DirectoryOrCreate`.

An overlay can set its own fallbacks for every pod-based resource with a
top-level `defaults:` block, which a resource may still override individually:

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

Recognised keys are `container_name`, `service_account`, `pull_policy`,
`restart_policy` and `host_path_type`. Precedence runs resource field, then
`defaults:`, then the kind's own default. Override per resource with
`container_name`, `service_account`, `pull_policy` and a volume's `type`.

> **Upgrading:** `deployment` previously hardcoded the container name
> `ossiriand` and `serviceAccountName: ossiriand-<env>-sa`. Those are gone —
> a general-purpose template should not inject one project's ServiceAccount.
> An overlay that relied on them gets the old rendering back with:
>
> ```yaml
> defaults:
>   container_name: ossiriand
>   service_account: ossiriand-{{ env }}-sa
> ```

`job` and `cronjob` only emit a pod-template `metadata` block when the overlay sets
`pod_labels` or `pod_annotations` — a Job's `spec.template` is immutable, so adding labels
unconditionally would break `apply` on existing Jobs.

> Rendering strips the blank lines the templates leave between keys. Blank lines
> *inside* a block scalar — a `configmap` value, a multi-line `secret` entry — are
> content and are preserved.

## Development

```bash
cargo test -p riveter
cargo clippy -p riveter --all-targets
cargo fmt -p riveter
```

### Template tests

Parsing a template proves only that it is valid Jinja, so `tests/golden/` covers
what each one actually emits. Three checks run over the fixtures in
`tests/golden/<name>.overlay.yaml`:

- every rendered document parses as YAML and carries an `apiVersion` and `kind`;
- every kind has a fixture, so a new template cannot ship without coverage;
- output is compared against the committed `<name>.expected.yaml`.

The third is a *change detector*, not a correctness check — it tells you output
differs from what was recorded, not that either is right. Its value is that
editing one template cannot silently alter the other forty; the expectations are
only trustworthy because they were read when they were committed.

After an intentional template change, regenerate and read the diff before
committing it:

```bash
UPDATE_GOLDEN=1 cargo test -p riveter
git diff cli/riveter/tests/golden
```

To check the output against real Kubernetes schemas — which the tests above do
not do — run [kubeconform](https://github.com/yannh/kubeconform) over the
rendered fixtures. Kinds backed by a CRD (Traefik, cert-manager, Gateway API)
have no schema available offline and are skipped:

```bash
kubeconform -strict -ignore-missing-schemas cli/riveter/tests/golden/*.expected.yaml
```

## License
MIT
