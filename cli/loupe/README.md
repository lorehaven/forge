# loupe
Markdown files and documentation books in the terminal.

## What it is
`loupe` renders a Markdown file to the terminal's width and pages through it: links you can follow, search, a table of contents, footnotes, formulas as Unicode text or typeset as images, and the images of the page where the terminal shows them (kitty, sixel). A directory is read as a documentation book (a Jekyll site with just-the-docs navigation, as forge's `docs/` is): its navigation tree, links between pages, and the same checks as `anvil docs check`. Piped, it prints plain text.

## Use cases
- Read a README or a book page without a browser (`loupe README.md`).
- Browse a book from its checkout (`loupe docs/`, `loupe docs/ services/gauge/metrics`).
- Check a book's front matter, navigation and links without Ruby or Jekyll (`loupe --check docs/`).
- Read a book released to Warehouse without a checkout (`loupe --book forge`, `loupe --book` to list them; `loupe login` for a token).
- See whether formulas can be typeset in this terminal, and with which math font (`loupe doctor`).
- Set defaults (width, footnotes, math, images, a math font) in `~/.config/loupe/config.toml`.

## Example
```sh
loupe README.md
cat notes.md | loupe -
loupe --plain --width 100 README.md
loupe docs/ cli/pulley
loupe --book forge services/gauge/metrics
loupe --check docs/
loupe doctor
```

## Documentation
Full documentation: [cli/loupe](../../docs/cli/loupe/index.md)
