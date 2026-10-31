---
title: Release and Docker commands
parent: anvil
grand_parent: Command-line tools
nav_order: 2
---

`install`, `release` and `docker ...`. These read the [configuration](configuration.md). Unverified: none of the commands on this page was run for the book (`release` changes manifests, commits, pushes and publishes; `docker push` contacts a registry). They are documented from `commands/install.rs`, `commands/release.rs` and `commands/docker.rs`.

## anvil install

```text
anvil install [--all | -p, --package <PACKAGE>]
```

`--all` conflicts with `--package`. Runs `cargo install --path <dir>` once per target (operation name `install (<package>)`).

Target selection:

1. `--all`: every workspace member, filtered by `[install].packages` if that list is non-empty. Error `No workspace packages found to install (after filtering by config)` when nothing is left.
2. `--package <p>`, else the **first** entry of `[install].packages`.
3. Otherwise the package whose directory is the current directory, else the first package under the workspace root. Error `Could not determine default package name. Use --package <name>.`

An unknown package: `Package '<p>' not found in cargo metadata`.

```bash
anvil install --all
```

## anvil release

```text
anvil release [--all | -p, --package <PACKAGE>] [--dry-run]
```

| Flag | Meaning |
|---|---|
| `--all` | Release every package in `[release].packages` plus every package of every `[docker.modules.*]`. Conflicts with `--package`. (The `--help` text says `[publish].packages`; the configuration key is `[release].packages`.) |
| `-p`, `--package <PACKAGE>` | One package. Must be a workspace member (`Package '<p>' is not a workspace member`). |
| `--dry-run` | Print the plan and stop. |

Without either flag the package whose directory is the current directory is used, or the only workspace package; otherwise `Could not determine release target. Use --package <name> or --all`. With `--all` and nothing configured: `No release packages configured. Set [release].packages and/or [docker.modules.*].packages in .anvil.toml or .anvil/`; a configured name that is not a member: `Configured release package '<p>' is not a workspace member`.

### The plan

Anvil collects the targets and all their workspace dependencies (transitively, over every dependency table), sorts them into **layers** (layer 0 depends on nothing in the set; a later layer only on earlier ones; within a layer alphabetical), and decides per package whether it needs a release:

* it has **no tag** `<package>-v*` yet (first release), or
* a file with a release-relevant extension changed in the package's directory since its latest tag (`rs toml j2 feature ftl yaml yml sh py sql json proto css js html svg tpl`; docs and licences do not count), or a `publish = false` workspace library it depends on changed since that tag, or
* a dependency taken with `{ workspace = true }` changed in the root `[workspace.dependencies]` since that tag, or
* a workspace dependency of it is being released (propagated until stable).

`publish = false` packages that are not Docker packages are never tagged or published; they only matter through their dependents. A package listed under `[docker.modules.*]` is a Docker release, every other one a Cargo release (`cargo publish`, plus `cargo install --path` if it is in `[install].packages`).

For a package that needs a release: if it already has a tag **and** the tag for its current manifest version exists, the version is bumped by one patch; otherwise (never released, or the manifest was bumped by hand past the last tag) the current version is tagged as it is. The tag is `<package>-v<version>`.

`--dry-run` prints the whole graph, including up-to-date packages, and exits without changing anything:

```text
Dry run: planned releases (organized by dependency layers)

Layer 0:
  - quench-core: 0.1.9 -> 0.1.10 (cargo publish), tag: quench-core-v0.1.10
  - quench-args: 0.1.4 (up to date)

Layer 1:
  - quench-http: 0.2.0 (no bump; initial package tag) (cargo publish), tag: quench-http-v0.2.0
```

*Unverified:* sample constructed from the format strings in `print_dry_run_plan_with_layers`; it was not run (the forge tree has no git repository to read tags from).

### A real run

1. Resolve and print the plan (only packages that need a release). If nothing: `Nothing to release: every package in [release].packages is up to date.` (with `--all`) or `Nothing to release: the target package has no changes since its last tag.`, exit 0.
2. Ask `Proceed with release? (yes/no): `; only `yes` or `y` (any case) continues, anything else prints `Release cancelled.` and exits 0.
3. Refuse if any tag to be created exists: `Tag '<tag>' already exists`.
4. Bump the patch version of every bumped package: the first `version = ...` line in the `[package]` table of its `Cargo.toml` is rewritten to `version = "X.Y.Z"` (a trailing comment is kept). A `version = { workspace = true }` line is replaced by the literal version, which detaches the package from the workspace version. A manifest without `[package] version` fails with `No [package] version found in <path>`.
5. `cargo build --package <p> ...` for all bumped packages in one call, to refresh `Cargo.lock`.
6. `git add` the manifests and `Cargo.lock`, `git commit -m <message> -- <files>`, then `git push`. The message comes from `[release].commit_message_template` (default `release: bump package versions ({summary})`); placeholders `{summary}` (`pkg vX.Y.Z, pkg2 vX.Y.Z`), `{changelog}` (one `- pkg vX.Y.Z` per line), `{count}`.
7. Per layer, per package: Docker packages run `docker release` (build, tag, push); Cargo packages run `cargo publish --registry <[release].registry> --package <p>`, with `CARGO_REGISTRIES_<REGISTRY>_TOKEN` set to a fresh `Bearer ...` token from Gatehouse when a secret file exists and the variable is not already in the environment; then `cargo install --path` when the package is in `[install].packages`.
8. After each package: `git tag <tag>` and `git push origin <tag>` (tags are pushed immediately so a later failure still leaves a record of what shipped).
9. `✅ Release completed successfully`.

Exit code 1 with the failing command's status on any error; packages already released keep their tags.

## anvil docker

```text
anvil docker <build|tag|push|release> -p, --package <PACKAGE>
anvil docker build-all
anvil docker release-all
```

A package is found by searching `[docker.modules.*].packages`; an unknown one fails with `Module not found for package: <p>`.

### Naming

| Item | Value |
|---|---|
| Local image name | `image_name` override, else the package name |
| Remote tag | `<registry>/<module_name>/<image_name>:<version>` for every registry |
| `module_name` | the override's `module_name`, else the module key |
| Registries | override `registries`, else override `registry` (deprecated), else `[docker].registry`; error `No Docker registry configured for package '<p>'. Set [docker].registry, [docker.modules.<module>.<p>].registries, or [docker.modules.<module>.<p>].registry` |
| Dockerfile | override `dockerfile`, else the module's |
| Version | the package's version from `cargo metadata` |

### Subcommands

| Command | Does |
|---|---|
| `build -p <p>` | `docker build -f <dockerfile> --network=host --progress=plain --build-arg BUILDKIT_INLINE_CACHE=1 --build-arg PROJECT_NAME=<p> --build-arg RESOURCES_PATH=<package dir relative to the workspace root> [registry build args] [--build-arg K=V from build_args] -t <image_name> .` with `DOCKER_BUILDKIT=1` and `BUILDKIT_PROGRESS=plain`. Prints `Building Docker image for package: <p> using <dockerfile>` and, if any, `  build args: K=V ...`. The context is the current directory. |
| `tag -p <p>` | `docker tag <image_name> <full tag>` per registry. |
| `push -p <p>` | `docker push <full tag>` per registry. |
| `release -p <p>` | `build`, `tag`, `push`. |
| `build-all` | `build` for every package of every module (module order is the iteration order of a hash map: unspecified). |
| `release-all` | `release` for every package. |

`build_args` entries are appended after the derived ones, so a package can override `RESOURCES_PATH` (Docker uses the last value). `-all` commands continue after a failure and end with a summary (`<op>-all completed with N failures:` then `  - module/package: error`), exit 1.

### Private cargo registry inside the build

If `[docker].cargo_registry = "<name>"` is set, anvil adds `--build-arg CARGO_REGISTRIES_<NAME>_INDEX=<index>` and `..._TOKEN=<token>` (each only if found). Index: env `CARGO_REGISTRIES_<NAME>_INDEX`, else `[docker].cargo_registry_index`, else `[registries.<name>] index` in `$CARGO_HOME/config.toml`. Token: env `CARGO_REGISTRIES_<NAME>_TOKEN`, else `[registries.<name>] token` in that file. The token ends up as a build argument; use a build that does not persist it.

```bash
anvil docker build -p hoist
anvil docker release-all
```
