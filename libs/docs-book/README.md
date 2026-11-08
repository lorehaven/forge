# docs-book
A documentation book (a Jekyll site of Markdown pages with just-the-docs navigation): its pages, navigation tree, link resolution and checks.

## What it is
`docs-book` reads a book's directory the way Jekyll would (`_config.yml`'s `exclude`, files starting with `_` or `.` left out) into its pages and their front matter (`title`, `parent`, `grand_parent`, `nav_order`, `has_children`, `nav_exclude`, `permalink`), builds the navigation tree as just-the-docs does, and resolves a link written on a page (`metrics.md`, `metrics.html`, `gauge/`, `/services/gauge/metrics.html`, a permalink) to the page and heading it means. `anvil docs check` uses its checks; loupe uses it to browse a book in the terminal.

## Use cases
- Check a book before building it: front matter, navigation, broken links.
- Walk a book's navigation in reading order, with previous and next pages and breadcrumbs.
- Follow links between pages without building the site.

## Example
```rust
use docs_book::Book;

let book = Book::open("docs".as_ref())?;
for (page, depth) in book.nav().walk() {
    println!("{}{}", "  ".repeat(depth), book.pages()[page].label());
}
for problem in book.problems() {
    eprintln!("{problem}");
}
```
