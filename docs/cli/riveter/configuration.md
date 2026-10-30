---
title: Configuration
parent: riveter
grand_parent: Command-line tools
nav_order: 5
---

Files, environment variables, the overlay format and the package manifest.

## Files riveter reads and writes

| Path (relative to the working directory) | Read/written | Purpose |
|---|---|---|
| `.riveter.toml` | read; written by `env set` | `[env] current` and optional `warehouse_url`. |
| `.riveter.secret.toml` (or `.riveter/secret.toml`) | read; written by `login` | `gatehouse_url`, `app_token` (names only). Looked up from the working directory upwards. |
| `.env` | read by `login`, `images`, Warehouse commands | Loaded as environment (`quench_core::dotenv`). For key variables also read by `secrets`. **Not** read by `install`. |
| `overlays/<env>/overlay.yaml` | read | The environment. |
| `overlays/<env>/.env` | read | Variables for `${NAME}` in that environment only. |
| `overlays/<env>/rivet.toml` | read by `pack` | Package manifest. |
| `overlays/<env>/values.yaml` or `values.toml` | read by `pack`/`install` | Defaults for `${NAME}` (not both). |
| `overlays/<env>/secrets.yaml` | read | Encrypted variables (see [Secrets](commands-secrets.md)). |
| `manifests/` | written | `<env>-manifests.<scope>.yaml`, `<env>-manifests.selection.yaml`, `<env>-manifests.phase-N.yaml` (one per gated phase, written when the overlay has `wait:` gates), `.gitignore` (`*`). |
| `packages/` | written by `pack`/`pull` | `<name>-<version>.rivet` and a `.gitignore` with `*.rivet`. |
| `$RIVETER_SCHEMA_DIR` / `$XDG_CACHE_HOME/riveter/schemas` / `~/.cache/riveter/schemas` | read; written by `schemas fetch` | CRD schema cache. |
| `~/.config/riveter/age.key` | read | Default age key file. |
| `~/.docker/config.json` (or `$DOCKER_CONFIG/config.json`) | read | Registry credentials for `images`/`pack`. |

### .riveter.toml

```toml
warehouse_url = "https://host/warehouse"   # optional; RIVETER_WAREHOUSE_URL wins

[env]
current = "dev"                            # written by `riveter env set dev`
```

Both keys are optional; a missing file is the empty configuration; an unparsable one is `Failed to parse .riveter.toml`. `env set` rewrites the whole file (pretty-printed TOML).

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `RIVETER_ENV` | every environment command | Environment name; `--env` wins; it wins over `.riveter.toml`. Must name an existing overlay. |
| `RIVETER_WAREHOUSE_URL` | `publish`, `pull`, `install`, `remote` | Warehouse including base path; wins over `.riveter.toml`. |
| `RIVETER_WAREHOUSE_TOKEN` | same | Bearer token (wins over the options below). |
| `RIVETER_GATEHOUSE_URL`, `RIVETER_CLIENT_ID`, `RIVETER_CLIENT_SECRET` | same | Client-credentials exchange (all three needed). `RIVETER_GATEHOUSE_URL` is also the default URL of `login`. |
| `RIVETER_AGE_KEY` | `secrets show|rekey`, render/install of encrypted overlays | The age secret key text. |
| `RIVETER_AGE_KEY_FILE` | same | A file holding the age key; if set it must be a file. |
| `RIVETER_REGISTRY_AUTH` | `images`, `pack`, `publish` | `;`-separated `REGISTRY=USER:PASS` items. |
| `RIVETER_REGISTRY_USERNAME`, `RIVETER_REGISTRY_PASSWORD` | same | Credentials for any registry (`*`). |
| `RIVETER_SCHEMA_DIR` | `validate`, `schemas` | Schema cache directory. |
| `DOCKER_CONFIG` | `images`, `pack` | Directory of Docker's `config.json`. |
| `XDG_CACHE_HOME`, `HOME` | `schemas`, age key | Cache and key locations. |
| `HOSTNAME` | `login` | Default token name `riveter on <HOSTNAME>`. |
| `CONVEYOR_SHA`, `GITHUB_SHA` | `--version-suffix {sha}` | Commit for the `{sha}` token. |

## overlay.yaml

A YAML file with minijinja templating available (`{% raw %}{% include %}{% endraw %}` and friends, loaded from the overlay directory). Top-level keys:

| Key | Type | Meaning |
|---|---|---|
| `namespace_name` | string | Namespace of every namespaced resource; also the name of a `namespace` resource that has none. |
| `kube_context` | string | kubectl context this environment deploys to; riveter passes `--context`. Consumed by riveter, not rendered. May use `${VAR}`. With several environments in `--env a,b` it is mandatory. |
| `defaults` | mapping | Fallbacks for pod-based kinds: `container_name`, `service_account`, `pull_policy`, `restart_policy`, `host_path_type`. Must be a mapping (error otherwise). Precedence: resource field, then `defaults:`, then the kind's own default. |
| `resources` | list | The resources. |

Other top-level keys are accepted and ignored by riveter (the golden fixtures contain `ns_name`).

### Variables

`${NAME}` in any value is replaced from the environment's own `overlays/<env>/.env` (dotenv format) plus, for installs, the package's values/secrets and `--env-file`/`--set`. An undefined variable aborts the render and lists every missing one with its location:

```text
Error: overlay references undefined variable(s):
  ${ENVNAME} at defaults.service_account
  ${TAG} at resources[0].image

define them in overlays/t/.env, or write `$${NAME}` to keep a literal `${NAME}` in the manifest
```

`$${NAME}` renders as the literal `${NAME}`. There is no fallback to a shared `.env`.

### Resource fields

Every resource has `kind` (lower-cased; aliases `sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, `persistentvolume`, `persistentvolumeclaim`) and `name` (except `namespace`, which takes `namespace_name`). Duplicate `kind/name` pairs are refused (`resource(s) declared more than once: deployment/a`). Common keys read by riveter itself:

| Key | Meaning |
|---|---|
| `immutable: true` or `lifecycle: immutable` / `static` | Excluded from `--scope mutable`; protected from default `delete`. (`lifecycle` is compared case-insensitively.) |
| `depends_on: [kind/name, ...]` | Apply after those resources; the overlay is reordered once after rendering, stably. Unchanged order if no resource uses it. |
| `wait: complete` | After applying, wait until the Job has completed (only for `job`; a failed Job fails the apply). |
| `wait: ready` | Wait until the Deployment/StatefulSet/DaemonSet has rolled out. |
| `wait_timeout: <seconds>` | Positive integer; per-gate timeout overriding `--timeout`. Requires `wait`. |

All other fields are per-kind and defined by the embedded templates (`forge/cli/riveter/src/templates/<kind>.yaml.j2`); the rendered output of a rich overlay for every kind is in `forge/cli/riveter/tests/golden/*.overlay.yaml` with `*.expected.yaml`. Keys are `snake_case` and map to Kubernetes `camelCase`; free-form fields (`tolerations`, `affinity`, `topology_spread_constraints`, HPA `metrics`/`behavior`, NetworkPolicy `ingress`/`egress`, CRD `versions`, webhook `webhooks`) are passed through in Kubernetes spelling.

Every rendered resource gets labels `app.kubernetes.io/managed-by: riveter` and `env: <env>` (what `prune` selects on); pod templates of Deployments get `app.kubernetes.io/name`, `app.kubernetes.io/instance` and `env`.

### Kinds

Templates embedded in the binary (`src/templates/`):

| Group | Kinds |
|---|---|
| Workloads | `deployment`, `statefulset`, `daemonset`, `replicaset`, `pod`, `job`, `cronjob` |
| Config & storage | `configmap`, `secret`, `pv`, `pvc`, `storageclass` |
| Networking | `service`, `ingress`, `ingressclass`, `networkpolicy`, `endpoints`, `endpointslice`, `gateway`, `httproute` |
| Traefik | `ingressroute`, `middleware` |
| Scaling & scheduling | `horizontalpodautoscaler`, `poddisruptionbudget`, `priorityclass`, `runtimeclass` |
| Policy & quota | `namespace`, `resourcequota`, `limitrange` |
| RBAC | `serviceaccount`, `role`, `rolebinding`, `clusterrole`, `clusterrolebinding` |
| API extension | `customresourcedefinition`, `apiservice`, `mutatingwebhookconfiguration`, `validatingwebhookconfiguration` |
| cert-manager | `certificate`, `issuer`, `clusterissuer` |
| Escape hatch | `raw` (emits its `manifest:` block verbatim; `${VAR}` still applies) |

`prune` considers every kind above except `namespace` and `raw`.

Pod-based kinds defaults: `deployment` pull policy `Always`; `job`: `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath type `File`; `cronjob`: `OnFailure`, `Always`, hostPath type unset; the rest `IfNotPresent` and hostPath `DirectoryOrCreate` (from the project's previous documentation; spot-checked, not exhaustively re-verified).

### Example

A scratch overlay used for the verified outputs in this book:

```yaml
namespace_name: dev-ns
kube_context: dev-cluster

resources:
  - kind: namespace
    immutable: true
  - kind: deployment
    name: api
    image: nginx:1.27
    replicas: 2
    port: 8080
  - kind: service
    name: api
    port: 80
    target_port: 8080
```

Gates:

```yaml
resources:
  - kind: job
    name: migrate
    image: img:${TAG}
    wait: complete
    wait_timeout: 120
  - kind: deployment
    name: api
    image: img:${TAG}
    depends_on: [job/migrate]
    wait: ready
```

Rendering this wrote `t-manifests.yaml` plus `t-manifests.phase-1.yaml` … for the phased apply (verified).

## rivet.toml

Read by `pack` from `overlays/<env>/rivet.toml`; regenerated into the archive. Missing file error: `an overlay needs a rivet.toml next to its overlay.yaml: [package] name = "<overlay directory name>" version = "0.1.0"`.

```toml
[package]
name = "forge"            # must equal the overlay directory name; lowercase a-z, 0-9, -
version = "0.4.0"         # maintained by hand; no build metadata
description = "the forge estate"   # optional
namespace = "forge"       # optional; defaults to the overlay's namespace_name

[requires]
riveter = ">=0.3"         # enforced at install
packages = []             # only noted at install

[[deployment]]            # optional: named groups of resources (used by Gantry)
name = "core"
resources = ["deployment/api"]
```

The `[[deployment]]` tables (fields `name`, `description`, `resources`, `default`, `conflicts_with`, `also_stops`) and `[meta]` belong to the `rivet-package` library; every workload named in a `[[deployment]]` must exist in the overlay. See `rivet-package` in the libraries section.

## values.yaml / values.toml

A flat mapping of defaults for the overlay's `${NAME}`s (`NAME: value` or `NAME = "value"`); nested values are an error; both files together are an error. Packed as is. `pack` prints the variables the overlay uses that have no default.
