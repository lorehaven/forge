---
title: Configuration
parent: anvil
grand_parent: Command-line tools
nav_order: 5
---

Every file, key and default that anvil reads. Source: `forge/cli/anvil/src/config.rs` and `commands/config_check.rs`.

## Files and merging

Anvil reads, from the **current directory** (no search upwards):

1. `.anvil.toml`
2. every `*.toml` file directly in `.anvil/`, sorted by file name, **except** `secret.toml`. Sub-directories of `.anvil/` and non-`.toml` files are ignored.

All files are merged into one tree:

* a table present in several files becomes one table holding the keys of all of them (`[docker.modules.core]` in one file and `[docker.modules.core.worker]` in another);
* any other value (string, number, boolean, array) set in two files is an error naming both files: `` `docker.registry` is set in both ./.anvil.toml and ./.anvil/docker.toml ``;
* no file at all: `no .anvil.toml and no .anvil/*.toml in .`.

Every command except `config check` loads leniently: a missing, unparsable or invalid configuration prints a warning (`anvil <reason>, defaulting to empty config`, suppressed for `--json` and `registry-token`) and continues with defaults. Typical layout (files are named freely):

```text
.anvil/docker.toml      [docker] and its modules
.anvil/install.toml     [install]
.anvil/release.toml     [release]
.anvil/artifacts.toml   [artifacts] (warehouse_url)
.anvil/hoist.toml       [artifacts.programs.hoist]
.anvil/secret.toml      credentials (not configuration)
```

Add `.anvil/` to `.dockerignore`. Keys not listed below are silently ignored by every command except `config check`, which reports them as unknown fields.

## [docker]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `registry` | string | `""` | Registry prefix for image tags. Optional only if every package sets `registries`. |
| `cargo_registry` | string | none | Name of a private cargo registry the Dockerfile build needs; forwarded as `CARGO_REGISTRIES_<NAME>_INDEX` / `_TOKEN` build args. |
| `cargo_registry_index` | string | none | Index URL override for that registry (then `~/.cargo/config.toml`). Env `CARGO_REGISTRIES_<NAME>_INDEX` wins over it. |
| `modules` | table of module tables | empty | See below. |

### [docker.modules.\<module\>]

| Key | Type | Required | Meaning |
|---|---|---|---|
| `packages` | array of string | yes, non-empty | Workspace packages built from this module. A package may be in one module only. |
| `dockerfile` | string | yes | Dockerfile path used for every package of the module unless overridden. |
| `<package>` | table | no | Per-package override, flattened into the module table (`[docker.modules.core.worker]`). |

### [docker.modules.\<module\>.\<package\>]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `dockerfile` | string | module's | Dockerfile for this package. |
| `module_name` | string | module key | Middle part of the tag: `<registry>/<module_name>/<image>:<version>`. |
| `image_name` | string | package name | Local image name and last part of the tag. |
| `registries` | array of string | `[]` | Registries to tag and push to. |
| `registry` | string | none | Deprecated single registry. Setting it **and** `registries` is reported by `config check`. |
| `build_args` | table string->string | `{}` | Extra `--build-arg K=V`, in key order, after the derived `PROJECT_NAME` and `RESOURCES_PATH`. |

## [install]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `packages` | array of string | `[]` | Packages `install --all` installs; the first one is the default for `install` without `-p`. Packages here are also installed after `cargo publish` in `release`. |

## [release]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `registry` | string | `""` | Cargo registry name for `cargo publish --registry`; also names the `CARGO_REGISTRIES_<NAME>_TOKEN` variable. |
| `packages` | array of string | `[]` | Packages `release --all` covers (plus all Docker module packages). |
| `commit_message_template` | string | `release: bump package versions ({summary})` | Placeholders `{summary}`, `{changelog}`, `{count}`. |
| `gatehouse_url` | string | none | Gatehouse (with base path) used by `login` when `--gatehouse-url` is absent. Not a secret. |

## [artifacts]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `warehouse_url` | string | none | Warehouse including base path. Required by `artifact release`. |
| `programs` | table | empty | `[artifacts.programs.<name>]`. |

### [artifacts.programs.\<name\>]

| Key | Type | Required | Meaning |
|---|---|---|---|
| `program` | string | yes | Application id the warehouse files every platform's build under (`dev.lorehaven.hoist`). |
| `dir` | string | no (default `.`) | Directory the builds run in; `version_file` and `file` are relative to it. |
| `version_file` | string | yes | `Cargo.toml` whose version is the release's. |
| `targets` | array of tables | yes, unless `image` is set | The builds, `[[artifacts.programs.<name>.targets]]`. |
| `image` | table | no | A service image released with the program; see below. |

### [[artifacts.programs.\<name\>.targets]]

| Key | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | no (default `platform`) | What `--target` calls it. |
| `platform` | string | yes | `android`, `linux`, `windows` or `macos`. |
| `format` | string | required off Android | `apk` for Android, otherwise `deb`, `rpm`, `pkg.tar.zst`, `tar.gz`, ... (sent as `?format=`). |
| `arch` | string | no | `x86_64`, `aarch64` or `universal`. |
| `label` | string | no | Display label for this build. |
| `build` | array of string | yes | Command and arguments, run in `dir`; `${version_name}`, `${version_code}`, `${program}` replaced in each. |
| `file` | string | yes | File the build leaves, relative to `dir`; same placeholders. |

`platform` values are not validated by `config check` (any non-empty string passes).

### [artifacts.programs.\<name\>.image]

| Key | Type | Required | Meaning |
|---|---|---|---|
| `dockerfile` | string | yes | From `dir`. |
| `context` | string | no (default `.`) | Build context, from `dir`. |
| `module` | string | yes | Middle part of the tag. |
| `name` | string | yes | Image name. |
| `registries` | array of string | no | Default `[docker].registry`. |
| `build_args` | table | no | Extra build args. |

## [docs]

Read by [`anvil docs`](commands-docs.md).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `out` | string | `target/docs` | Where builds go, from the workspace root: `<out>/<book>/`. |
| `jekyll` | string | `jekyll` | The command that runs Jekyll, split on spaces (`bundle exec jekyll`). |
| `warehouse_url` | string | `[artifacts].warehouse_url` | Warehouse including base path. Required by `docs release` (here or under `[artifacts]`). |
| `storage` | string | none | The warehouse files storage the books are kept in. Required by `docs release`. |
| `theme` | string | `forge` | The look of the books: `forge` is the forge services' own (their design tokens, the light and dark modes and six colour palettes chosen by the same cookies, a theme menu), `none` leaves the `just-the-docs` theme as it comes. A file the book has itself (`_sass/custom/custom.scss`, `_includes/head_custom.html`, `header_custom.html`, `title.html`, `nav_footer_custom.html`) is kept. |
| `books` | table | empty | `[docs.books.<name>]`. |

### [docs.books.\<name\>]

| Key | Type | Default | Meaning |
|---|---|---|---|
| `dir` | string | required | The Jekyll source directory, from the workspace root (`docs`, `../quench/docs`). |
| `title` | string | the book's name | Shown in the manifest. |
| `version` | string | none | The book's version, literally. |
| `version_file` | string | none | A `Cargo.toml` whose version is the book's. Used when `version` is not set; without both, today's date is the version. |
| `baseurl` | string | none | The `baseurl` the site is built for. |
| `openapi` | table | none | `[docs.books.<name>.openapi]`: `specs` (required: the directory of the book with one directory per service, each holding an `openapi.yaml`) and `sources` (required: the service's source directories from the workspace root, `{name}` replaced by the service's directory name, e.g. `["services/{name}/service/src"]`). |

```toml
[docs]
out = "target/docs"
storage = "docs"

[docs.books.forge]
dir = "docs"
title = "Forge"

[docs.books.forge.openapi]
specs = "services"
sources = ["services/{name}/service/src"]
```

## The secret file

Written by `anvil login` and `anvil keystore-password` (through the `gatehouse-auth` library), never read as configuration, never printed. Location:

* `.anvil/secret.toml` if the directory `.anvil/` exists where `login` is run;
* otherwise `.anvil.secret.toml`.

Lookup (`find_secret_file`): starting at the working directory and going up through every parent, in each directory `.anvil/secret.toml` first, then `.anvil.secret.toml`; the first found wins. One file in a parent of several checkouts serves them all.

Keys (names only):

| Table | Key | Written by | Meaning |
|---|---|---|---|
| top level | `gatehouse_url` | `login` | Gatehouse including base path. |
| top level | `app_token` | `login` | Long-lived application token. |
| `[android]` | `keystore_password` | `keystore-password` | Becomes `QUENCH_KEYSTORE_PASSWORD` for artifact builds. |
| `[android]` | `key_password` | `keystore-password --key` | Becomes `QUENCH_KEY_PASSWORD`. |

The file is created with mode `0600` on Unix (a plain write elsewhere). A later `login` rewrites `gatehouse_url` and `app_token` and keeps the other keys; `keystore-password` changes one key and leaves the rest. `login` also appends the secret path to `.gitignore` in the current directory unless it already has a line equal to that path, `.*.secret.toml` or `.*/secret.toml`.

For CI without a file, `ANVIL_APP_TOKEN` and `ANVIL_GATEHOUSE_URL` replace `gatehouse_url`/`app_token` for `registry-token` and `artifact release`. `release` (its `cargo publish`) reads the same variables when there is no file, so a CI job can publish.

## What `config check` reports

All paths are dotted, sorted alphabetically.

| Message | When |
|---|---|
| `unknown field <k>` | A key not listed in the tables above, at the top level or in `docker`, `docker.modules.<m>.<package>`, `install`, `release`, `artifacts`, `artifacts.programs.<p>`, `...image`, `...targets[i]`, `docs`, `docs.books.<b>`, `docs.books.<b>.openapi`. (`docker.modules.<m>` accepts any other key as a package override and checks it as one.) |
| `missing required field <k>` | `dockerfile`/`packages` of a module; `program`, `version_file`, `targets` of a program (`targets` only if no `image`); `dockerfile`, `module`, `name` of an image; `platform`, `file`, `build` of a target. |
| `expected a table / a string / an array of strings, found <type>` | Wrong type. |
| `must not be empty` | An empty string, or an empty `packages` list in a module. |
| `package <p> is also built by module <m>` | One package in two modules. |
| `overrides package <p>, which is not in this module's packages list` | Override without matching package. |
| `sets both the deprecated registry and registries - use registries only` | |
| `format is required off Android (...)` | Target with a non-`android` platform and no `format`. |

Not checked: `build_args` of an image, `docker.modules.<m>.<p>.build_args` keys other than that values are strings, `platform` value, and the existence of files.
