---
title: Docker, crates and admin commands
parent: warehouse-cli
grand_parent: Command-line tools
nav_order: 2
---

Unverified (not run: each contacts a Warehouse): `catalog`, `tags`, `search`, `versions`, `yank`, `unyank`, `admin gc`. `docker login` and `crates login` only write the local config (docker login verified). Documented from `application.rs` and `api/*.rs`.

Every command takes `--registry <NAME>` (default: the active registry of its kind; errors `no active docker registry; use warehouse docker registry add ... --use or warehouse docker registry use ...`, and the same for `crates` and `files`; an unknown name: `registry '<name>' does not exist`).

## docker login

```text
warehouse docker login --username <USER> --password <PASS> [--registry <NAME>] [--global]
```

Saves `username`/`password` into the registry's `[docker]` table. Both flags are required; the password is given on the command line (shell history) and stored in plain text in `registries/<name>.toml` (file mode `0644` as created by the process umask; observed `-rw-r--r--`). `--global` writes the (effective) registry to `~/.config/warehouse/`. Output: `warehouse credentials saved for '<name>'`.

## docker catalog

```text
warehouse docker catalog [--registry <NAME>] [--n <N>]
```

`--n` default `100`: page size (`GET {url}{path}/_catalog?n=<N>`). The first request is unauthenticated. Only on `401` with a Bearer challenge does the CLI fetch a token from the challenge's realm with Basic credentials (the saved ones; errors `missing username; run warehouse docker login`, `missing password; ...`), with `service` taken from the challenge, else `--service` given at add time, else the URL host, and no `scope` parameter for the catalog; then it repeats the request. A `401` without a Bearer challenge is `registry returned 401 without a Bearer challenge`. A connection error on http is retried over https. Output:

```text
registry: main
<repository>
<repository>
```

*Unverified:* shape from the source.

## docker tags

```text
warehouse docker tags <REPOSITORY> [--registry <NAME>] [--n <N>]
```

`GET {url}{path}/<repository>/tags/list?n=<N>` with the same flow and token scope `repository:<repository>:pull`. Output: `registry: <name>`, `repository: <name>`, then one tag per line.

## crates login

```text
warehouse crates login --token <TOKEN> [--registry <NAME>] [--global]
```

Saves the API token (as given to `cargo login`) in the registry's `[crates]` table (plain text). Output: `warehouse token saved for crates registry '<name>'`.

## crates search

```text
warehouse crates search <QUERY> [--registry <NAME>] [--limit <N>]
```

`--limit` default `10`. `GET {url}{base_path}/api/v1/crates?q=<query>&per_page=<N>` (bearer token if saved). Output: `registry: <name>`, `query: "<q>"  (<total> total)`, a blank line, then rows `<name>  <max_version>  <description>` with aligned columns, or `warehouse no crates found`.

## crates versions

```text
warehouse crates versions <CRATE> [--registry <NAME>] [--all]
```

Reads the sparse index: `GET {url}{base_path}/index/<prefix>/<crate>` where the prefix follows the crates.io convention (`1`, `2`, `3/<first letter>`, `<first two>/<next two>`; the crate name is lower-cased for the prefix). `--all` includes yanked versions. Output, newest first:

```text
registry: main
crate: quench-args

version               status    checksum
------------------------------------------------------------------------
0.1.4                 active    0123456789abcdef…
```

Warnings: `warehouse no versions found` (`--all`), `no active versions (use --all to include yanked)`. 404: `crate '<name>' not found`.

## crates yank / unyank

```text
warehouse crates yank   <CRATE> <VERSION> [--registry <NAME>]
warehouse crates unyank <CRATE> <VERSION> [--registry <NAME>]
```

Require a saved token (`no token set for '<name>'; run warehouse crates login --token <token>`). `yank` sends `DELETE {url}{base_path}/api/v1/crates/<crate>/<version>/yank`, `unyank` sends `PUT .../unyank`. Output: `warehouse yanked <crate>-<version> from '<name>'` / `unyanked <crate>-<version> in '<name>'`.

## admin gc

```text
warehouse admin gc [--registry <NAME>] [--docker] [--crates]
```

Runs garbage collection on the Warehouse service. The registry defaults to the **active docker registry** (not the crates one). With neither flag both run; `--docker` alone runs only Docker GC; `--crates` alone only crates GC. The requests are `POST {url}{base_path}/admin/docker/gc` (Basic auth from the saved docker username/password if present) and `POST .../admin/crates/gc` (Bearer from the saved crates token if present). The base URL is the registry's crates URL, or its docker URL when the crates URL is empty. Output:

```text
warehouse running garbage collection for registry 'main'
warehouse running Docker garbage collection...
warehouse Docker GC completed
  Deleted: 3
  Kept: 40
warehouse running crates garbage collection...
warehouse Crates GC completed
  Deleted crates: 1
  Kept crates: 120
  Removed index entries: 0
  Deleted owner files: 0
  Removed empty dirs: 2
```

A failure prints `Docker GC failed: <error>` / `Crates GC failed: <error>` and stops with exit 1 (the later GC does not run). *Unverified:* sample constructed from the source.
