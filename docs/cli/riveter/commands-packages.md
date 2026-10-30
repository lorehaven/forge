---
title: Packages and Warehouse
parent: riveter
grand_parent: Command-line tools
nav_order: 2
---

`pack`, `publish`, `pull`, `install`, `remote` and `login`. Unverified (not run: they contact a registry, Warehouse, Gatehouse or a cluster): all of them except argument errors. Documented from `package_cmd.rs`, `package.rs`, `registry.rs`, `login_cmd.rs`.

A package is one overlay directory. Its `rivet.toml` (see [Configuration](configuration.md#rivettoml)) names it and carries the version. Build metadata is added at pack time with `--version-suffix`.

## pack

```text
riveter pack [--version-suffix <SUFFIX>] [--no-pin] [--out <DIR>] [--registry-auth <REGISTRY=USER:PASS>]...
```

| Flag | Default | Meaning |
|---|---|---|
| `--version-suffix <SUFFIX>` | none | Build metadata appended as `+<SUFFIX>`. `{timestamp}` expands to the UTC time `YYYYMMDDHHMMSS`, `{sha}` to the first 7 characters of `$CONVEYOR_SHA` (else `$GITHUB_SHA`). A token that cannot be expanded is an error. The manifest version must not already carry build metadata. |
| `--no-pin` | off | Do not pin `image:` tags to digests; then packing needs no registry access. |
| `--out <DIR>` | `packages` | Output directory. |
| `--registry-auth` | none | As for [`images`](commands.md#images). |

Uses the environment from `--env`/`RIVETER_ENV`/`env set`. Writes `<out>/<name>-<version>.rivet`. What goes in: every regular file of `overlays/<env>/` except dotfiles (so `.env` is never packed; `.env.example` is) and generated files; a symlink is an error; includes that leave the directory are refused; a `secret` resource may only hold `${VAR}` placeholders (and a `raw` resource may not declare a Secret). Unless `--no-pin`, every `image:` line naming a tag is rewritten to `repo:tag@sha256:<digest>` (one `HEAD` per image, Bearer/Basic flows); unresolvable images are reported as `not pinned: ...`, but a lookup failure aborts the pack.

Output:

```text
ok packed forge 0.4.0+20261030120000.abc1234 -> packages/forge-0.4.0+20261030120000.abc1234.rivet (18.2 KiB, sha256 0123456789ab)
ok pinned 5 image(s) to digests
ok left out: .env
ok installing needs these variables (no default in values.toml): DB_PASSWORD
```

*Unverified:* shape taken from `report_packed`; values are illustrative. Packing is reproducible: same input, same bytes. If the output directory has no `.gitignore`, one containing `*.rivet` is written.

## publish

```text
riveter publish [FILE] [--version-suffix <SUFFIX>] [--skip-unchanged] [--no-pin] [--out <DIR>] [--registry-auth ...]
```

Without `FILE`, packs the current environment first (same flags as `pack`); with `FILE`, publishes that `.rivet` file. `--skip-unchanged` fetches the newest published version and compares content fingerprints (files, pinned digests, manifest and base version; not build metadata or archive order); equal means `ok <name> is unchanged since <version>; nothing published`. Otherwise `PUT {warehouse}/api/v1/rivets/{name}/{version}` with `Content-Type: application/zstd` and prints `ok published <name> <version> (sha256 <12 hex>)`. Needs the `warehouse:write` grant. Errors: 401 `Warehouse rejected the credentials (HTTP 401)`; 403 `Warehouse denied this action (HTTP 403): publishing needs the warehouse:write grant`; 409 `already published`; others `Warehouse answered HTTP <code>`. A version can be published once.

## pull

```text
riveter pull <PACKAGE> [--out <DIR>]
```

`PACKAGE` is `name`, `name@version` or `name@latest`. Downloads, verifies the digest Warehouse recorded and validates the archive, writes `<out>/<name>-<version>.rivet` (default `packages`) and prints `ok pulled <name> <version> -> <path>`. A `.rivet` path is refused (`is a file; pull takes a package name`). Package names are lowercase `a-z`, `0-9`, `-`.

## install

```text
riveter install <PACKAGE> [--env-file <FILE>] [--set <KEY=VALUE>]... [--replicas <KIND/NAME=N>]...
                [--except <KIND/NAME>]... [--inventory] [--dry-run] [--no-wait] [--timeout <SECONDS>]
                [--scope <SCOPE>] [TARGET]...                                  alias: i
```

| Argument/flag | Default | Meaning |
|---|---|---|
| `PACKAGE` | required | `name`, `name@version` (from Warehouse) or a `.rivet` file (anything ending `.rivet`, or an existing file). |
| `--env-file <FILE>` | none | dotenv-format variables, e.g. an overlay's `.env`. |
| `--set <KEY=VALUE>` | none | One variable; repeatable; wins over everything. |
| `--replicas <KIND/NAME=N>` | none | Force replicas of a Deployment or StatefulSet (`deployment/sage=0` installs it stopped); must name an existing resource that has replicas. |
| `--except <KIND/NAME>` | none | Leave resources out (patterns as for targets). |
| `--inventory` | off | Also print `riveter-inventory: [{"apiVersion","kind","name","namespace"}, ...]` (one JSON line, everything the package declares whatever was selected). |
| `--dry-run`, `--no-wait`, `--timeout` | as `apply` | |
| `--scope` | **`all`** | Unlike `apply`, because a package is installed whole. |
| `[TARGET]...` | all | As for `apply`. |

Steps: parse the reference; fetch (Warehouse) or read the file; validate; check `requires.riveter` (an unsatisfied requirement is an error: `<name> <version> requires riveter <req>, but this is riveter <version>`; each `requires.packages` entry only produces a warning that it is not verified); open `secrets.yaml` if present (needs `RIVETER_AGE_KEY` or `RIVETER_AGE_KEY_FILE`, or `~/.config/riveter/age.key`); merge variables, lowest precedence first: the package's `values.yaml`/`values.toml`, its `secrets.yaml`, `--env-file`, `--set` (**the working directory's `.env` is not read**); unpack to a scratch tree; create the package namespace if the cluster lacks it (`namespace  creating <ns>`; skipped for `--dry-run`); render and apply as `apply` does. A variable nobody supplies is an error naming it. Every installed resource is stamped with label `riveter.forge/package: <name>` and annotation `riveter.forge/package-version: <version>`. Nothing is written to the working directory and the scratch tree is deleted afterwards. Prints `ok installing <name> <version> from <source>` and `ok applied N resource(s): ...`.

## remote

```text
riveter remote list              alias: ls
riveter remote versions <NAME>
```

`list` prints the newest version of every package as rows `name, version, description, size` (`ok the registry holds no packages` when empty). `versions <NAME>` prints `name, version, status, size`, status `available` or `yanked` (`ok no versions of <name>`). Uses `GET {warehouse}/api/v1/rivets...`.

## login

```text
riveter login [--gatehouse-url <URL>] [--username <NAME>] [--name <TOKEN_NAME>] [--scope <SERVICE:ACTION>]... [--days <N>]
```

Same flow as `anvil login` (see [anvil](../anvil/commands-credentials.md#anvil-login)) with these differences: the URL default is `$RIVETER_GATEHOUSE_URL` (a `.env` works), then the URL already stored; the default token name is `riveter on <HOSTNAME>`; the secret file is `.riveter.secret.toml` (riveter has no `.riveter/` directory by default; `.riveter/secret.toml` is used if that directory exists). Prints `Stored an application token in <path>` and `It is git-ignored; revoke it from Gatehouse if the machine is lost.`. Error without a URL: `no Gatehouse URL: pass --gatehouse-url, or set RIVETER_GATEHOUSE_URL (a .env works)`.

## Warehouse credentials

`publish`, `pull`, `install` (of a remote package) and `remote` build a client the same way (`Registry::from_env`; a `.env` in the working directory is loaded first):

1. Base URL: `RIVETER_WAREHOUSE_URL`, else `warehouse_url` in `.riveter.toml`. Includes the base path (`https://host/warehouse`). Else: `RIVETER_WAREHOUSE_URL is not set - point it at Warehouse, including its base path ...`.
2. Token, first match: `RIVETER_WAREHOUSE_TOKEN`; the pair-plus-one `RIVETER_GATEHOUSE_URL` + `RIVETER_CLIENT_ID` + `RIVETER_CLIENT_SECRET` (client-credentials grant); the stored application token from `riveter login` (exchanged for a short-lived token); else `no credentials: run riveter login, or set RIVETER_WAREHOUSE_TOKEN, or RIVETER_GATEHOUSE_URL with RIVETER_CLIENT_ID and RIVETER_CLIENT_SECRET`.
3. Timeout 120 s.
