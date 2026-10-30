---
title: Registry commands
parent: warehouse-cli
grand_parent: Command-line tools
nav_order: 1
---

`warehouse docker registry`, `warehouse crates registry`, `warehouse files registry`: manage the named registry files. Verified locally.

All three kinds read and write the **same** files `registries/<name>.toml`; each kind only fills its own table (`[docker]`, `[crates]`, `[files]`) and has its own *active registry* in `config.toml`. Names must be non-empty and contain no `/` or `\`, and not be `.` or `..`.

## add

```text
warehouse docker registry add <NAME> --url <URL> [--base-path <PATH>] [--path <PATH>] [--service <SERVICE>] [--insecure-tls] [--use] [--global]
warehouse crates registry add <NAME> --url <URL> [--base-path <PATH>] [--insecure-tls] [--use] [--global]
warehouse files  registry add <NAME> --url <URL> [--base-path <PATH>] [--insecure-tls] [--use] [--global]
```

| Argument/flag | Kind | Default | Meaning |
|---|---|---|---|
| `<NAME>` | all | required | Registry name. |
| `--url <URL>` | all | required | Base URL (`http://registry.local:8698`). Trimmed, trailing `/` removed. For docker it is `[docker] url`, for crates `[crates] url`, for files `[files] url`. |
| `--base-path <PATH>` | all | `""` | Service base path for crates/files/admin endpoints (`/warehouse`). Normalised: leading `/` added, trailing `/` removed, `/` alone becomes empty. Stored once per registry file (`base_path`), shared by all kinds. |
| `--path <PATH>` | docker | `/v2` | Docker API path segment, normalised (leading `/`; empty becomes `/v2`). |
| `--service <SERVICE>` | docker | none | Docker token `service` value; when omitted it is derived from the auth challenge or the URL host. |
| `--insecure-tls` | all | off | Skip TLS certificate validation for this kind. |
| `--use` | all | off | Make it the active registry of this kind. It is also made active automatically when the kind has no active registry yet. |
| `--global` | all | off | Write to `~/.config/warehouse/` instead of `.warehouse/`. |

Adding an existing name updates it (the other kinds' tables and stored credentials are kept). Output: `warehouse registry '<name>' saved` (docker), `warehouse crates registry '<name>' saved`, `warehouse files registry '<name>' saved`.

**Gotcha (verified):** `--base-path` defaults to the empty string and is written on every `add`, so `crates registry add main --base-path /warehouse` followed by `files registry add main --url ...` (without `--base-path`) resets the shared `base_path` to empty. Pass `--base-path` on each `add` of the same name.

## list

```text
warehouse docker registry list
warehouse crates registry list
warehouse files  registry list
```

One line per registry, local entries shadowing global ones of the same name; `*` marks the active registry of that kind.

```text
$ warehouse docker registry list
* main -> http://registry.local:8698/v2 (local)
  second -> http://other:5000/v2 (global)
$ warehouse crates registry list
* main -> https://reg.example.com (local)
```

Formats: docker `<marker> <name> -> <url><path>[ base-path=<bp>] (<local|global>)`; crates `<marker> <name> -> <url>[ base-path=<bp>][ [token set]] (<source>)`; files `<marker> <name> -> <url>[ base-path=<bp>] (<source>)`. Differences (verified): the **docker and files lists show every registry file** even if it has no URL for that kind (`  second ->  (global)` for a docker-only registry), while the crates list shows only registries with a crates URL. With none: `warehouse no registries configured` (docker, files) or `warehouse no crates registries configured`.

## use

```text
warehouse docker registry use <NAME> [--global]
warehouse crates registry use <NAME> [--global]
warehouse files  registry use <NAME> [--global]
```

Sets the active registry of that kind in `.warehouse/config.toml` (or `--global`). The registry file must exist **in that scope** (`registry '<name>' does not exist`, verified). Output: `warehouse active registry set to '<name>'` (docker), `active crates registry set to '<name>'`, `using files registry '<name>'`.

## remove

```text
warehouse docker registry remove <NAME> [--global]
warehouse crates registry remove <NAME> [--global]
warehouse files  registry remove <NAME> [--global]
```

Deletes `registries/<name>.toml` of the scope, **for all kinds** (the file is shared). If it was the active registry of this kind the active setting is cleared (the other kinds' active settings are not touched). Errors: `registry '<name>' does not exist in .warehouse/registries` (verified). Output: `warehouse registry '<name>' removed` (docker), `crates registry '<name>' removed`, `files registry '<name>' removed`.
