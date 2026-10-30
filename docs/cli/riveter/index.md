---
title: riveter
parent: Command-line tools
nav_order: 2
has_children: true
permalink: /cli/riveter/
---

Riveter renders Kubernetes manifests from per-environment overlays (`overlays/<env>/overlay.yaml`, minijinja templates, `snake_case` resource declarations) and applies, diffs, deletes and prunes them with `kubectl`. It also packs an overlay into a versioned `.rivet` package, publishes it to Warehouse, installs packages, keeps secrets in git encrypted with age, and validates custom resources against CRD schemas offline.

Version: `riveter 0.4.15` (`forge/cli/riveter/Cargo.toml`). Binary: `riveter`. Library crate: `riveter`.

## Install

```bash
cd forge/cli/riveter
cargo install --path .        # or: anvil install -p riveter
```

Runtime requirements: `kubectl` on `PATH` for `apply`, `diff`, `delete`, `prune`, `install`, `secrets sync`, `schemas fetch` (riveter checks for the binary first and says why it needs it); `git` for `diff --against`. Everything else (`list`, `render`, `validate`, `pack --no-pin`, `secrets keygen|set|import|list|show|remove|rekey`, `env`) works offline and is the safe way to try the tool.

## Concepts

* **Environment**: a directory `overlays/<env>/` holding an `overlay.yaml`. The environment for a run is, in order: `--env`/`-e`, `$RIVETER_ENV`, the value `riveter env set` recorded in `.riveter.toml`.
* **Resource**: one entry of the overlay's `resources:` list; `kind` + `name`. Targets (`kind[/name]`, wildcards `*` and `?`, kind aliases) select resources; scopes (`mutable`, `immutable`, `all`) filter by lifecycle.
* **Rendered manifest**: written under `manifests/` (`manifests/<env>-manifests.<scope>.yaml`).
* **Package**: a whole overlay directory as `<name>-<version>.rivet` (format owned by the `rivet-package` library).

Riveter reads `overlays/` and writes `manifests/` relative to the working directory.

## Global option

| Option | Meaning |
|---|---|
| `-e`, `--env <ENV>` | Environment to act on; overrides `env set` and `RIVETER_ENV`. `global`: accepted before or after the subcommand. Only `apply` and `diff` accept a comma-separated list (`--env a,b,c`); every other command fails with `this command takes one environment; only apply and diff accept a list (--env a,b)`. |
| `-h`, `--help` | The help is the command tree shared with the REPL plus a Targets and Scopes reference (see [Commands](commands.md#help)). |
| `-V`, `--version` | Prints `riveter 0.4.15`. |

With no subcommand riveter starts the [REPL](repl.md).

## Exit codes

| Code | When |
|---|---|
| `0` | Success. Also: `diff` when the cluster differs (it only warns), `apply`/`delete` that matched no resources (`ok no resources matched selected scope`). |
| `1` | Any error: printed as `Error: <message>` (with `Caused by:` lines). Includes `validate` failures, a failing `kubectl`, a rollout that did not become ready in time, `multi`-environment failures. |
| `2` | Argument parse error (unknown flag, missing argument). |

## Where to go next

* [Commands](commands.md): every top-level command, flag and alias.
* [Packages and Warehouse](commands-packages.md): `pack`, `publish`, `pull`, `install`, `remote`, `login`.
* [Secrets](commands-secrets.md): `secrets ...`.
* [REPL](repl.md).
* [Configuration](configuration.md): `.riveter.toml`, environment variables, `overlay.yaml`, `rivet.toml`, values and secrets files, resource kinds.
* [Recipes](recipes.md).
