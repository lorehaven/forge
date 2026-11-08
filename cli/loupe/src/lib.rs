//! loupe: Markdown files and documentation books in the terminal.
//!
//! * [`app`]: what a command line does, with the environment and streams passed in (plain output, `--check`).
//! * [`render`]: a page's Markdown laid out with quench-markdown-term, formulas as text through quench-tex.
//! * [`pager`]: the pager as a state machine over a screen model, without terminal I/O.
//! * [`graphics`]: formulas typeset with quench-tex and drawn with kitty or sixel over the text layout.
//! * [`doctor`]: `loupe doctor`'s report; [`config`]: the config file; [`fonts`]: the math font, validated.
//! * [`released`]: books released to Warehouse, fetched into a cache.
//! * [`view`]: a screen as escape sequences; `terminal` (Unix): the pager on the real terminal.

pub mod app;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod fonts;
pub mod graphics;
pub mod login;
pub mod pager;
pub mod released;
pub mod render;
#[cfg(unix)]
pub mod terminal;
pub mod view;
