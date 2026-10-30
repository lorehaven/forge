---
title: Documentation commands
parent: anvil
grand_parent: Command-line tools
nav_order: 4
---

`anvil docs` checks, builds and releases documentation **books**: directories of Markdown files that Jekyll turns into a site (the `just-the-docs` theme takes its navigation from the pages' front matter). Books are declared under `[docs.books.<name>]`, see [Configuration](configuration.md#docs).

```text
anvil docs list
anvil docs check   [BOOK]
anvil docs build   [BOOK] [--offline] [--no-check] [--out DIR] [--baseurl URL]
anvil docs release [BOOK] [--dry-run] [--offline] [--no-check] [--baseurl URL]
```

`BOOK` is a name from `[docs.books]`; left out, the command runs for every book, in name order. These commands do not run through the output pump, so `--silent` and `--tail` have no effect.

## anvil docs list

One line per book: its name, its directory (from the workspace root) and its title.

```text
$ anvil docs list
forge  docs  Forge
quench  ../quench/docs  Quench
```

## anvil docs check

Checks the Markdown and the OpenAPI documents without running Jekyll, so it needs no Ruby. Every problem of every book is printed; the exit code is `1` if there is any.

```text
$ anvil docs check
ok   forge
ok   quench
```

What is checked:

| Check | Problem reported |
|---|---|
| Front matter | A `.md` file without front matter (Jekyll would copy it as it is, not render it); front matter that is not valid YAML; a page without `title`. |
| Navigation | A `parent` that is no page's `title`; a `parent` whose page lacks `has_children: true`; a `parent` title shared by several pages with children and no `grand_parent` to tell them apart; two pages under one parent with the same `nav_order`. |
| Links | A relative `[text](target)` that names no file (a directory counts when it has an `index.md`). Fenced code blocks and code spans are skipped, and so are `http(s):`, `mailto:`, `#anchor` and any other scheme. A link starting with `/` is read from the book's directory. |
| OpenAPI | Only for a book with `[docs.books.<name>.openapi]`: for every `<dir>/<specs>/<service>/openapi.yaml`, the YAML must parse and declare `openapi: 3.1.x` with `info` and `paths`; every route in the service's source must be a documented path and method, and every documented path and method must have a route; every `$ref` must resolve inside the document. |

Files and directories whose names start with `_` or `.`, and what `exclude:` lists in the book's `_config.yml`, are not pages and are skipped (`_reports/`, `_style.md`, ...).

**Routes in the source** are the pairs found in the Rust files under the configured `sources` directories: an attribute `#[get("/path")]` (also `post`, `put`, `delete`, `patch`, `head`, `options`; spaces allowed) and a hand registration `method: Method::HEAD, pattern: "/path"`. `/x/:id` and `/x/{id:.+}` are compared as `/x/{id}`. Routes that a framework crate adds (`/health`, the swagger redirects) have no attribute in the service and are not expected in the document.

## anvil docs build

Checks the book (unless `--no-check`), runs `jekyll build`, and packs the result.

| Option | Meaning |
|---|---|
| `--offline` | Build with the gems that are installed: `remote_theme` is switched off, `just-the-docs` is used as a local theme and only `jekyll-seo-tag` is loaded, so nothing is fetched. Without it the book's own `_config.yml` decides (the book's `remote_theme` needs the network at build time). |
| `--no-check` | Skip the checks. |
| `--out <dir>` | Where the builds go (default: `out` under `[docs]`, else `target/docs`). |
| `--baseurl <url>` | The `baseurl` the site is built for, for a site served below a path (`/docs/forge`). Default: the book's `baseurl`, else the one in `_config.yml`. |

Jekyll is started as `jekyll build --source <dir> --disable-disk-cache --destination <out>/<book>/site [--config <dir>/_config.yml[,<offline override>]] [--baseurl <url>]` with `JEKYLL_ENV=production`. The command is `jekyll` unless `[docs].jekyll` says otherwise (`bundle exec jekyll`). If it cannot be run, anvil says how to install it. A failed Jekyll run prints the last 25 lines of its error output.

What a build leaves under `<out>/<book>/`:

| File | Content |
|---|---|
| `site/` | The Jekyll output; cleared at the start of every build. |
| `<book>-<version>.tar.gz` | Every file of `site/` (paths relative to it), as a gzip-compressed tar. The archive is **reproducible**: entries are sorted, owner is root, modification time is 0, so the same site gives the same bytes. |
| `manifest.json` | `book`, `title`, `version`, `built` (UTC, RFC 3339), `archive` (file name), `archive_bytes`, `archive_sha256`, `files`, `site_bytes`, and `baseurl` when there is one. |
| `jekyll-offline.yml` | With `--offline`: the override configuration. |

The **version** of a book is, in this order: `version` from the configuration; the version of its `version_file` (a `Cargo.toml`, `[workspace.package]` else `[package]`); today's date (`2026-10-30`).

```text
$ anvil docs build forge --offline
ok   forge
running: jekyll build /mnt/extdrive/projects/lorehaven/forge/docs
built forge 2026-10-30: 223 files, 14727801 bytes -> target/docs/forge/forge-2026-10-30.tar.gz
```

## anvil docs release

Builds the books exactly like `build`, then sends them to a warehouse **files storage** (see the Warehouse's [file protocol](../../services/warehouse/protocols.md)). For each book three files are written with `PUT {warehouse_url}/api/v1/files/{storage}/file?path=...` and a bearer token (the same stored login as `artifact release`; see [Credentials](commands-credentials.md)):

| Path in the storage | Content | Content type |
|---|---|---|
| `<book>/<version>/<book>-<version>.tar.gz` | The site archive. | `application/gzip` |
| `<book>/<version>/manifest.json` | The manifest of the build. | `application/json` |
| `<book>/latest.json` | The manifest plus `archive_path` and `manifest_path` (where the two files above are in the storage): the one file a preview service reads to find the newest book. | `application/json` |

An upload to an existing path replaces it, so releasing a version again replaces that version and moves `latest.json` to it.

| Option | Meaning |
|---|---|
| `--dry-run` | Build and check everything, then print `would send <storage>/<path> (<bytes> bytes)` for each file; asks for no token and sends nothing. |
| `--offline`, `--no-check`, `--baseurl` | As for `build`. |

Refusals: `401`/`403` say to run `anvil login --scope warehouse:write` (and that the storage needs a write grant); `404` says the files storage does not exist (in the cluster it is a static storage, `docs=/docs` in `FILE_STORAGES` of the Warehouse deployment in homecloud; a dynamic storage can be created with the Warehouse's `POST /api/v1/files`); any other status prints the status and the body. `warehouse_url` comes from `[docs]`, else `[artifacts]`; `storage` is required.

```text
$ anvil docs release forge --offline --dry-run
ok   forge
running: jekyll build /mnt/extdrive/projects/lorehaven/forge/docs
built forge 2026-10-30: 224 files, 14814378 bytes -> /mnt/extdrive/projects/lorehaven/forge/target/docs/forge/forge-2026-10-30.tar.gz
would send docs/forge/2026-10-30/forge-2026-10-30.tar.gz (2124805 bytes)
would send docs/forge/2026-10-30/manifest.json (311 bytes)
would send docs/forge/latest.json (426 bytes)
```

*Unverified:* a release against a real Warehouse was not run; the upload path and the answers are covered by tests with a stand-in Warehouse.
