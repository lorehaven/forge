---
title: docs-book
parent: Libraries
has_children: true
permalink: /libs/docs-book/
---

`docs-book` (library `docs_book`) reads a documentation **book**, a directory of Markdown pages that Jekyll turns into a site with the just-the-docs theme, the way the estate's books are (`docs/` in forge and quench). It gives the pages and their front matter, the navigation tree just-the-docs builds from that front matter, where a link written on a page leads, and every problem a book can have. It touches no network and runs no Jekyll. It exists so that `anvil docs check` (before building a book) and loupe (browsing one in the terminal) share one definition of a book.

## Facts

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace) |
| License | MIT |
| Library name | `docs_book` |
| Test harness | `harness = false`; tests run through the `quench-test` macros (`tests/unit.rs`) |
| Unsafe | none (workspace lint) |
| Lints | the workspace's (`[lints] workspace = true`) |

## Install

Inside the workspace: `docs-book = { workspace = true }` (`path = "libs/docs-book"`, registry `ennor`, version `0.1.0` in `[workspace.dependencies]`). Users: `cli/anvil`, `cli/loupe`.

## Features

None.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-serde` (feature `yaml`) | normal | front matter and `_config.yml` |
| `quench-core` | normal | `Result`, `Context`, `bail`; `tmp::TempDir` in tests |
| `quench-test` | dev | the test harness |

## What it reads

* **Pages**: every `.md` file of the book's directory except what Jekyll leaves out: files and directories starting with `_` or `.`, and what `_config.yml` lists under `exclude`. A file without front matter is not a page (Jekyll copies it as it is) and is reported as a problem.
* **Front matter**: `title`, `parent`, `grand_parent`, `nav_order`, `has_children`, `nav_exclude`, `permalink`.
* **`_config.yml`**: `title`, `baseurl`, `exclude`.

## Example

```rust
use docs_book::{Book, Target};

let book = Book::open("docs".as_ref())?;
// The navigation in reading order, indented by depth.
for (page, depth) in book.nav().walk() {
    println!("{}{}", "  ".repeat(depth), book.pages()[page].label());
}
// Where a link on a page leads.
let metrics = book.page("services/gauge/metrics").unwrap();
if let Target::Page { page, anchor } = book.resolve(metrics, "configuration.md#targets") {
    println!("{} {anchor:?}", book.pages()[page].path); // services/gauge/configuration.md Some("targets")
}
// What `anvil docs check` reports (without the OpenAPI checks, which are anvil's).
for problem in book.problems() {
    eprintln!("{problem}");
}
```

| Page | What it covers |
|---|---|
| [API reference](api.md) | every public item |
