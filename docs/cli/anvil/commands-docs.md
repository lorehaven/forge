---
title: Documentation commands
parent: anvil
grand_parent: Command-line tools
nav_order: 4
---

`anvil docs` checks, builds and releases documentation **books**: directories of Markdown files that anvil turns into a static site (the navigation comes from the pages' front matter: `parent`, `nav_order`, `has_children`, as in the `just-the-docs` Jekyll theme the books were first written for). No Jekyll and no Ruby are involved. Books are declared under `[docs.books.<name>]`, see [Configuration](configuration.md#docs).

```text
anvil docs list
anvil docs check   [BOOK]
anvil docs build   [BOOK] [--remote-theme] [--no-check] [--out DIR] [--baseurl URL]
anvil docs release [BOOK] [--dry-run] [--remote-theme] [--no-check] [--baseurl URL]
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

Checks the Markdown and the OpenAPI documents. The page, navigation and link checks are those of the [docs-book](../../libs/docs-book/index.md) library, which loupe uses to browse a book; the OpenAPI checks are anvil's own. Every problem of every book is printed; the exit code is `1` if there is any.

```text
$ anvil docs check
ok   forge
ok   quench
```

What is checked:

| Check | Problem reported |
|---|---|
| Front matter | A `.md` file without front matter (it would be copied as it is, not rendered); front matter that is not valid YAML; a page without `title`. |
| Navigation | A `parent` that is no page's `title`; a `parent` whose page lacks `has_children: true`; a `parent` title shared by several pages with children and no `grand_parent` to tell them apart; two pages under one parent with the same `nav_order`. |
| Links | A relative `[text](target)` that names no file (a directory counts when it has an `index.md`). Fenced code blocks and code spans are skipped, and so are `http(s):`, `mailto:`, `#anchor` and any other scheme. A link starting with `/` is read from the book's directory. A link to a file under a `_` or `.` path (e.g. `_reports/`) is reported too: the file exists in the repository, but Jekyll leaves it out of the site, so the link is dead there. |
| OpenAPI | Only for a book with `[docs.books.<name>.openapi]`: for every `<dir>/<specs>/<service>/openapi.yaml`, the YAML must parse and declare `openapi: 3.1.x` with `info` and `paths`; every route in the service's source must be a documented path and method, and every documented path and method must have a route; every `$ref` must resolve inside the document. |

Files and directories whose names start with `_` or `.`, and what `exclude:` lists in the book's `_config.yml`, are not pages and are skipped (`_reports/`, `_style.md`, ...).

**Routes in the source** are the pairs found in the Rust files under the configured `sources` directories: an attribute `#[get("/path")]` (also `post`, `put`, `delete`, `patch`, `head`, `options`; spaces allowed) and a hand registration `method: Method::HEAD, pattern: "/path"`. `/x/:id` and `/x/{id:.+}` are compared as `/x/{id}`. Routes that a framework crate adds (`/health`, the swagger redirects) have no attribute in the service and are not expected in the document.

## anvil docs build

Checks the book (unless `--no-check`), writes the site, and packs the result.

| Option | Meaning |
|---|---|
| `--no-check` | Skip the checks. |
| `--out <dir>` | Where the builds go (default: `out` under `[docs]`, else `target/docs`). |
| `--baseurl <url>` | The `baseurl` the site is built for, for a site served below a path (`/docs/forge`). Default: the book's `baseurl`, else the one in `_config.yml`. |

The site is written by anvil itself ([`quench-html`](../../libs/docs-book/index.md)'s Markdown, CommonMark with GFM tables, task lists, strikethrough and footnotes): each page becomes an HTML page in a layout of a sidebar with the navigation, a header with search and the theme menu, breadcrumbs, and links to the previous and next page. Headings get anchors (`## Loupe --book` is `#loupe---book`), links between pages (`[x](other.md#part)`) point at the pages as written, tables scroll sideways in a wrapper, and the search is a client-side search over `assets/js/search-data.json` (one entry per page and per section). Every address is written from the site's root (`/x/`, with the `baseurl` in front when there is one), which Lectern rewrites for the path it serves a book under. Files that are not pages (images, downloads, a `.md` without front matter) are copied as they are; names starting with `_`, `.` or `#`, `Gemfile` and what `_config.yml` lists under `exclude` are left out.

Liquid is read as far as the books use it: `{% raw %}`/`{% endraw %}` are dropped and `{{ site.baseurl }}`, `{{ site.title }}` and `{{ page.title }}` replaced, unless `render_with_liquid: false` is set in the page's front matter or in a `defaults` scope of `_config.yml` that covers it. Not done: syntax highlighting (code blocks are `<pre><code class="language-x">`), kramdown attribute lists, collections, plugins.

What a build leaves under `<out>/<book>/`:

| File | Content |
|---|---|
| `site/` | The site; cleared at the start of every build. |
| `<book>-<version>.tar.gz` | Every file of `site/` (paths relative to it), then the book's Markdown sources under `_source/` (see below), as a gzip-compressed tar. The archive is **reproducible**: entries are sorted, owner is root, modification time is 0, so the same book gives the same bytes. |
| `manifest.json` | `book`, `title`, `version`, `built` (UTC, RFC 3339), `archive` (file name), `archive_bytes`, `archive_sha256`, `files` (the site's files), `site_bytes`, `sources` (`"_source/"`), `source_files`, and `baseurl` when there is one. |

The `_source/` part of the archive is the book as written, for readers that want the pages rather than the site ([loupe](../loupe/index.md)'s `--book`): every page docs-book reads (with its front matter, from the book's own directory, not the staged copy whose links were rewritten to `.html`), `_config.yml`, and every local image a page links to (`png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `avif`; relative to the page, or from the site root with the `baseurl` taken off), with their paths in the book. Files no page references, drafts and other `_` directories are left out. Lectern serves the site from the same archive and leaves `_source/` out: it is neither served nor counted.

The **forge theme** (`[docs].theme`, on unless it is `none`) is the look of the forge services: their design tokens, the light and dark modes and six colour palettes, applied from the cookies `qtheme` and `fui_palette` (the ones the forge services share, so a choice made in one place holds in all), the theme menu in the header and the brand mark in the sidebar. It is written into `assets/css/site.css` after the layout; `none` leaves the layout alone. Inside a frame (see [Lectern](../../services/lectern/index.md)) the book hides its own theme menu, since the bar around it has one. The tokens are a copy of `libs/forge-shell/assets/tokens.css`; a test fails when the two differ.

The **version** of a book is, in this order: `version` from the configuration; the version of its `version_file` (a `Cargo.toml`, `[workspace.package]` else `[package]`); today's date (`2026-10-30`).

```text
$ anvil docs build forge
ok   forge
wrote 213 page(s) and 228 file(s)
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
| `--no-check`, `--baseurl` | As for `build`. |

Refusals: `401`/`403` say to run `anvil login --scope warehouse:write` (and that the storage needs a write grant); `404` says the files storage does not exist (in the cluster it is a static storage, `docs=/docs` in `FILE_STORAGES` of the Warehouse deployment in homecloud; a dynamic storage can be created with the Warehouse's `POST /api/v1/files`); any other status prints the status and the body. `warehouse_url` comes from `[docs]`, else `[artifacts]`; `storage` is required.

```text
$ anvil docs release forge --dry-run
ok   forge
wrote 213 page(s) and 228 file(s)
built forge 2026-10-30: 224 files, 14814378 bytes -> /mnt/extdrive/projects/lorehaven/forge/target/docs/forge/forge-2026-10-30.tar.gz
would send docs/forge/2026-10-30/forge-2026-10-30.tar.gz (2124805 bytes)
would send docs/forge/2026-10-30/manifest.json (311 bytes)
would send docs/forge/latest.json (426 bytes)
```

*Unverified:* a release against a real Warehouse was not run; the upload path and the answers are covered by tests with a stand-in Warehouse.
