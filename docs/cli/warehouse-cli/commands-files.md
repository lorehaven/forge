---
title: Files commands
parent: warehouse-cli
grand_parent: Command-line tools
nav_order: 3
---

`warehouse files ...`: plain file storage on the Warehouse. Unverified (not run: they talk to a Warehouse); documented from `application.rs` and `api/files_api.rs`.

Registry selection: `--registry <NAME>`, else the active **files** registry (`resolve_files_registry_name`). The `--help` texts of these options say "defaults to active docker registry"; the code uses the files one (see the [report](../../_reports/cli.md)). The service URL is `[files] url`, else `[docker] url`, else `[crates] url`, followed by the registry's `base_path`. **No Authorization header is sent** by any `files` command.

Endpoints are under `/api/v1/files/<storage>/...`; paths are percent-encoded.

| Command | Request |
|---|---|
| `storages` | `GET /api/v1/files/storages` |
| `ls` | `GET /api/v1/files/<storage>/entries?path=<path>` |
| `upload` | `PUT /api/v1/files/<storage>/file?path=<remote path>` (`application/octet-stream`) |
| `preview` | `GET /api/v1/files/<storage>/preview?path=<path>` |
| `download` | `GET /api/v1/files/<storage>/download?path=<path>` |
| `mkdir` | `POST /api/v1/files/<storage>/folder?path=<path>` |
| `rmdir` | `DELETE /api/v1/files/<storage>/folder?path=<path>` |
| `delete` | `DELETE /api/v1/files/<storage>/file?path=<path>` |
| `bulk-delete` | `DELETE /api/v1/files/<storage>/bulk` (JSON body of paths) |
| `bulk-download` | `POST /api/v1/files/<storage>/bulk-download` (JSON body of paths) |

## storages

```text
warehouse files storages [--registry <NAME>]
```

Prints `registry: <name>` and `<storage> -> <root>` per storage, or `warehouse no storages configured`.

## ls

```text
warehouse files ls <STORAGE> [--path <PATH>] [--registry <NAME>]
```

`--path` default `""` (root). Output: `registry:`, `storage:`, `path: /<path>`, blank line, then `dir  `/`file` rows `<kind> <size right-aligned to 10> <path>` (size `-` for directories), or `(empty)`.

## upload

```text
warehouse files upload <STORAGE> <LOCAL_FILE>... [--remote-dir <DIR>] [--registry <NAME>]
```

At least one file (`at least one local file is required`). Each file is read whole into memory and uploaded to `<remote-dir>/<file name>` (the directory is trimmed of `/`; without it the file name alone). Prints `warehouse uploaded <local> -> <remote path>` per file.

## preview

```text
warehouse files preview <STORAGE> <PATH> [--registry <NAME>]
```

Prints `storage:`, `path:`, `kind:`, `truncated: true|false`, a blank line and the content.

## download

```text
warehouse files download <STORAGE> <PATH> [--output <FILE>] [--registry <NAME>]
```

A folder is downloaded as a zip. The output name is `--output`, else the server's `Content-Disposition` file name, else `download.bin`. Prints `warehouse saved <file>`.

## mkdir / rmdir / delete

```text
warehouse files mkdir  <STORAGE> <PATH> [--registry <NAME>]       -> folder created: <path>
warehouse files rmdir  <STORAGE> <PATH> [--registry <NAME>]       -> folder deleted: <path>   (recursive)
warehouse files delete <STORAGE> <PATH> [--registry <NAME>]       -> file deleted: <path>
```

There is no confirmation prompt.

## bulk-delete

```text
warehouse files bulk-delete <STORAGE> <PATH>... [--registry <NAME>]
```

At least one path (`at least one path is required`). Prints `warehouse bulk delete complete`.

## bulk-download

```text
warehouse files bulk-download <STORAGE> <PATH>... [--output <ZIP>] [--registry <NAME>]
```

`--output` default `files-bulk.zip`. At least one path required. Prints `warehouse saved <zip>`.
