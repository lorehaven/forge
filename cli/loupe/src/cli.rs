//! The command line: `loupe FILE`, `loupe -`, `loupe DIR [ROUTE]`, `loupe --check DIR`, `loupe doctor`.

use quench_args::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "loupe")]
#[command(version)]
#[command(
    about = "Loupe - Markdown files and documentation books in the terminal",
    long_about = "Renders a Markdown file (or `-` for stdin) to the terminal's width and pages through it; a \
                  directory is read as a documentation book (Markdown pages with just-the-docs style navigation). \
                  When stdout is not a terminal, or with --plain or NO_COLOR, the page is printed as plain text."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// A Markdown file, `-` for stdin, or a book's directory
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    /// In a book: the page to open (its path without `.md`, as `services/gauge/metrics`)
    #[arg(value_name = "ROUTE")]
    pub route: Option<String>,

    /// Print plain text and exit, without the pager or colours
    #[arg(long)]
    pub plain: bool,

    /// Lay the page out at this many columns instead of the terminal's width
    #[arg(long, value_name = "N")]
    pub width: Option<usize>,

    /// Where footnote definitions go: at the end of the page (the default), or after the paragraph that
    /// references them
    #[arg(long, value_enum)]
    pub footnotes: Option<Footnotes>,

    /// Formulas as their TeX source, or as Unicode text even where the terminal shows images
    #[arg(long, value_enum)]
    pub math: Option<Math>,

    /// `off`: no typeset formulas or images, whatever the terminal can do (default `on`)
    #[arg(long, value_enum)]
    pub images: Option<Images>,

    /// A TrueType math font (with a MATH table) to typeset formulas with, instead of the bundled Noto Sans Math
    #[arg(long, value_name = "PATH")]
    pub math_font: Option<PathBuf>,

    /// Check the book in DIR (front matter, navigation, links); exit 1 on problems
    #[arg(long, value_name = "DIR")]
    pub check: Option<PathBuf>,

    /// A book released to Warehouse by `anvil docs release`: BOOK (its latest version) or BOOK@VERSION; the
    /// page to open is then the one argument. Alone, lists the released books
    #[arg(
        long,
        value_name = "BOOK[@VERSION]",
        num_args = 0..=1,
        default_missing_value = ""
    )]
    pub book: Option<String>,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Command {
    /// What works here: the math font, terminal graphics (kitty, sixel), cell size and colours. Exits 1 when
    /// formulas fall back to text.
    Doctor,
    /// Sign in to Gatehouse and store an application token for reading released books (in loupe's config
    /// directory, `~/.config/loupe/.loupe.secret.toml`)
    Login(LoginArgs),
}

/// `loupe login`.
#[derive(Args, Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginArgs {
    /// Gatehouse with its base path (else `LOUPE_GATEHOUSE_URL`, the config's `gatehouse_url`, `[release]
    /// gatehouse_url` of .anvil.toml, or the stored one)
    #[arg(long, value_name = "URL")]
    pub gatehouse_url: Option<String>,
    /// The Gatehouse user (asked for when not given)
    #[arg(long)]
    pub username: Option<String>,
    /// The token's name (default `loupe on <host>`)
    #[arg(long)]
    pub name: Option<String>,
    /// A grant for the token, `service:action`; may be repeated (default `warehouse:read`)
    #[arg(long, value_name = "GRANT")]
    pub scope: Vec<String>,
    /// Days until the token expires (default: Gatehouse's)
    #[arg(long, value_name = "DAYS")]
    pub days: Option<i64>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footnotes {
    End,
    Inline,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Math {
    Source,
    Text,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Images {
    On,
    Off,
}
