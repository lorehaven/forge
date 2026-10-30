---
title: warehouse-cli
parent: Command-line tools
nav_order: 3
has_children: true
permalink: /cli/warehouse-cli/
---

The command-line client of the Warehouse service (Docker registry, Cargo registry, file storage, admin). Package `warehouse-cli`, binary **`warehouse`**.

Version: `warehouse 0.1.16` (`forge/cli/warehouse-cli/Cargo.toml`). Library crate `warehouse_cli` (modules `application`, `cli`, `config`, `domain`, `ui`, `api::{admin_api,crates_api,docker_api,files_api}`).

## Install

```bash
cd forge/cli/warehouse-cli
cargo install --path .       # installs the binary `warehouse`
```

No external tools are needed. The commands are async (tokio) and use `quench-client`.

## Command tree

```text
warehouse docker   registry <add|list|use|remove>   login   catalog   tags
warehouse crates   registry <add|list|use|remove>   login   search   versions   yank   unyank
warehouse files    registry <add|list|use|remove>   storages  ls  upload  preview  download
                   mkdir  rmdir  delete  bulk-delete  bulk-download
warehouse admin    gc
```

There are no global options besides `-h/--help` and `-V/--version`. The tool has no `rivet`/`artifact` commands: rivets are handled by `riveter publish|pull|remote` and app artifacts by `anvil artifact release`.

Every run prints a boxed banner (`Warehouse CLI` / `docker crates files admin`) first, then status lines of the form `warehouse <message>` (colour: green ok, yellow warn, red error).

## Registries and configuration (summary)

A *registry* is a named TOML file holding the URLs (and credentials) of one Warehouse for the three kinds at once. Which registry a command uses is chosen with `--registry <name>` or the *active* registry of that kind. Files live under `.warehouse/` (local, default) or `~/.config/warehouse/` (`--global`). Details and the file schema are on the [Configuration](configuration.md) page.

## Exit codes

| Code | When |
|---|---|
| `0` | Success (also `registry list` with nothing configured: a warning only). |
| `1` | Any error: printed as `Error: <message>` (for example `Error: registry 'nope' does not exist`). |
| `2` | Argument parse error. |

## Verification status

Verified locally with a scratch `.warehouse/` and a scratch `HOME`: every `registry add|list|use|remove` form, `docker login` (writes the config file), error messages. Unverified (not run: they talk to a Warehouse): `docker catalog|tags`, `crates login`'s network use (none: it only saves), `crates search|versions|yank|unyank`, all `files` commands except registry management, `admin gc`. Those are documented from `application.rs` and `api/*.rs`.

## Where to go next

* [Registry commands](commands-registry.md): `registry add|list|use|remove` for docker, crates and files.
* [Docker, crates and admin commands](commands-docker-crates.md).
* [Files commands](commands-files.md).
* [Configuration](configuration.md) and [Recipes](recipes.md).
