---
title: Pager
parent: loupe
grand_parent: Command-line tools
nav_order: 2
---

On a terminal, loupe shows the page on the alternate screen, with a status line at the bottom: the file name (or the book's breadcrumb), the lines shown out of the total and how far through, then a message, what the focused image is, or a warning. The terminal's size is followed (`SIGWINCH`): a new width lays the page out again and keeps the reader's place (the heading at the top of the screen, and how far below it).

Keys and graphics detection go through `/dev/tty`, whatever stdin is.

## Keys

| Keys | Does |
|---|---|
| `j` `k`, `↓` `↑` | one line down, up |
| `space` `b`, `PgDn` `PgUp` | a screen down, up |
| `d` `u` | half a screen down, up |
| `g` `G`, `Home` `End` | top, bottom |
| `/` *pattern* `Enter`, `n` `N` | search forward, next, previous; matches are shown in reverse video, the current one in bold. Case is ignored unless the pattern has an upper-case letter. Searches wrap with a message. |
| `Tab` `Shift-Tab` | the next / previous link (the focused one is underlined in reverse video) |
| `Enter` | follow the focused link (without one: a line down) |
| `Backspace`, `h`, `←` | back to where the last jump or link came from |
| `t` | the page's table of contents; `j` `k` move, `Enter` jumps (the heading becomes the top line, even near the end of the page), `Esc` closes |
| `f` | the page's footnotes; `Enter` jumps to one, `Backspace` returns |
| `m` | switch the formula under the cursor (on the focused link's line, else the first on screen) between text and its TeX source |
| `y` | copy the code block under the cursor (or the first one on screen) to the clipboard, with OSC 52; blocks inside quotes, lists and callouts are found too, and long lines that wrap with `↪` are copied whole |
| `o` | in a book: the navigation tree, indented by depth; typing filters by title or route, `↑` `↓` move, `Enter` opens, `Esc` closes |
| `[` `]` | in a book: the previous / next page in navigation order |
| `Esc` | drop the focus and the search highlights |
| `Ctrl-L` | redraw |
| `Ctrl-Z` | suspend (the terminal is restored; `fg` comes back) |
| `q`, `Ctrl-C` | quit |

## Following links

| Link | Goes to |
|---|---|
| `#anchor` | the heading with that slug on the page (GitHub-style slugs, or `{#id}`); it becomes the top line, and a heading near the end of the page is shown with empty rows below the page |
| a footnote mark | its definition; the definition's `↩` goes back to the reference |
| another page of the book (`../gauge/metrics.md`, the `.html` the site has, a permalink) | that page, at its anchor if the link has one |
| another Markdown file (outside a book) | that file |
| a URL, or a file that is not a page | asks `open … ? (y/n)`, then `xdg-open` |
| a page that does not exist | a message on the status line |

Every jump and opened page is remembered, so `Backspace` walks back through them.

## Signals

| Signal | Does |
|---|---|
| `SIGINT`, `SIGTERM` from outside | restore the terminal (raw mode off, the alternate screen left, the cursor shown, kitty images deleted) and exit `130` / `143` |
| `SIGTSTP` (`kill -TSTP`), or Ctrl-Z | restore the terminal and stop; `SIGCONT` (`fg`) sets the terminal up again and redraws |
| `SIGWINCH` | lay the page out for the new size |

Ctrl-C is a key in raw mode: it quits like `q` (exit 0). The signals are received by a small tokio runtime on a thread of its own, and reach the pager as events, as keys do. A panic restores the screen and runs `stty sane` before its message.

## Formulas and images as pictures

When the terminal shows images (checked once at start, through `/dev/tty`, as [`loupe doctor`](commands.md#loupe-doctor) does) and `--images off` is not given:

* **Formulas** (unless `--math text` or `--math source`, or a math font that cannot be used) are typeset with quench-tex in the math font, sized to the terminal's cells and drawn in its foreground colour over the text layout:
  * an inline formula sits on the text line: its em is 0.9 of the cell height, its baseline at 0.78 of the cell, scaled down to fit the cells the text form takes (and left as text when that would be under 0.4 of its size);
  * a display formula is centred in the rows its text form takes, across the page.
* **Images of the page** (`![alt](path)`): a local file, relative to the page's file, or for a `/` path to the book's root (with the site's `baseurl` taken off), is decoded with quench-image (PNG, JPEG baseline and progressive, GIF's first frame, WebP lossy and lossless). It is scaled down (never up) to the page's width and at most 20 rows (fewer on a short terminal), and drawn in rows the layout keeps for it under its `[image: alt]` text. A remote URL is not fetched, and an image that cannot be read stays `[image: alt]`; focusing it (`Tab`) says why on the status line (`image not shown: https://…: remote images are not fetched`, `… SVG images are not supported`, `cannot read …`), and a drawn one says its file and size.
* **kitty**: PNG, sent once per page layout under an id (formula `i` is `0x4c00 + i`, image `i` is `0x4c00 + 0x100000 + i`), placed with a source rectangle (`x,y,w,h`) when it moves or is cut by the top or bottom of the screen, hidden when it leaves the screen, and freed when the page is laid out again or loupe exits;
* **sixel**: RGBA blended onto the terminal's background, its rows cropped to the visible ones before encoding, and drawn again after each redraw of the text.

The text under a formula's picture is blanked. A formula that cannot be typeset, or one switched to its source with `m`, stays text. Without graphics everything is text, as in plain output.

## Limitations

* Only Unix terminals.
* A formula wrapped across lines is placed where it starts.
* Images are not fetched over the network, and SVG is not drawn.
* The image ids and pictures are not checked against a real kitty or sixel terminal by the tests: they assert on the escape sequences.
