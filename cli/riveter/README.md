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

### Manifest Operations

- `riveter render [--scope mutable|immutable|all]`: Render templates for the current environment.
- `riveter apply [--dry-run] [--scope mutable|immutable|all]`: Apply resources to the Kubernetes cluster.
- `riveter delete [--scope mutable|immutable|all]`: Delete resources from the Kubernetes cluster.

By default, `apply` and `delete` use `--scope mutable`, which skips resources marked immutable.
To include everything explicitly, use `--scope all`.

You can mark immutable resources inside `resources` entries:

```yaml
- kind: namespace
  immutable: true

- kind: ingress
  lifecycle: immutable # `static` also works
```

### Interactive REPL

Simply run `riveter` or `riveter repl` to enter the interactive shell:

```bash
riveter
```

REPL commands:

- `env list`
- `env set <env>`
- `env show`
- `render [--scope mutable|immutable|all]`
- `apply [--dry-run] [--scope mutable|immutable|all]`
- `delete [--scope mutable|immutable|all]`
- `help` or `h`
- `exit`, `quit`, or `q`

REPL aliases:

- `render` -> `r`
- `apply` -> `a`
- `delete` -> `d` or `del`

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

Three kinds keep the defaults they had before the templates were unified, so migrating
does not change what an existing overlay renders:

| Kind | Default |
| --- | --- |
| `deployment` | container name `ossiriand`, `serviceAccountName: ossiriand-<env>-sa`, `imagePullPolicy: Always` |
| `job` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type: File` |
| `cronjob` | `restartPolicy: OnFailure`, `imagePullPolicy: Always`, hostPath `type:` unset |

Newer pod-based kinds default to `imagePullPolicy: IfNotPresent` and set
`serviceAccountName` only when the overlay does. Override any of them per resource with
`container_name`, `service_account`, `pull_policy` and a volume's `type`.

`job` and `cronjob` only emit a pod-template `metadata` block when the overlay sets
`pod_labels` or `pod_annotations` — a Job's `spec.template` is immutable, so adding labels
unconditionally would break `apply` on existing Jobs.

> Rendering strips blank lines from the final manifest. Blank lines inside a block scalar
> (a `configmap` value, a multi-line `secret` entry) are removed with them.

## License
MIT
