---
title: Commands
parent: loupe
grand_parent: Command-line tools
nav_order: 1
---

Verified by running the binary on forge's `docs/` and on scratch files and books, piped (plain output), and in a pseudo-terminal under `script` (the pager). Typeset images were not seen on a real kitty or sixel terminal: their escape sequences are checked by the tests (see [Pager](pager.md#formulas-and-images-as-pictures)).

## `loupe FILE` and `loupe -`

```text
Usage: loupe [OPTIONS] [PATH] [ROUTE] [COMMAND]
```

The file's front matter (a leading `---` block) is stripped, `{{ site.baseurl }}` is replaced, and the Markdown is parsed with every extension (GFM tables, strikethrough, task lists, footnotes, math) and laid out at the terminal's width, or `--width`.

* **Plain output**: when standard output is not a terminal, or with `--plain`, or when `NO_COLOR` is set to a non-empty value, the page is printed as plain text (no escape sequences) and loupe exits. Links are numbered ` [n]` and their destinations listed at the end; images are `[image: alt]`.
* **The pager** otherwise: see [Pager](pager.md). `-` reads the whole of stdin first; the pager then reads keys, and asks what the terminal can draw, through `/dev/tty`, so `cat notes.md | loupe -` pages on a terminal with typeset formulas and images where it shows them.

```text
$ printf -- '---\ntitle: x\n---\n# Hi\n\nEuler $e^{i\\pi}+1=0$.\n\n$$\n\\frac{a}{b}\n$$\n' | loupe - --width 30
Hi
══════════════════════════════

Euler e^(iπ) + 1 = 0.

              a
              ─
              b
```

## Flags

| Flag | Default | Does |
|---|---|---|
| `--plain` | off | Plain text, no pager, no colour. |
| `--width N` | the config file's `width`, else the terminal's columns, 80 when piped | Lay the page out at `N` columns (at least 1; layouts narrower than 4 are made at 4). In the pager the width then stays fixed when the terminal is resized. |
| `--footnotes end\|inline` | `end` | `end`: definitions under a rule at the end of the page, each with a `↩`. `inline`: each definition, dim and indented, right after the block that first references it. |
| `--math source\|text` | (neither) | `source`: every formula as its TeX source (`$tex$`, or a frame labelled `tex`). `text`: formulas as Unicode text, and never typeset as images. Without the flag, formulas are text, and typeset as images where the terminal shows them. |
| `--images on\|off` | `on` | `off`: no typeset formulas and no images of the page, whatever the terminal can do. |
| `--math-font PATH` | the bundled Noto Sans Math | A TrueType font with a `MATH` table to typeset formulas with; validated first (see [The math font](index.md#the-math-font)). |
| `--check DIR` | | Run the book checks and exit (below). |
| `--book [BOOK[@VERSION]]` | | A book released to Warehouse; the page is then the one argument (`loupe --book forge services/gauge/metrics`). Alone, lists the released books. See [`loupe --book`](#loupe---book) below. |
| `-h`, `--help` / `-V`, `--version` | | Help; `loupe 0.1.0`. |

Each flag except `--plain`, `--check` and `--book` can have its default in the [config file](index.md#the-config-file); the flag wins.

`doctor` and `help` are subcommands. When a file or directory of that name is in the current directory too, the subcommand still runs, with a hint on stderr:

```text
$ loupe doctor
loupe: running `loupe doctor`; to open the file named doctor, use `loupe ./doctor` or `loupe -- doctor`
```

`loupe -- doctor` and `loupe ./doctor` open the file.

## `loupe DIR [ROUTE]`

A directory is opened as a book with `docs-book` (the model `anvil docs` uses): its pages and front matter, and the navigation tree from `title`, `parent`, `grand_parent` and `nav_order`. ROUTE is a page's path without `.md` (`services/gauge/metrics`, `services/gauge` for its `index.md`); a path with `.md` or `.html`, or a permalink, works too. Without ROUTE the book opens at its `index.md`, or at the first page of its navigation.

```text
$ loupe docs/ nowhere
loupe: the book has no page `nowhere`
```

Plain output prints the one page. In the pager, the status line shows the breadcrumb (`Forge › Command-line tools › pulley`), `o` opens the navigation, `[` and `]` go to the previous and next page, and links between pages open them. When the book has problems the status line says so (`the book has 3 problems (loupe --check docs/)`).

## `loupe --book`

```text
loupe [OPTIONS] --book BOOK[@VERSION] [ROUTE]
loupe --book
```

Opens a book that `anvil docs release` published: its latest release, or the one given after `@`, at ROUTE or at its index. The release's Markdown (`_source/` in its archive) is fetched into `~/.cache/loupe/<book>/<version>/` once, and opened from there like a book's directory; a version given and already cached needs no request at all. When it fetched, loupe says so on stderr (`loupe: fetched forge 2026.11.08 into …`). Every flag works as for a directory (`--plain`, `--width`, ...). Alone, `--book` lists the books of the storage with their latest version and title.

Where Warehouse is, the credentials, and every error message: see [Released books](index.md#released-books).

## `loupe login`

```text
loupe login [--gatehouse-url URL] [--username USER] [--name NAME] [--scope GRANT]... [--days N]
```

Signs in to Gatehouse and stores an application token in `~/.config/loupe/.loupe.secret.toml` (readable by its owner only), as `anvil login` and `riveter login` do for their tools. The username is asked for when not given, the password always (without echo; it is never stored). The token asks for `warehouse:read` unless `--scope` names other grants, and is named `loupe on <host>` unless `--name` says otherwise. Not run for the book: it needs a Gatehouse.

## `loupe --check DIR`

Runs the book's checks: pages without front matter or whose front matter is not YAML, navigation problems (a page without a title, a `parent` that is not a page with children or is ambiguous, siblings sharing a `nav_order`), and links to nothing. Each problem is a line; the last line counts them. Exit 1 when there are problems.

```text
$ loupe --check docs
docs: ok (210 pages, no problems)
```

```text
$ loupe --check broken
untitled.md: no `title` in the front matter
index.md: broken link `missing.md`
broken: 2 problems
```

## `loupe doctor`

Reports, without a page to render, what works in this terminal. It asks the terminal through `/dev/tty`, so it works with stdin piped:

```text
$ loupe doctor
loupe doctor
  math font   Noto Sans Math, bundled (quench-text): typesets (a sample formula drew 213×56 px)
  graphics    none: the terminal answered neither the kitty graphics query nor with sixel in its device attributes
  cell size   not measured (assumed 10×20 px when drawing)
  colours     foreground not measured, background not measured
  formulas    shown as text: the terminal shows no images
```

```text
$ loupe --math-font /usr/share/fonts/gsfonts/D050000L.otf doctor
loupe doctor
  math font   /usr/share/fonts/gsfonts/D050000L.otf (--math-font): not used: /usr/share/fonts/gsfonts/D050000L.otf is a CFF font: CFF outlines are not read yet
...
  formulas    shown as text: the math font cannot be used
```

* **math font**: the font in use and where it came from: `Noto Sans Math, bundled (quench-text)`, or a path with `(--math-font)` or `(the config file's math_font)`; then whether a sample formula typesets with it, or why the font is not used.
* **graphics**: `kitty` (the terminal answered the kitty graphics query), `sixel` (its device attributes include 4), `forced by QUENCH_GRAPHICS=…`, or `none` and why: not a terminal, `TERM` unset or `dumb`, inside tmux or screen (nothing is asked there; `QUENCH_GRAPHICS=kitty` or `sixel` forces a protocol), `QUENCH_GRAPHICS=none`, or no answer.
* **cell size** and **colours**: as measured (`CSI 16 t` / `CSI 14 t`, `OSC 10` / `OSC 11`).
* Exit `0` when formulas are typeset as images, `1` when they fall back to text, so it can be a check in a host recipe.
