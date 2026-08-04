# Warehouse CLI

Warehouse CLI (package `warehouse-cli`, binary `warehouse`) is the command-line client for the Warehouse service — the workspace's self-hosted Docker registry, Cargo registry, and file storage. It lets you manage registry configurations and drive Docker/crates/file/admin operations from a terminal instead of hand-rolling `curl` calls against Warehouse's API.

## Features

- **Docker registry operations**: manage configured registries, log in, browse the image catalog, and list tags for a repository.
- **Cargo registry operations**: manage configured registries, log in, search crates, list published versions (including yanked), and yank/unyank a version.
- **File storage operations**: manage configured registries, list storages, browse/upload/preview/download files and folders (including bulk operations and zip downloads), and create/remove directories.
- **Admin operations**: trigger garbage collection for Docker, crates, or both.
- **Multiple registries**: each of Docker, crates, and files has its own independent registry list, with one settable as active per kind; a command that omits `--registry` uses the active one.
- **Local vs. global config**: registry entries are written under `.warehouse/` in the current directory by default, or under `~/.config/warehouse/` with `--global`.

## Requirements

- A reachable Warehouse service instance to point registries at.

## Usage

```bash
cargo run -p warehouse-cli -- --help
# or, once installed:
warehouse --help
```

### Docker

```bash
warehouse docker registry add <name> --url <url> [--base-path /warehouse] [--path /v2] [--service <svc>] [--insecure-tls] [--use] [--global]
warehouse docker registry list
warehouse docker registry use <name> [--global]
warehouse docker registry remove <name> [--global]
warehouse docker login --username <user> --password <pass> [--registry <name>] [--global]
warehouse docker catalog [--registry <name>] [--n <page-size>]
warehouse docker tags <repository> [--registry <name>] [--n <page-size>]
```

### Crates

```bash
warehouse crates registry add <name> --url <url> [--base-path /warehouse] [--insecure-tls] [--use] [--global]
warehouse crates registry list
warehouse crates registry use <name> [--global]
warehouse crates registry remove <name> [--global]
warehouse crates login --token <token> [--registry <name>] [--global]
warehouse crates search <query> [--registry <name>] [--limit <n>]
warehouse crates versions <crate_name> [--registry <name>] [--all]
warehouse crates yank <crate_name> <version> [--registry <name>]
warehouse crates unyank <crate_name> <version> [--registry <name>]
```

### Files

```bash
warehouse files registry add <name> --url <url> [--base-path /warehouse] [--insecure-tls] [--use] [--global]
warehouse files registry list
warehouse files registry use <name> [--global]
warehouse files registry remove <name> [--global]
warehouse files storages [--registry <name>]
warehouse files ls <storage> [--path <remote-path>] [--registry <name>]
warehouse files upload <storage> <local_files...> [--remote-dir <dir>] [--registry <name>]
warehouse files preview <storage> <path> [--registry <name>]
warehouse files download <storage> <path> [--output <file>] [--registry <name>]
warehouse files mkdir <storage> <path> [--registry <name>]
warehouse files rmdir <storage> <path> [--registry <name>]
warehouse files delete <storage> <path> [--registry <name>]
warehouse files bulk-delete <storage> <paths...> [--registry <name>]
warehouse files bulk-download <storage> <paths...> [--output files-bulk.zip] [--registry <name>]
```

### Admin

```bash
warehouse admin gc [--registry <name>] [--docker] [--crates]
```

With neither `--docker` nor `--crates`, both run; either flag alone scopes garbage collection to that registry kind.

## Configuration

Registry configuration is stored per kind (Docker, crates, files) under `.warehouse/registries/` in the current directory (local, the default) or `~/.config/warehouse/registries/` (`--global`). `registry add ... --use` and `registry use <name>` set which configured registry is active for commands that omit `--registry`.

[Home](../README.md)
