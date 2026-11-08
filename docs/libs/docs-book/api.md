---
title: API reference
parent: docs-book
grand_parent: Libraries
nav_order: 1
---

Every public item, by source file (`src/lib.rs`, `page.rs`, `nav.rs`, `links.rs`).

## `Book` (`lib.rs`)

A book read from its directory.

| Item | What it does |
|---|---|
| `Book::open(dir: &Path) -> Result<Book>` | Reads the pages, `_config.yml` and the navigation. Fails when `dir` is not a directory (`the book's directory … does not exist`), or a page or `_config.yml` cannot be read, or `_config.yml` is not YAML. Pages without front matter or with bad YAML are not errors: they are in `problems()`. |
| `dir() -> &Path`, `title() -> Option<&str>`, `baseurl() -> &str` | The directory; `title` and `baseurl` of `_config.yml` (`baseurl` empty when absent). |
| `pages() -> &[Page]` | The pages in path order. Everything else refers to pages by their index in this list. |
| `nav() -> &Nav` | The navigation tree. |
| `page(route: &str) -> Option<usize>` | The page at a route: `services/gauge/metrics` (path without `.md`), `services/gauge` for its `index.md`, `""` or `/` for the book's index; a path with `.md`/`.html` or a `permalink` is accepted too. |
| `resolve(from: usize, target: &str) -> Target` | Where a link on page `from` leads (see `resolve` below), with the book's `baseurl`. |
| `problems() -> Vec<String>` | Every problem: pages that could not be read, `nav_problems`, and `link_problems` of every page. |

## Pages (`page.rs`)

| Item | What it does |
|---|---|
| `Page { path, title, parent, grand_parent, nav_order: Option<i64>, has_children, nav_exclude, permalink, body }` | One page; `path` from the book's directory with `/`; `body` is the Markdown after the front matter. |
| `Page::route() -> &str` | The path without `.md`; a directory for an `index.md`; `""` for the book's `index.md`. |
| `Page::label() -> &str` | The title, or the file name without `.md`. |
| `split_front_matter(text) -> Option<(&str, &str)>` | The YAML between the leading `---` lines, and the rest; `None` when the text does not start with `---`. |
| `strip_front_matter(text) -> &str` | The text without its front matter, or the text itself. |
| `parse_page(path, text) -> Result<Page, String>` | A page from its text; the error is the problem message (no front matter, YAML that does not parse). |
| `read_pages(dir) -> Result<(Vec<Page>, Vec<String>)>` | Every page of the directory, and the problems met. |
| `config(dir) -> Result<Option<Value>>` | `_config.yml` as a `quench_serde::Value`, if there is one. |

## Navigation (`nav.rs`)

just-the-docs' rules: a page names its parent by title; when several pages with `has_children: true` share that title, `grand_parent` (the parent's own parent) picks one. Siblings are ordered by `nav_order`, then the pages without one by title (case aside), then by path. A page without a title, with `nav_exclude: true`, or whose parent cannot be found is not in the navigation.

| Item | What it does |
|---|---|
| `Nav::of(pages) -> Nav` | The tree. |
| `roots() -> &[usize]`, `children(i) -> &[usize]`, `parent(i) -> Option<usize>` | The tree, in order. |
| `trail(i) -> Vec<usize>` | The page and its ancestors, top level first: a breadcrumb. |
| `walk() -> Vec<(usize, usize)>` | Every page in reading order (depth first) with its depth. |
| `neighbours(i) -> (Option<usize>, Option<usize>)` | The pages before and after in reading order. |
| `nav_problems(pages) -> Vec<String>` | Pages without a title; a `parent` that is no page's title, or whose page lacks `has_children`; an ambiguous parent without a telling `grand_parent`; siblings sharing a `nav_order`. |

## Links (`links.rs`)

| Item | What it does |
|---|---|
| `links(body) -> Vec<String>` | The targets of `[text](target)` outside fenced blocks and code spans. |
| `is_outside(target) -> bool` | Not a path in the book: empty, an anchor, a URL with a scheme (`https:`, `mailto:`…). |
| `link_problems(dir, page) -> Vec<String>` | Links that name no file (a directory counts with an `index.md`), and links into a `_` or `.` path, which Jekyll leaves out of the site. |
| `resolve(pages, from, target, baseurl) -> Target` | Where a link leads, as the built site serves it. |
| `Target::{Page { page, anchor }, Anchor(String), Outside(String), Missing(String)}` | A page and heading; a heading of the same page; out of the book (a URL, or a file that is not a page such as `diagram.png`); a path in the book with no page. |

`resolve` tries, in order: a page whose `permalink` matches (slashes aside); for `dir/` the directory's `index.md`; for `x.md` that page; for `x.html` the page `x.md`, else `x/index.md`; for `x` without an extension `x.md`, else `x/index.md`. Paths are relative to the linking page, or to the book's root when they start with `/` (after the `baseurl`, when the site has one). Extensions are compared case aside; page paths are not (`Configuration.MD` finds no `configuration.md`, as on a case-sensitive server).
