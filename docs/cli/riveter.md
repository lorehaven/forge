# Riveter

Riveter is a Kubernetes manifest tool, powered by Rust and `minijinja` templates. Overlays (per-environment YAML) declare resources by kind; Riveter renders them into full Kubernetes manifests and can apply, diff, delete, or prune them against a cluster with `kubectl`. It exists so that Kubernetes environments in this workspace are declared once, in a compact `snake_case` overlay format, rather than as hand-written YAML duplicated per environment.

## Features

- Jinja2-style templating (`minijinja`) covering most core Kubernetes kinds plus Traefik, cert-manager, Gateway API, and RBAC resources, with a `raw` escape hatch for anything else.
- Environment management (`env list|set|show`), with the active environment recorded in `.riveter.toml` and overridable per invocation with `--env`/`-e` or `$RIVETER_ENV`.
- Scoped operations (`mutable`/`immutable`/`all`) so `render`/`apply`/`delete` can skip resources marked immutable by default.
- Targeting: any command can act on a `kind[/name]` subset, with `*`/`?` wildcards and kind aliases (`sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, ...).
- `kubectl` integration for `apply`, `diff`, and `delete`, including per-rollout readiness waiting on `apply`.
- `prune`, which finds cluster resources labelled `app.kubernetes.io/managed-by: riveter` for the environment that the overlay no longer declares, and removes them.
- `validate`, which checks an environment's custom resources (cert-manager, Traefik, Gateway API, and any CRD fetched from a cluster) against their CRD schemas offline, catching a misspelt field or a wrong type before anything is applied - see [Validating custom resources](#validating-custom-resources).
- `images`, which scans overlay templates for `image:` tags and checks the registry for newer compatible tags, optionally rewriting the templates in place.
- Variable substitution from an environment's own `.env` file (`${NAME}`, with `$${NAME}` as an escape).
- Packages: `pack` turns an overlay into a versioned, checksummed `.rivet` archive with every image pinned to a digest, `publish`/`pull`/`remote` move it through Warehouse's rivet registry, and `install` fetches one and applies it - see [Packages](#packages).
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
| `validate [--scope ...] [--file <f>] [target...]` | | Check custom resources against their CRD schemas, offline |
| `schemas <list\|fetch>` | | List the CRD schemas held, or read more from a cluster (CLI-only) |
| `images [--update] [--overlays-dir <dir>] [--registry-auth ...]` | | Check/update deployment image tags |
| `pack [--version-suffix <s>] [--no-pin] [--out <dir>] [--registry-auth ...]` | | Build the environment into a `.rivet` package (CLI-only) |
| `publish [file] [--version-suffix <s>] [--no-pin] [--out <dir>]` | | Upload a package to Warehouse, packing the environment first if no file is given (CLI-only) |
| `pull <package> [--out <dir>]` | | Download a package and verify it (CLI-only) |
| `install <package> [--env-file <f>] [--set K=V] [--dry-run] [--no-wait] [--timeout <s>] [--scope ...] [target...]` | `i` | Fetch a package, render it and apply it (CLI-only) |
| `remote <list\|versions <name>>` | | Browse the packages in Warehouse (CLI-only) |
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

`delete` only removes what the overlay still declares, so removing an entry from an overlay would otherwise leave the live resource running forever, invisible to riveter. `prune` closes that gap: every template stamps `app.kubernetes.io/managed-by: riveter` alongside `env: <env>`, so `riveter prune [--dry-run]` can ask the cluster what it owns and compare. Prune compares against the **whole** overlay, not the current scope, so a resource left out by `--scope` is never mistaken for one the overlay dropped. Two kinds are never pruned: `namespace` (deleting one takes everything inside it) and `raw` (its labels come from the overlay rather than from riveter). Objects that something else derived from one of riveter's are not pruned either, however many of riveter's labels they carry: Kubernetes and cert-manager copy a Service's or Ingress's labels onto the `Endpoints`, `EndpointSlice`s and `Certificate`s they generate, so a label match alone would propose deleting every certificate and endpoint in the namespace. Anything with an owner reference is skipped (riveter's own objects have none), and so are the `Endpoints` of a Service that has a selector, which carry no owner and are recognised by that Service being present. The `Endpoints` of a Service *without* a selector are the hand-written kind riveter's `endpoints` template is for, and stay prunable. The listing asks kubectl for those few fields rather than whole objects, so a prune never reads Secret contents. Resources created before the managed-by label existed are invisible to `prune` until they are applied once more.

### Bootstrapping an environment

Marking `namespace` immutable protects it from `delete`, but it also means the default `--scope mutable` will not create it. Rather than let every namespaced resource fail with its own `namespaces "prod-ns" not found`, `apply` checks the namespace first and, if it's missing, errors telling you to run `riveter apply --scope all` to create it. The check is skipped for `--dry-run`, and a cluster riveter cannot reach is not treated as a missing namespace — the apply proceeds and reports the real error.

### Variables

Overlay values may reference variables from the environment's own `.env` (`overlays/<env>/.env`) with `${NAME}`. A reference with no definition is an error naming every missing variable and where it appears — an undefined `${VAR}` left as-is would reach the cluster as that literal string, which for a Secret means shipping the placeholder as the value. Write `$${NAME}` for a reference riveter should leave alone (renders as literal `${NAME}`), for when something later — a shell in a container `command`, another templating pass — is meant to expand it instead. An environment reads only its own `.env`: there is deliberately no fallback to a shared file, since that would let an environment resolve a variable from a file belonging to a different environment, quietly rendering production with development's credentials. If several environments share a value, define it in each `.env`.

### Secrets on disk

Rendering writes plaintext: a `secret`'s `string_data` lands in `manifests/<env>-manifests.yaml` as typed. A manifest that carries a Secret — including one emitted through `raw` — is therefore written `0600`, readable only by the user who rendered it; manifests without Secrets keep the usual mode. File permissions protect against other users on the machine and do nothing against `git add -A`, so riveter also writes `manifests/.gitignore` ignoring everything in the directory (an existing `.gitignore` there is left alone). Prefer `env_refs` pointing at a Secret managed outside riveter over putting live credentials in an overlay.

### Validating custom resources

`kubectl apply` checks a custom resource against its CRD's schema only when it arrives, so a typo in a `Certificate` or an `IngressRoute` surfaces after the rest of an overlay has already been applied. `riveter validate` finds it first:

```
$ riveter validate
error certificate/api-tls: spec: unknown field `dnsNmaes` (did you mean `dnsNames`?)
error ingressroute/web: spec.routes[0]: "match" is a required property
ok    4 resource(s) satisfy their schema
info  built-in kinds are not checked: deployment x3, service x2
Error: 2 resource(s) failed validation
```

It renders the environment in memory (nothing is written to `manifests/`, nothing reaches a cluster) and checks each resource whose `apiVersion`/`kind` has a schema held. `raw` resources are checked the same way, by what they render to. It exits non-zero if anything failed. Targets and `--scope` work as they do for `render` (scope defaults to `all`), and `--file <path>` (`-` for standard input) checks the documents in files instead, which is how to check a live cluster:

```bash
kubectl get certificates -A -o yaml | riveter validate --file -
```

**What counts as an error**, following what the API server does with a structural schema: a wrong type, a missing `required` field, a value outside an `enum` or a `minimum`, an `apiVersion` the CRD does not serve (the error lists the ones it does) - and **an unknown field**, as `kubectl`'s strict field validation reports it. A single unknown field gets a "did you mean" when another field of the same object is close in spelling (a swap of two letters counts as one edit, so `mathc` finds `match`).

**What is checked against.** cert-manager (`Certificate`, `Issuer`, `ClusterIssuer`), Traefik (`IngressRoute`, `Middleware`) and Gateway API (`Gateway`, `HTTPRoute`) - the CRDs riveter has templates for - are compiled into the binary. `riveter schemas fetch` reads schemas from a cluster's CRDs into a cache directory (`$RIVETER_SCHEMA_DIR`, else `$XDG_CACHE_HOME/riveter/schemas`, else `~/.cache/riveter/schemas`), and a cached schema takes precedence over a bundled one, so after upgrading a CRD, fetch again:

```bash
riveter schemas list                          # what is held, and from where
riveter schemas fetch                         # refresh the kinds riveter templates
riveter schemas fetch --crd widgets.example.io
riveter schemas fetch --all                   # every CRD in the cluster, for raw resources
riveter schemas fetch --context staging       # a kubectl context other than the current one
```

The bundle is produced by that same command, so there is one code path: after a cert-manager, Traefik or Gateway API upgrade, `riveter schemas fetch --output cli/riveter/src/schemas` regenerates it. Descriptions and examples are stripped on the way in, which is most of a CRD's size (the whole bundle is about 130 KB).

**Kinds with no schema are counted, not failed.** Built-in kinds (`Deployment`, `Service`, `Ingress`, ...) get a quiet note; a kind from some other group gets a warning naming it and how to fetch its schema. Built-in kinds are out of scope here: this checks custom resources.

**Limits.** CEL rules a CRD carries (`x-kubernetes-validations`, 176 of them in the bundled Gateway API schemas) cannot be evaluated; `schemas list` shows how many each schema holds. `format` is not checked. An unknown field is an error even inside a schema that would merely prune it, as the strict validation `kubectl` applies by default would. And `validate` renders with the overlay's own `.env`, as `render` does, so it needs those variables: it cannot run in a CI job that has no `.env`, which is why `pack`, which does not render, is what a pipeline's check stage runs.

### Checking for image updates

`riveter images` scans every `deployment*.yaml.j2` overlay template for `image:` lines and checks each registry for a newer tag with the same prefix/suffix and at least as many version components. A floating tag (`latest`, `stable`, `edge`, `main`, `master`, `dev`, `nightly`) is reported but never compared:

```bash
riveter images             # list available updates
riveter images --update    # rewrite templates in place to the newest compatible tag
```

Registry credentials are collected in ascending precedence: Docker's own config, then `RIVETER_REGISTRY_AUTH` (or `RIVETER_REGISTRY_USERNAME`/`RIVETER_REGISTRY_PASSWORD`), then repeatable `--registry-auth REGISTRY=USER:PASS` flags.

### Interactive REPL

Run `riveter` or `riveter repl` with no arguments to enter the interactive shell. REPL commands accept `--scope mutable` and `--scope=mutable` alike, matching what clap accepts on the CLI; anything else beginning with `-` is rejected rather than ignored, so a typo such as `apply --dry-runn` is an error instead of a live apply. `help` prints the full command tree with each command's subcommands and options nested beneath it; `help <command>` adds prose, the scope reference, target syntax and worked examples for one command; `help targets` prints the target syntax on its own. `riveter --help` prints that same tree — generated from one table shared by both surfaces, minus `exit` and plus `repl` — and the aliases work on the CLI too (`riveter ls`, `riveter a --dry-run deployment/api`, `riveter h apply`).

## Packages

A package is one whole overlay directory, versioned, as a `<name>-<version>.rivet` (a checksummed `tar.zst`; the format is [rivet-package](../libs/rivet-package.md)). The registry it lives in is [Warehouse's rivet registry](../docker/warehouse-service.md#rivet-registry). `overlays/forge/` becomes the package `forge`: the directory name *is* the package name, because the overlay's own `{% include "forge/base.yaml.j2" %}` lines spell it out, so an install puts the files back under `overlays/<name>/` and nothing needs rewriting.

### Packing

An overlay needs a `rivet.toml` beside its `overlay.yaml`, naming the package (it must match the directory) and giving the version you maintain by hand:

```toml
[package]
name = "forge"
version = "0.4.0"
description = "the forge estate"      # optional
# namespace defaults to the overlay's namespace_name

[requires]
riveter = ">=0.3"
```

`riveter pack` then writes `packages/forge-0.4.0.rivet`. A CI build adds build metadata with `--version-suffix`: `b123` gives `0.4.0+b123`, and the version you wrote must not already carry any. The suffix may use two tokens, expanded by riveter because a pipeline step has no shell to do it: `{timestamp}` is the UTC time as `YYYYMMDDHHMMSS`, and `{sha}` is the first seven characters of `$CONVEYOR_SHA` (or `$GITHUB_SHA`). A token that cannot be expanded - `{sha}` with no commit known, or an unknown name - is an error rather than a literal. Warehouse orders two builds of one version by comparing their build metadata as text, so start with something that sorts: `{timestamp}.{sha}` does, where a bare `b9` sorts after `b10`.

What goes in, and what does not:

- **Everything in the overlay directory** except dotfiles. `.env`, which holds secrets, can therefore never be packed; `.env.example` is the one dotfile that is, as the documentation of what to supply. Left-out files are listed so a missing one is explained.
- **A Secret may only hold placeholders.** A package is readable by everyone who can read the registry, which is a wider audience than the private repository it was built from. `pack` therefore reads the overlay before `${VAR}` expansion and refuses any `secret` resource whose `data`/`string_data`/`stringData` value is not a `${VAR}` reference (naming the resource and key), and any `raw` resource that declares a Secret, since that cannot be inspected. A placeholder is how a Secret stays in an overlay at all: its value arrives at install through `--env-file` or `--set`.
- **A symlink is an error**, not a skip: following it could pull in a file from anywhere, and dropping it would ship a package that renders differently from its overlay.
- **Includes must stay inside the directory.** Riveter renders the overlay with a recording template loader (so an include behind a condition or built from a variable is found too) and refuses a package whose overlay loaded anything from outside `<name>/`, or anything it would not pack.
- **Images are pinned.** Every `image:` line in `overlay.yaml` and `*.yaml.j2` that names a tag is rewritten to `repo:tag@sha256:<digest>`, resolved against the registry with a `HEAD` on the manifest (index types accepted, so a multi-arch tag pins the index; the bearer-token and Basic flows `images` already speaks). The tag stays for readability. Already-pinned lines are left alone; a `${VAR}` image or one with no tag is reported as not pinned rather than guessed at. **Packing therefore needs registry access** and fails if a digest cannot be resolved; `--no-pin` opts out. Credentials are the same as `riveter images` (`--registry-auth`, `RIVETER_REGISTRY_AUTH`, Docker config).
- **Output is reproducible**: same input, same bytes.

`pack` ends by reading back what it wrote with the check Warehouse will apply, and prints which `${VAR}`s the overlay uses that the package does not default - what the installer must supply. That list is read from the overlay's parsed values, as expansion reads them, so a `${...}` in a YAML comment is not counted.

An optional `values.toml` in the overlay directory (`NAME = "default"`, strings, numbers or booleans) supplies defaults for those variables and is packed as it is.

### Publishing and fetching

```
RIVETER_WAREHOUSE_URL     Warehouse, including its base path (https://host/warehouse)
RIVETER_WAREHOUSE_TOKEN   a bearer token, or instead:
RIVETER_GATEHOUSE_URL + RIVETER_CLIENT_ID + RIVETER_CLIENT_SECRET
                          exchanged for a token with Gatehouse's client_credentials grant
```

Both are also read from a `.env` in the working directory. `publish` needs the `warehouse:write` grant; `pull`, `install` and `remote` need only a valid identity. A version can be published once. `pull` and `install` check the download against the digest Warehouse recorded, then validate the archive, before using it.

### Installing

`riveter install forge` (the newest), `forge@0.4.0`, or `./forge-0.4.0.rivet` for a local file. The package is unpacked to a scratch directory and rendered and applied exactly as `apply` would - same `--scope`, targets, `--dry-run`, rollout waiting and `kube_context` handling - so nothing is written to the working directory, and the scratch tree is deleted afterwards.

Variables for the overlay's `${NAME}`s come from, lowest first: the package's `values.toml`, `--env-file <dotenv file>` (typically the overlay's own `.env`), then `--set KEY=value`. **The working directory's `.env` is not read** - an install is a function of the package and what was passed. They are held in memory, never written to disk. A variable nobody supplies is an error that names it.

Every installed resource is stamped, after rendering, with the label `riveter.forge/package: <name>` and the annotation `riveter.forge/package-version: <version>` (an annotation because `0.4.0+b123` is not a legal label value), so the cluster records what is live. This is done on the rendered document rather than the overlay data because the embedded templates build their metadata by hand and not all of them honour `labels`/`annotations`; the cost is that comments in the rendered YAML are dropped. `requires.riveter` is enforced; `requires.packages` is only noted, since Riveter does not yet know what else is installed. As with `apply`, the default scope skips immutable resources such as the namespace, so a first install into a fresh cluster needs `--scope all`.

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

## Where it works

Riveter reads `overlays/` and writes `manifests/` relative to the working directory. `install` cannot rely on that, so those two directories, the overlay's `.env`, and the labels/annotations a render stamps are a per-thread `Workspace` (`env::with_workspace`) that a command runs inside; outside one, everything is as it always was. It is thread-local rather than a `chdir` so tests (which share a process) cannot race each other through it, and it is restored even if the closure panics.

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

`tests/unit/package_tests.rs` packs real overlays in temp directories: what is and is not packed (a real `.env` never is), byte-identical rebuilds, the version-suffix rules, a missing or mismatched `rivet.toml`, includes from outside the directory (including one behind a condition), symlinks, image pinning against a fake resolver (looked up once per distinct image, CRLF and a missing final newline preserved, already-pinned and templated images left alone, a failed lookup naming `--no-pin`), the values layering, and an installed package rendering from its scratch tree with every resource - namespace included - stamped. `registry_tests.rs` and `image_digest_tests.rs` run the registry client and the digest lookup against a `wiremock` server, including the bearer and Basic challenges and the client-credentials exchange.

None of that proves the real stack agrees, so the package commands were also run by hand against a real Warehouse on real Postgres (migration applied through Foundry): pack, publish, a `409` on republish, `remote`, a byte-identical pull, and an install whose `kubectl` was a stub that recorded the manifest it was given. That is where the foreign key on `uploaded_by` showed up - an auth-less test publisher is `admin`, but a CI client's subject is not a user at all - and where the incomplete stamping did. Neither could have been found with the in-memory database and a mocked server.

`tests/unit/prune_tests.rs` pins what prune calls an orphan, against the exact listing the cluster printed when the bug showed: owned objects and a selecting Service's `Endpoints` are never orphans, a selectorless Service's `Endpoints` and a hand-made `Certificate` still are, and a forge-shaped namespace of certificates, slices and endpoints around two stale Jobs yields only the Jobs. `repl_tests.rs` runs the same through `find_orphans` with a fake `kubectl`.

`tests/unit/schema_tests.rs` checks the validator against a small CRD of its own (every kind of error, each served version against its own schema, the suggestion rules, a field called `default` or `title` surviving the stripping, `nullable`/int-or-string/closed-object normalisation, combinator branches left open, the cache beating the bundle) and against the **real** bundled schemas. `schema_cmd_tests.rs` covers documents and lists, `fetch` against a stand-in `kubectl` (what it asks for, what it writes, what it does when kubectl fails or returns nothing), validating an environment end to end - including that it writes nothing - and one test that renders every golden fixture and validates what riveter's own templates emit against the real CRD schemas, which is what would notice a template drifting from a CRD.

None of that proves the schemas agree with a real cluster, so the validator was also run over every custom resource in it - 11 Certificates, 2 ClusterIssuers, 11 IngressRoutes and 6 Middlewares, `status` blocks included - and accepted all 30, then given deliberately broken ones (a typo, a wrong type, a missing required field, a bad enum, an unserved version, a nested typo) and caught each. The first check is the one that matters: a validator that rejects valid objects would be worse than none.

[Home](../README.md)
