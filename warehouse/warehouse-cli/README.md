# Warehouse CLI

Command line client for interacting with the Warehouse service.

## Run

```bash
cargo run -p warehouse-cli -- --help
```

## Command Help

Top-level commands:

- `warehouse docker ...`: Docker registry operations
- `warehouse crates ...`: Cargo registry operations
- `warehouse files ...`: file storage operations
- `warehouse admin ...`: admin/maintenance operations

Docker command group:

- `warehouse docker registry add <name> --url <url> [--path /v2] [--service <svc>] [--insecure-tls] [--use] [--global]`
- `warehouse docker registry list`
- `warehouse docker registry use <name> [--global]`
- `warehouse docker registry remove <name> [--global]`
- `warehouse docker login --username <user> --password <pass> [--registry <name>] [--global]`
- `warehouse docker catalog [--registry <name>] [--n <page-size>]`
- `warehouse docker tags <repository> [--registry <name>] [--n <page-size>]`

Crates command group:

- `warehouse crates registry add <name> --url <url> [--insecure-tls] [--use] [--global]`
- `warehouse crates registry list`
- `warehouse crates registry use <name> [--global]`
- `warehouse crates registry remove <name> [--global]`
- `warehouse crates login --token <token> [--registry <name>] [--global]`
- `warehouse crates search <query> [--registry <name>] [--limit <n>]`
- `warehouse crates versions <crate_name> [--registry <name>] [--all]`
- `warehouse crates yank <crate_name> <version> [--registry <name>]`
- `warehouse crates unyank <crate_name> <version> [--registry <name>]`

Files command group:

- `warehouse files storages [--registry <name>]`
- `warehouse files ls <storage> [--path <remote-path>] [--registry <name>]`
- `warehouse files upload <storage> <local_files...> [--remote-dir <dir>] [--registry <name>]`
- `warehouse files preview <storage> <path> [--registry <name>]`
- `warehouse files download <storage> <path> [--output <file>] [--registry <name>]`
- `warehouse files mkdir <storage> <path> [--registry <name>]`
- `warehouse files rmdir <storage> <path> [--registry <name>]`
- `warehouse files delete <storage> <path> [--registry <name>]`
- `warehouse files bulk-delete <storage> <paths...> [--registry <name>]`
- `warehouse files bulk-download <storage> <paths...> [--output files-bulk.zip] [--registry <name>]`

Admin command group:

- `warehouse admin gc [--registry <name>] [--docker] [--crates]`
  - no `--docker`/`--crates`: run both
  - `--docker`: docker GC only
  - `--crates`: crates GC only
