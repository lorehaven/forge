---
title: loupe
parent: Command-line tools
nav_order: 9
has_children: true
permalink: /cli/loupe/
---

loupe shows Markdown in the terminal: a single file, stdin, or a documentation book (a Jekyll site with just-the-docs navigation, such as forge's own `docs/`). On a terminal it opens a pager with `less`-like keys, links to follow, search, a table of contents, footnotes and the book's navigation. Piped, or with `--plain` or `NO_COLOR`, it prints the page as plain text and exits. Formulas are shown as Unicode text, or typeset as images where the terminal speaks the kitty graphics protocol or sixel.

Version: `loupe 0.1.0` (`forge/cli/loupe/Cargo.toml`). Binary and library crate: `loupe`.

It is built on quench and forge crates only: `quench-markdown-term` (layout), `quench-html` (the Markdown parser, with tables, footnotes and math), `quench-tex` (formulas as text, and the typesetter with the bundled Noto Sans Math), `quench-cli` (raw mode, keys, terminal graphics), and `docs-book` (the book model it shares with `anvil docs`). No TeX installation, renderer or font is needed on the machine.

## Install

```bash
cd forge/cli/loupe
cargo install --path .
```

The pager needs a Unix terminal and `stty` on `PATH` (quench-cli switches raw mode with it). Following a link outside the book runs `xdg-open` after asking.

## Synopsis

```text
loupe [OPTIONS] FILE            # render; the pager on a terminal, plain text when piped
loupe [OPTIONS] -               # stdin
loupe [OPTIONS] DIR [ROUTE]     # a book, at its index or at ROUTE (path without .md)
loupe --book BOOK[@VERSION] [ROUTE]   # a book released to Warehouse (latest, or that version)
loupe --book                    # the released books
loupe login                     # store an application token for reading released books
loupe --check DIR               # the book's checks; exit 1 on problems
loupe doctor                    # math font, terminal graphics, cell size and colours
```

See [Commands](commands.md) for every flag, and [Pager](pager.md) for the keys.

## The config file

Defaults for the flags live in `$XDG_CONFIG_HOME/loupe/config.toml` (when `XDG_CONFIG_HOME` is set to an absolute path), else in `~/.config/loupe/config.toml`. Every key is optional, and a flag on the command line wins over the file:

```toml
math_font = "~/fonts/SomeMath.ttf"   # instead of the bundled Noto Sans Math; ~/ and paths relative to this file work
footnotes = "inline"                 # end | inline
math = "text"                        # source | text
images = "off"                       # on | off
width = 100                          # columns
warehouse_url = "https://host/warehouse"   # released books (see Released books)
storage = "docs"
gatehouse_url = "https://host/gatehouse"   # for loupe login
```

A file that is not TOML, has an unknown key or a value of the wrong kind is reported once on stderr (`loupe: …/config.toml: unknown setting `colour` (known: math_font, footnotes, math, images, width, warehouse_url, storage, gatehouse_url); the config file is ignored`) and left out.

## Released books

`anvil docs release` sends a book to a Warehouse files storage (`<book>/latest.json`, `<book>/<version>/manifest.json`, `<book>/<version>/<book>-<version>.tar.gz`); since this version of anvil, the archive holds the book's Markdown under `_source/` beside the built site. `loupe --book` reads it back:

```text
$ loupe --book                                   # the books in the storage, with their latest versions
forge   2026.11.08  Forge
quench  2026.11.01  Quench
$ loupe --book forge services/gauge/metrics      # the latest release, at a page
loupe: fetched forge 2026.11.08 into /home/me/.cache/loupe/forge/2026.11.08
$ loupe --book forge@2026.11.08                  # a given version (from the cache: no request at all)
```

1. `latest.json` (or, for `BOOK@VERSION`, that version's `manifest.json`) says the version and where its archive is.
2. A version already in the cache, `$XDG_CACHE_HOME/loupe/<book>/<version>/` (else `~/.cache/loupe/...`), is opened as it is. Otherwise the archive is downloaded, checked against the manifest's `archive_sha256`, and only its `_source/` is unpacked into the cache (into a scratch directory first, renamed into place, so a version is there whole or not at all).
3. The cached directory is then an ordinary book: the same pager, navigation, link resolution, `--plain` output and images (from the cache).

**Where Warehouse is**, the first that is set:

| Setting | From |
|---|---|
| Warehouse URL (with its base path) | `LOUPE_WAREHOUSE_URL`; `warehouse_url` in loupe's [config file](#the-config-file); `warehouse_url` under `[docs]` (or `[artifacts]`) of the nearest `.anvil.toml` / `.anvil/*.toml` above the working directory, so it works in a forge checkout as it is |
| Files storage | `LOUPE_STORAGE`; `storage` in the config file; `storage` under `[docs]` of the anvil configuration; else `docs` |

**Credentials**, as the other forge tools keep them (gatehouse-auth's application tokens, exchanged with Gatehouse for a short-lived bearer token on every run), the first found:

1. `LOUPE_WAREHOUSE_TOKEN`, a bearer token as it is;
2. loupe's stored token: `.loupe/secret.toml` or `.loupe.secret.toml` in or above the working directory, then `.loupe.secret.toml` in loupe's config directory (`~/.config/loupe/`), where `loupe login` puts it;
3. anvil's: `ANVIL_APP_TOKEN` with `ANVIL_GATEHOUSE_URL`, or the token `anvil login` stored in `.anvil/secret.toml` / `.anvil.secret.toml` above the working directory.

Without any, the requests go without a token, which a storage that allows reading without one answers. Lectern serves only the built site (`/{book}/…`) and a list of books, never the archive or the Markdown, so it is no fallback for the sources.

`loupe login [--gatehouse-url URL] [--username USER] [--name NAME] [--scope GRANT]... [--days N]` signs in to Gatehouse (the password is read without echo and never stored) and stores an application token, with the grant `warehouse:read` unless `--scope` says otherwise. The Gatehouse URL is the flag, else `LOUPE_GATEHOUSE_URL`, else `gatehouse_url` in the config file, else `gatehouse_url` under `[release]` of the anvil configuration, else the one stored by an earlier login.

**Errors**, each saying what to do:

| When | Message |
|---|---|
| no Warehouse URL anywhere | `no Warehouse configured: set LOUPE_WAREHOUSE_URL (…), or warehouse_url in ~/.config/loupe/config.toml, or run loupe inside a checkout whose .anvil.toml has warehouse_url under [docs]` |
| refused without credentials | ``the `docs` storage at … needs credentials (401): run `loupe login` (or `anvil login` in a checkout), or set LOUPE_WAREHOUSE_TOKEN`` |
| refused with credentials | ``Warehouse refused the credentials for the `docs` storage (403): run `loupe login` again; reading needs the `warehouse:read` grant`` |
| the stored token cannot be exchanged | ``cannot get a Warehouse token: Gatehouse rejected the application token …; run `loupe login` again`` |
| an unknown book | ``no released book `quench` in the `docs` storage (`loupe --book` lists them)`` |
| an unknown version | ``the book `forge` has no version `0.9` (the latest is 1.0)`` |
| a release without `_source/` | `the release forge 1.0 has no Markdown sources (_source/): it was released before loupe support; release the book again with a newer anvil` |
| a damaged download | `the archive of forge 1.0 does not match its checksum: download it again` |

`loupe --book` alone lists the books: the storage's top-level directories (or, for a storage that lists files, the books with a `latest.json`) that have a `latest.json`, with its version and title. Only the first page of a long listing is read.

## The math font

Formulas are typeset with **Noto Sans Math**, embedded in quench-text, so a default build needs no font on the machine. Another font can be chosen with `--math-font PATH` or `math_font` in the config file (a path; there is no lookup by family name). loupe validates it before using it:

1. it must be readable and an sfnt that quench-text parses; a CFF font (`OTTO`) is refused by name: `… is a CFF font: CFF outlines are not read yet`;
2. it must have a `MATH` table with its three subtables (constants, glyph info, variants): `… has no MATH table: it is not a math font`, `… has a MATH table without its glyph info`;
3. it must have glyphs for `+ − = ( ) ∑ ∫ √`.

When it fails, loupe never stops and never shows a half-typeset formula: formulas are shown as text, and one warning says why, on stderr for plain output (only for a page that has formulas), on the status line in the pager:

```text
loupe: no math font: /usr/share/fonts/gsfonts/D050000L.otf is a CFF font: CFF outlines are not read yet; formulas are shown as text
```

`--math text` and `--math source` silence it (text was asked for). Images of the page are still drawn. [`loupe doctor`](commands.md#loupe-doctor) names the font in use and where it came from.

## Exit codes

| Code | When |
|---|---|
| `0` | The page was shown or printed; `--check` found no problems; `doctor` found that formulas are typeset as images. |
| `1` | A file or book cannot be read, a ROUTE matches no page, `--check` found problems, `doctor` found that formulas fall back to text, or the terminal failed. |
| `2` | A usage error: bad arguments, nothing to show, `--width 0`, a ROUTE without a book, a ROUTE given twice with `--book`. |
| `130` | The pager got `SIGINT` from outside (the terminal is restored first). |
| `143` | The pager got `SIGTERM` (the terminal is restored first). |

## Where to go next

[Commands](commands.md) | [Pager](pager.md)
