---
title: Commands
parent: riveter
grand_parent: Command-line tools
nav_order: 1
---

Environment, rendering and cluster commands. Package and secret commands have their own pages: [Packages and Warehouse](commands-packages.md), [Secrets](commands-secrets.md). Verified locally in a scratch project (`env`, `list`, `render`, `validate`, `schemas list`, `secrets keygen|set|list|show`, REPL). Not run (touch a cluster, a registry or Warehouse): `apply`, `diff`, `prune`, `delete`, `images`, `schemas fetch`, `pack` with pinning, `publish`, `pull`, `install`, `remote`, `login`, `secrets sync` other than `--dry-run`'s error path.

Common arguments:

| Item | Meaning |
|---|---|
| `[TARGET]...` | `kind[/name]`. Both halves accept `*` and `?` wildcards, are case-insensitive, and kind aliases resolve (`sts`, `ds`, `hpa`, `pdb`, `crd`, `netpol`, `sa`, `persistentvolume`, `persistentvolumeclaim`). Omit to act on every resource in scope. A target that matches nothing is an error listing what is available. A target for a resource excluded by `--scope` is an error telling you to pass `--scope all`. |
| `--scope <mutable\|immutable\|all>` | `mutable` skips resources marked `immutable: true` or `lifecycle: immutable` (the check also accepts `static`; see [Configuration](configuration.md#resource-fields)); `immutable` selects only those; `all` everything. Default differs per command (below). Values are case-insensitive in the REPL, exact lower case on the command line. |

## env

```text
riveter env list
riveter env set <ENV>
riveter env show
```

| Subcommand | Does |
|---|---|
| `list` | Prints, sorted, the names of directories under `overlays/` that contain an `overlay.yaml`. Fails if `overlays/` does not exist. |
| `set <ENV>` | Checks `overlays/<ENV>/overlay.yaml` exists (`overlay not found: overlays/x/overlay.yaml`), then writes `[env] current = "<ENV>"` to `.riveter.toml` (keeping `warehouse_url`). |
| `show` | Prints `riveter current environment: <env>` (plus ` (from $RIVETER_ENV)` when that variable is set). Without an environment: `Error: No environment set. Run riveter env set <env>, pass --env <env>, or set $RIVETER_ENV`. |

```text
$ riveter env list
dev
prod
$ riveter env set dev
$ cat .riveter.toml
[env]
current = "dev"
$ riveter env show
riveter current environment: dev
```

The recorded environment is shared state in the working directory: a second terminal running `env set` retargets every other one. Name the environment explicitly (`--env`) in scripts.

## list

```text
riveter list [--scope <SCOPE>] [TARGET]...        alias: ls
```

Default scope `all`. Prints one row per resource (kind, name, lifecycle) without rendering.

```text
$ riveter list
  namespace   dev-ns  immutable
  deployment  api     mutable
  service     api     mutable
$ riveter -e prod ls 'dep*'
  deployment  api  mutable
```

## render

```text
riveter render [--scope <SCOPE>] [TARGET]...      alias: r
```

Default scope `mutable`, so a render previews exactly what a default `apply` sends. Renders the selected resources and writes:

| Case | File |
|---|---|
| no targets, scope `mutable` | `manifests/<env>-manifests.mutable.yaml` |
| no targets, scope `immutable` | `manifests/<env>-manifests.immutable.yaml` |
| no targets, scope `all` | `manifests/<env>-manifests.yaml` |
| targets given | `manifests/<env>-manifests.selection.yaml` |

(All four names were observed.) A manifest that contains a Secret is written with mode `0600`; `manifests/.gitignore` is created with `*` unless it exists. Reports what the scope left out:

```text
$ riveter render
ok rendered 2 resource(s) to manifests/dev-manifests.mutable.yaml
warn 1 resource(s) outside this scope: namespace/dev-ns — `--scope all` includes them
```

## apply

```text
riveter apply [--dry-run] [--no-wait] [--timeout <SECONDS>] [--scope <SCOPE>] [TARGET]...      alias: a
```

| Flag | Default | Meaning |
|---|---|---|
| `--dry-run` | off | `kubectl apply --dry-run=client`; no cluster contact, no namespace check, no waiting. |
| `--no-wait` | off | Do not wait for rollouts or gates; the whole manifest is applied in one `kubectl apply`. |
| `--timeout <SECONDS>` | `300` | Per-rollout (and default per-gate) wait. |
| `--scope` | `mutable` | |

Behaviour:

1. Requires `kubectl`. Renders (see [render](#render)); zero matches: `ok no resources matched selected scope`.
2. Announces the cluster: `context  <env> -> <kube_context>` if the overlay pins `kube_context` (kubectl is then run with `--context <name>`), else a warning naming kubectl's current context.
3. Unless `--dry-run`: if the overlay's namespace does not exist on the reachable cluster and this apply would not create it, fails (`re-run with --scope all to create it first` when the overlay declares an excluded `namespace` resource). An unreachable cluster is not treated as a missing namespace.
4. If the overlay declares `depends_on`/`wait` gates and neither `--dry-run` nor `--no-wait` is given, applies phase by phase (`phase 1/2: ...`), waiting at each gate (`Job` complete or workload ready); a failure reports `applied:`, `failed, possibly partly applied:` and `not applied:` lists. Otherwise one `kubectl apply -f <file>`.
5. Unless `--dry-run`/`--no-wait`: `kubectl rollout status <kind>/<name> -n <ns> --timeout=<T>s` for every selected Deployment, StatefulSet and DaemonSet; a timeout fails with `... did not become ready within <T>s — the manifests were applied, but the rollout has not completed`.
6. Prints `ok applied N resource(s): ...` (`would apply` for `--dry-run`).

Several environments: `riveter --env canary,prod apply`. Each is rendered first; the run is refused if any fails to render or pins no `kube_context`; then they apply in the order given, stopping at the first failure; a final report has one line per environment (`applied 12 resource(s)`, `FAILED: ...`, `not attempted`) and the exit status is non-zero if any failed or were skipped. A name listed twice, an empty entry (`a,,b`) or one with no overlay is refused before anything runs.

The tree printed by `riveter --help` lists `--replicas` and `--except` under `apply`; they are accepted only by [`install`](commands-packages.md#install) (`riveter apply --replicas x=1` fails with `unexpected argument '--replicas' found`).

## diff

```text
riveter diff [--against <GIT_REF>] [--scope <SCOPE>] [TARGET]...      alias: df
```

Default scope `mutable`.

* Without `--against`: `kubectl diff -f <manifest>`; exit status 0 means no difference (`ok cluster matches these manifests`), 1 means differences (`warn the cluster differs from these manifests`), anything else is `kubectl diff failed`. **Riveter itself exits 0 in both cases.** `--env a,b` is supported (one report, `warn at least one environment differs from its manifests`).
* With `--against <GIT_REF>`: no cluster. Reads the overlay as of the commit/branch/tag from git, renders it and the current one, prints a unified diff (`a/<ref>/<env>.yaml` vs `b/working-tree/<env>.yaml`) and `warn <env> renders differently from <ref>` or `ok <env> renders the same as <ref>`. One environment only (`--against compares one environment with its own history; name one --env`). Needs `git`; `.env` is not in git so the current one is used for both sides; nothing is written to the working directory. A missing ref or an environment with no overlay at the ref is refused by name.

## delete

```text
riveter delete [--scope <SCOPE>] [TARGET]...      aliases: d, del
```

Default scope `mutable`, so immutable resources (typically the namespace) survive unless `--scope all` is given. Renders, then `kubectl delete -f <manifest>`. Prints `ok deleted N resource(s): ...`. Only removes what the overlay still declares; use `prune` for dropped entries. There is no confirmation prompt.

## prune

```text
riveter prune [--dry-run]
```

Finds live resources labelled `app.kubernetes.io/managed-by=riveter,env=<env>` (via `kubectl get <kinds> -l ... --ignore-not-found`), compares them with the whole overlay (not the current scope), and deletes the difference. Never pruned: kind `namespace` and `raw`; objects with an owner reference; `Endpoints` of a selecting Service. Output: `ok nothing to prune — the cluster matches the overlay`, or `warn N resource(s) the overlay no longer declares: ...` (`--dry-run`) / `warn pruned N resource(s): ...`.

## images

```text
riveter images [--update] [--overlays-dir <DIR>] [--registry-auth <REGISTRY=USER:PASS>]...
```

| Flag | Default | Meaning |
|---|---|---|
| `--update` | off | Rewrite the `image:` lines in the templates in place to the newest compatible tag found. |
| `--overlays-dir <DIR>` | `overlays` | Directory scanned for `*.yaml.j2` templates. |
| `--registry-auth <REGISTRY=USER:PASS>` | none, repeatable | Credentials for one registry (`*` is the fallback for any). |

Scans every `*.yaml.j2` under the directory for `image:` lines, asks each registry (Docker Hub or a Registry HTTP API v2 endpoint, Bearer or Basic auth) for tags and prints tables of updates and errors. Credential sources in increasing precedence: Docker's `config.json` (`$DOCKER_CONFIG/config.json`, else `~/.docker/config.json`), `RIVETER_REGISTRY_AUTH` (`;`-separated `REGISTRY=USER:PASS` items), `RIVETER_REGISTRY_USERNAME` + `RIVETER_REGISTRY_PASSWORD` (apply to `*`), then `--registry-auth`. Ends with `ok no updates found`, `warn found N update(s); re-run with --update to apply` or `ok updated N image tag(s)`; it exits 1 with `one or more images could not be checked; see the Errors table above` if any lookup failed. In the REPL only `--update` and `--registry-auth <value>` are read. Contacts registries (Unverified, not run).

## validate

```text
riveter validate [--scope <SCOPE>] [-f, --file <FILE>]... [TARGET]...
```

Default scope `all`. Checks custom resources against CRD schemas, offline, the way the API server will (a misspelt field is an error with a suggestion). Bundled schemas: cert-manager `Certificate`, `ClusterIssuer`, `Issuer`; Gateway API `Gateway`, `HTTPRoute`; Traefik `IngressRoute`, `Middleware`; more from `schemas fetch` (cache takes precedence). Kinds without a schema are counted, not failed; built-in groups (core, apps, batch, ...) are reported as `built-in kinds are not checked`.

| Flag | Meaning |
|---|---|
| `-f`, `--file <FILE>` | Check the documents in these files instead of an environment (`-` is standard input); repeatable. Targets are rejected with `--file` (`targets select resources of an environment; with --file there is none`). |

```text
$ riveter validate
info built-in kinds are not checked: deployment x1, namespace x1, service x1
```

Failures print `error <kind>/<name>: <issue>` per issue and end `Error: N resource(s) failed validation` (exit 1). Success lines: `ok N resource(s) satisfy their schema`, `warn not checked, no schema held for: <kind> xN - riveter schemas fetch --crd <name> (or --all) reads them from a cluster`. CEL rules in schemas are not evaluated.

## schemas

```text
riveter schemas list                      alias: ls
riveter schemas fetch [--crd <NAME>]... [--all] [--context <CTX>] [--output <DIR>]
```

`list` prints group, kind, versions and source (`bundled` or cache), with a note for unevaluated CEL rules:

```text
  cert-manager.io              Certificate      v1         bundled
  gateway.networking.k8s.io    Gateway          v1,v1beta1 bundled   (28 CEL rule(s) not evaluated)
```

`fetch` runs `kubectl get crd` against a cluster (Unverified, not run) and writes schemas to the cache directory (`$RIVETER_SCHEMA_DIR`, else `$XDG_CACHE_HOME/riveter/schemas`, else `~/.cache/riveter/schemas`; error `cannot find a cache directory: set RIVETER_SCHEMA_DIR`) or `--output <DIR>`. Without `--crd`/`--all` it takes the seven CRDs riveter has templates for (`certificates`, `clusterissuers`, `issuers` of cert-manager.io; `gateways`, `httproutes` of gateway.networking.k8s.io; `ingressroutes`, `middlewares` of traefik.io). `--all` takes every CRD in the cluster. `--context` picks the kubectl context.

## login

See [Packages and Warehouse](commands-packages.md#login).

## repl

```text
riveter repl
```

Starts the [REPL](repl.md); also the behaviour with no arguments.

## help

```text
riveter help [COMMAND]        alias: h
```

Without an argument prints the same output as `--help`. With a command name (aliases accepted) prints that command's clap help; `riveter help targets` (or `target`) prints the targets reference. An unknown name fails with the unknown-topic message. The overview printed by `--help` (and by the REPL's `help`) is generated from `src/help.rs`; where it differs from the real flags the flags in this book are right (see the [report](../../_reports/cli.md)).
