---
title: Credentials, config and artifacts
parent: anvil
grand_parent: Command-line tools
nav_order: 3
---

`login`, `keystore-password`, `registry-token`, `config check` and `artifact`. Unverified (not run: touch a real service or credentials): `login`, `keystore-password`, `registry-token`, `artifact release`. Verified locally with a scratch project: `config check`, `artifact list`, and the "unknown program" error of `artifact release`.

## anvil login

```text
anvil login [--gatehouse-url <URL>] [--username <NAME>] [--name <TOKEN_NAME>] [--scope <SERVICE:ACTION>]... [--days <N>]
```

Signs in to Gatehouse with a username and password, mints a long-lived application token and stores it in the secret file. The password is read from the terminal without echo, used for this one call and never stored.

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--gatehouse-url` | string | `[release].gatehouse_url`, else the URL already in the secret file | Gatehouse including its base path (`https://host/gatehouse`). Trailing `/` is trimmed. Error if none: `no Gatehouse URL: pass --gatehouse-url, or set gatehouse_url under [release] in .anvil.toml or .anvil/`. |
| `--username` | string | asked: `Gatehouse username: ` | Must not be empty. |
| `--name` | string | `anvil on $HOSTNAME` (`anvil on this machine` if `HOSTNAME` is unset or empty) | Label next to the token in Gatehouse. |
| `--scope` | string, repeatable | none (the owner's full access) | A `service:action` grant, e.g. `warehouse:write`. |
| `--days` | integer | server default | Days until expiry; the help states default and maximum 365. |

Requests made (from `gatehouse-auth`): `POST {url}/api/v1/auth/login` with `{"username","password"}`, then `POST {url}/api/v1/me/tokens` with `{"name","scope","expires_in_days"}` and the session as bearer; timeout 30 s per request. Errors: `Gatehouse refused the login (<status>)`, `Gatehouse refused to create the token (<status>): <body>`.

On success it writes the secret file (mode `0600` on Unix), adds its path to the `.gitignore` of the current directory if no line already covers it, and prints:

```text
Stored an application token in .anvil/secret.toml
It is git-ignored; revoke it from Gatehouse if the machine is lost.
```

Which file: `.anvil/secret.toml` if the directory `.anvil/` exists in the current directory, otherwise `.anvil.secret.toml`; a secret file of the other kind is deleted so one login leaves one token. Other contents of the file (the `[android]` table) survive a new login. Use `--scope warehouse:write` for a token that only publishes.

## anvil keystore-password

```text
anvil keystore-password [--key]
```

Stores the Android release keystore password in the secret file's `[android]` table. Run `anvil login` first: the command fails with `no secret file: run anvil login first` otherwise.

| Flag | Meaning |
|---|---|
| `--key` | Also ask for the key's own password (`Key password: `) and store it as `key_password`. Leave it out when the key uses the keystore's password. |

The prompts `Keystore password: ` and `Key password: ` do not echo. An empty keystore password fails with `the keystore password is required`; an empty key password with `the key password is required (leave out --key when it is the keystore's)`. Output on success: `Stored the keystore password in <path>`. (With `--key` the printed message is the same and names only the keystore password.)

`anvil artifact release` reads `keystore_password` and `key_password` back and passes them to every build command as `QUENCH_KEYSTORE_PASSWORD` and `QUENCH_KEY_PASSWORD`, unless the variable is already set in the environment. The file layout is on the [Configuration](configuration.md#the-secret-file) page.

## anvil registry-token

```text
anvil registry-token
```

Prints one line, `Bearer <short-lived access token>`, for use as a Cargo credential provider. No banner is printed (stdout is data). Credentials: the environment pair `ANVIL_APP_TOKEN` + `ANVIL_GATEHOUSE_URL` when both are set and non-empty, else the secret file found from the working directory upwards. Error without either: `no credentials: run anvil login, or set ANVIL_APP_TOKEN and ANVIL_GATEHOUSE_URL`. The token comes from `POST {gatehouse_url}/api/v1/token` (form `grant_type=app_token`, `app_token=...`); `401` gives `Gatehouse rejected the application token (revoked, expired or unknown) - run the tool's login again`.

```toml
# ~/.cargo/config.toml
[registries.ennor]
credential-provider = ["cargo:token-from-stdout", "anvil", "registry-token"]
```

The command's `--help` description shows the credential-provider example with literal backslashes before the double quotes (`[\"cargo:token-from-stdout\", ...]`); that is a typo in the help string, the TOML above is the correct form.

## anvil config check

```text
anvil config check
```

Reads `.anvil.toml` and `.anvil/*.toml` from the current directory, merges them, and validates the merged tree against anvil's schema, reporting every problem at once. Unknown fields, missing required fields, wrong types, empty strings, a package built by two modules, a package override for a package that is not in the module's `packages`, both `registry` and `registries` set, `format` missing on a non-Android artifact target. Exit 0 and `the configuration is valid (<files>)` when clean; otherwise the issues sorted by path and exit 1. The full rule list is in [Configuration](configuration.md#what-config-check-reports).

Verified output (scratch project with three files):

```text
anvil 4 errors in the configuration (.anvil/artifacts.toml, .anvil/docker.toml, .anvil/release.toml):
  artifacts.programs.hoist.targets[1]: `format` is required off Android (`deb`, `tar.gz`, ...): the warehouse takes it from the upload
  docker.modules.other: missing required field `dockerfile`
  docker.modules.other.packages: package `svc` is also built by module `core`
  release.bogus: unknown field `bogus`
Error: 4 error(s) in the configuration
```

(The same run also prints, before the check, `anvil failed to read the configuration (...), defaulting to empty config`: every command first loads the configuration leniently.) A file that is not TOML, or a value set in two files, stops the check at once: `` `x` is set in both ./.anvil.toml and ./.anvil/a.toml `` and `config check failed`.

## anvil artifact

```text
anvil artifact release (<NAME> | --all [-y | --yes]) [--target <T>] [--dry-run] [--allow-dirty] [--skip-existing] [--bump | --no-bump]
anvil artifact list
```

### artifact list

Prints each program as `<name> (<program id>)` and one line per target: name padded to 10 columns, format (`-` if none), arch (`-` if none); then `image    <module>/<name>` for a service image. With no programs: `no artifacts: add [artifacts.programs.<name>] to .anvil.toml or a file in .anvil/`.

```text
hoist (dev.lorehaven.hoist)
  android    apk -
  linux-deb  - x86_64
```

### artifact release

| Argument/flag | Meaning |
|---|---|
| `<NAME>` | Program key under `[artifacts.programs]`. Unknown: `no artifact <name> in the anvil configuration (known: a, b)` (verified). Conflicts with `--all`; without either: `name a program (see anvil artifact list) or pass --all to release every one`. |
| `--all` | Every program of `[artifacts.programs]`, each with all its targets, in name order, stopping at the first that fails (the error names it: `releasing <name>`). Prints `N program(s): a, b`, asks once for confirmation (`Proceed with the release? (yes/no)`; a dry run does not ask) and, unlike a single program, looks after versions (see below). No programs configured is an error. |
| `-y`, `--yes` | With `--all`: do not ask. With no answer at all (standard input closed) the question is an error, not a "no": `pass --yes to release without asking`. |
| `--bump` | Look after the version (below). On by default with `--all`. |
| `--no-bump` | With `--all`: do not touch versions; a version the warehouse has is a `409` error as before. Conflicts with `--bump`. |
| `--target <T>` | Only the target whose `name` (default: its `platform`) is `T`. `--target image` releases only the program's `image`. No match: `no target <T>: this program has x, y`. |
| `--dry-run` | Build everything, check the files, send nothing; no token is requested. |
| `--allow-dirty` | Release with uncommitted changes (the commit is printed as `<hash>-dirty`). |
| `--skip-existing` | A `409` from the warehouse is reported as skipped, not an error. |

Steps:

1. Needs `[artifacts].warehouse_url` (`no warehouse: set warehouse_url under [artifacts] in .anvil.toml or .anvil/`).
2. Version: `version_file` (relative to `dir`, default the current directory) is parsed; `[workspace.package] version`, else `[package] version`. `version_code = major*1_000_000 + minor*1_000 + patch` (minor and patch capped at 999; `0.1.3` gives `1003`).
3. The working tree of `dir` must be clean (`git status --porcelain`), else `uncommitted changes:` plus up to 8 lines. Not inside a repository: no check.
4. Header line: `<name> <version> (version_code <code>), commit <hash> -> <warehouse_url>`.
5. For each selected target: print `[<label>] building: <build...>`, run `build` in `dir` with `${version_name}`, `${version_code}` and `${program}` replaced in every argument, environment `ANVIL_VERSION_NAME`, `ANVIL_VERSION_CODE` and the stored signing passwords; read `file` (placeholders replaced; error `[label] the build left no file at <path>`).
6. Dry run: `[label] would publish <program> <platform> <version> (<n> KiB)` and `    PUT <url>`. Otherwise fetch a token once (as `registry-token`, minus `Bearer `) and `PUT` the file.
7. The service image (if configured and no other `--target` was given, or `--target image`) is built and pushed last, tagged `<registry>/<module>/<name>:<version_name>` for every registry (`registries` of the image, else `[docker].registry`); dry run prints `[image] would build <Dockerfile> and push <tags>`.

Upload request: `PUT {warehouse_url}/api/v1/artifacts/{program}/{platform}/{version_code}?[format=..&][arch=..&]version_name=..&[label=..&]filename=..` (query values percent-encoded), `Authorization: Bearer <token>`, `Content-Type: application/vnd.android.package-archive` for APK/Android, else `application/octet-stream`. Responses: `201` prints `[label] published`; `409` is an error (`... is already published, and versions are immutable: raise the version (or --skip-existing to leave this one)`) unless `--skip-existing` (`already published, skipped`); `401`/`403` say `anvil login --scope warehouse:write`; anything else `publish failed: HTTP <status> <body>`; a connection failure `the warehouse could not be reached: <error>`.

Builds run sequentially and a failure stops the release; a release that stopped half-way is finished by running it again with `--skip-existing`.

### Looking after the version (`--bump`, and `--all`)

Versions are immutable in the warehouse, so a release of a version it already has can only be a new version. With `--bump` (default under `--all`) anvil first asks the warehouse what it holds: `GET {warehouse_url}/api/v1/artifacts/{program}` with the same bearer token (the token is requested even for a dry run). Per program it compares the targets it would build, as `(platform, format, version_code)` (a target without `format` on Android counts as `apk`; yanked builds count, they still hold the version), with the list:

| The warehouse holds | Then |
|---|---|
| none of the builds at this `version_code` | the version is released as it is |
| all of them | the patch version is raised (`0.1.3` -> `0.1.4`) in `version_file` (its `[package]`, else its `[workspace.package]` version), the working tree must be clean first (else the usual `uncommitted changes` error, before anything is edited), `Cargo.lock` is refreshed (`cargo update --workspace --offline`), and the file and the lock are committed as `release: <name> <version>` when `dir` is in a git repository; then the builds run with the new version. **The commit is not pushed**: pushing is for whoever runs this (a pipeline does it once at the end). |
| some of them (a release that stopped half way) | the version stays; the builds that are there are not built again (`[android] already in the warehouse, not built again`), the others are sent, and a `409` is skipped as with `--skip-existing` |

A dry run with a raised version prints `<name> <from>: the warehouse has every build of it; a real run would release <to> (nothing is built or changed now)` and builds nothing for that program. If the warehouse cannot be asked (any status but `200`): `<name>: the warehouse could not say which versions it holds (HTTP <n>)...`; run without `--bump` to manage the version yourself. A version that is not `MAJOR.MINOR.PATCH` cannot be raised and is an error.
