//! What a command line does, with the process's environment and streams passed in: plain output and the
//! checks are written here; the pager and `doctor` are handed back to `main`, which owns the terminal.

use crate::cli::{Cli, Command, Footnotes, Images, LoginArgs, Math};
use crate::config::{Config, Effective};
use crate::fonts::MathFont;
use crate::pager::Pager;
use crate::released::{self, AnvilSettings, Net};
use crate::render::{Settings, render_page};
use docs_book::Book;
use quench_markdown_term::{FootnotePlacement, MathMode, to_plain};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The parts of the environment loupe looks at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    /// `NO_COLOR` is set to something.
    pub no_color: bool,
    /// Standard output is a terminal.
    pub stdout_tty: bool,
    /// The terminal's size (columns, rows), when there is one.
    pub size: Option<(usize, usize)>,
    /// The config file, when there is one: its path and its text.
    pub config: Option<(PathBuf, String)>,
    /// `$HOME`, for `~/` in the config file.
    pub home: Option<String>,
    /// The working directory: where `.anvil.toml` and stored credentials are looked for.
    pub cwd: PathBuf,
    /// Where released books are cached (`$XDG_CACHE_HOME/loupe`, else `~/.cache/loupe`).
    pub cache: Option<PathBuf>,
    /// `LOUPE_WAREHOUSE_URL`, `LOUPE_STORAGE` and `LOUPE_GATEHOUSE_URL`.
    pub warehouse_url: Option<String>,
    pub storage: Option<String>,
    pub gatehouse_url: Option<String>,
}

/// What `main` does next.
#[derive(Debug)]
pub enum Outcome {
    /// Exit with this status.
    Exit(i32),
    /// Run the pager on the terminal.
    Pager(Box<Pager>, PagerOptions),
    /// Run `doctor` with this math font.
    Doctor(MathFont),
    /// Run `login`, with the Gatehouse URL from the flag, the environment or the configuration (`None`: ask
    /// what is stored).
    Login(LoginArgs, Option<String>),
}

/// How the pager may draw.
#[derive(Debug, Clone)]
pub struct PagerOptions {
    /// The page's images may be drawn when the terminal shows images (not `--images off`).
    pub images: bool,
    /// Formulas may be typeset as images too (and not `--math text` or `--math source`).
    pub typeset: bool,
    /// The math font to typeset with (it may have failed to load; formulas are text then).
    pub font: MathFont,
}

/// The default size of a terminal that does not say.
const FALLBACK_SIZE: (usize, usize) = (80, 24);

fn settings(options: &Effective, width: usize, numbered_links: bool) -> Settings {
    Settings {
        width,
        footnotes: match options.footnotes {
            Footnotes::End => FootnotePlacement::End,
            Footnotes::Inline => FootnotePlacement::Inline,
        },
        math: if options.math == Some(Math::Source) {
            MathMode::Source
        } else {
            MathMode::Text
        },
        numbered_links,
        baseurl: String::new(),
    }
}

/// Runs `cli`. Messages go to `err` prefixed `loupe:`; plain pages and check results to `out`.
pub fn run(
    cli: &Cli,
    env: &Env,
    net: &dyn Net,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Outcome {
    match run_inner(cli, env, net, stdin, out, err) {
        Ok(outcome) => outcome,
        Err(Failure { message, status }) => {
            let _ = writeln!(err, "loupe: {message}");
            Outcome::Exit(status)
        }
    }
}

#[derive(Debug)]
struct Failure {
    message: String,
    status: i32,
}

fn fail<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure {
        message: message.into(),
        status: 1,
    })
}

fn usage<T>(message: impl Into<String>) -> Result<T, Failure> {
    Err(Failure {
        message: message.into(),
        status: 2,
    })
}

/// The config file's settings; a file that cannot be read is reported and left out.
fn config(env: &Env, err: &mut dyn Write) -> Config {
    let Some((path, text)) = &env.config else {
        return Config::default();
    };
    match Config::parse(text, path, env.home.as_deref()) {
        Ok(config) => config,
        Err(why) => {
            let _ = writeln!(
                err,
                "loupe: {}: {why}; the config file is ignored",
                path.display()
            );
            Config::default()
        }
    }
}

/// The math font in effect: `--math-font`, else the config file's `math_font`, else the bundled one.
fn math_font(cli: &Cli, config: &Config) -> MathFont {
    match (&cli.math_font, &config.math_font) {
        (Some(path), _) => MathFont::choose(Some(path), "--math-font"),
        (None, Some(path)) => MathFont::choose(Some(path), "the config file's math_font"),
        (None, None) => MathFont::bundled(),
    }
}

fn run_inner(
    cli: &Cli,
    env: &Env,
    net: &dyn Net,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Outcome, Failure> {
    let config = config(env, err);
    let options = Effective::of(cli, &config);
    let font = math_font(cli, &config);
    match &cli.command {
        Some(Command::Doctor) => return Ok(Outcome::Doctor(font)),
        Some(Command::Login(args)) => {
            let url = args
                .gatehouse_url
                .clone()
                .or_else(|| env.gatehouse_url.clone())
                .or_else(|| config.gatehouse_url.clone())
                .or_else(|| AnvilSettings::find(&env.cwd).gatehouse_url);
            return Ok(Outcome::Login(args.clone(), url));
        }
        None => {}
    }
    if let Some(dir) = &cli.check {
        return check(dir, out).map(Outcome::Exit);
    }
    let released = match &cli.book {
        Some(spec) => match released_book(cli, spec, env, &config, net, out, err)? {
            Released::Listed(status) => return Ok(Outcome::Exit(status)),
            Released::Opened(opened) => Some(opened),
        },
        None => None,
    };
    if options.width == Some(0) {
        return usage("--width must be at least 1");
    }
    let path = match (&released, cli.path.as_deref()) {
        (Some(_), _) => "",
        (None, Some(path)) => path,
        (None, None) => {
            return usage(
                "nothing to show: give a Markdown file, `-` for stdin, or a book's directory (see --help)",
            );
        }
    };
    let typeset = options.images == Images::On && options.math.is_none();
    let size = env.size.unwrap_or(FALLBACK_SIZE);
    let show = Show {
        plain: cli.plain || env.no_color || !env.stdout_tty,
        size,
        width: options.width.unwrap_or(size.0),
        // `--math text` and `--math source` ask for text: the font's warning is not shown then.
        warning: if typeset { font.warning() } else { None },
        pager: PagerOptions {
            images: options.images == Images::On,
            typeset,
            font,
        },
        options,
    };
    if let Some(opened) = released {
        let name = format!("{}@{}", opened.book, opened.version);
        return show.book(&name, &opened.dir, cli.path.as_deref(), None, out, err);
    }
    if path == "-" {
        if cli.route.is_some() {
            return usage("a ROUTE is for a book's directory, not stdin");
        }
        let mut markdown = String::new();
        if let Err(e) = stdin.read_to_string(&mut markdown) {
            return fail(format!("cannot read stdin: {e}"));
        }
        return show.file("stdin", None, markdown, out, err);
    }
    let target = PathBuf::from(path);
    if target.is_dir() {
        return show.book(path, &target, cli.route.as_deref(), Some(path), out, err);
    }
    if cli.route.is_some() {
        return usage(format!(
            "a ROUTE is for a book's directory, and {path} is not a directory"
        ));
    }
    let markdown = match std::fs::read_to_string(&target) {
        Ok(m) => m,
        Err(e) => return fail(format!("cannot read {path}: {e}")),
    };
    show.file(path, Some(target), markdown, out, err)
}

/// How a page is shown: printed plain, or handed to the pager.
struct Show {
    plain: bool,
    size: (usize, usize),
    width: usize,
    options: Effective,
    /// The math font's warning, when it cannot be used and formulas would have been typeset.
    warning: Option<String>,
    pager: PagerOptions,
}

impl Show {
    fn settings(&self, numbered_links: bool) -> Settings {
        settings(&self.options, self.width, numbered_links)
    }

    fn file(
        self,
        name: &str,
        path: Option<PathBuf>,
        markdown: String,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<Outcome, Failure> {
        if self.plain {
            return print_plain(
                &markdown,
                &self.settings(true),
                self.warning.as_deref(),
                out,
                err,
            );
        }
        let mut pager = Pager::file(
            name,
            path,
            markdown,
            self.settings(false),
            self.options.width,
            self.size,
        );
        pager.set_warning(self.warning);
        Ok(Outcome::Pager(Box::new(pager), self.pager))
    }

    /// A book in `dir`, shown as `name`; `checkable` is the directory to suggest `--check` on, when there is one.
    fn book(
        self,
        name: &str,
        dir: &Path,
        route: Option<&str>,
        checkable: Option<&str>,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<Outcome, Failure> {
        let book = match Book::open(dir) {
            Ok(book) => book,
            Err(e) => return fail(format!("cannot read the book in {name}: {e}")),
        };
        let page = start_page(&book, route)?;
        if self.plain {
            let mut s = self.settings(true);
            s.baseurl = book.baseurl().to_string();
            let body = format!("\n{}", book.pages()[page].body);
            return print_plain(&body, &s, self.warning.as_deref(), out, err);
        }
        let problems = book.problems().len();
        let mut pager = Pager::book(
            book,
            page,
            self.settings(false),
            self.options.width,
            self.size,
        );
        let problems = (problems > 0).then(|| {
            let plural = if problems == 1 { "" } else { "s" };
            checkable.map_or_else(
                || format!("the book has {problems} problem{plural}"),
                |dir| format!("the book has {problems} problem{plural} (loupe --check {dir})"),
            )
        });
        pager.set_warning(self.warning.or(problems));
        Ok(Outcome::Pager(Box::new(pager), self.pager))
    }
}

/// The options that take a value, so their values are not taken for the subcommand.
const VALUE_OPTIONS: [&str; 7] = [
    "--width",
    "--footnotes",
    "--math",
    "--images",
    "--math-font",
    "--check",
    "--book",
];

/// A hint for a subcommand (`doctor`, `help`) that is also the name of a file or directory in `cwd`.
///
/// The subcommand runs; the hint says how to open the file instead.
#[must_use]
pub fn subcommand_hint(args: &[String], cwd: &Path) -> Option<String> {
    let mut words = args.iter().skip(1);
    while let Some(word) = words.next() {
        if word == "--" {
            return None;
        }
        if VALUE_OPTIONS.contains(&word.as_str()) {
            words.next();
            continue;
        }
        if word.starts_with('-') && word != "-" {
            continue;
        }
        if (word == "doctor" || word == "help") && cwd.join(word).exists() {
            return Some(format!(
                "loupe: running `loupe {word}`; to open the file named {word}, use `loupe ./{word}` or \
                 `loupe -- {word}`"
            ));
        }
        return None;
    }
    None
}

/// What `--book` came to: a listing written, or a book in the cache.
enum Released {
    Listed(i32),
    Opened(released::Opened),
}

/// `--book SPEC`: the book fetched into the cache (or found there); `--book` alone lists them.
fn released_book(
    cli: &Cli,
    spec: &str,
    env: &Env,
    config: &Config,
    net: &dyn Net,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Released, Failure> {
    if cli.route.is_some() {
        return usage(
            "with --book, the page is the one argument: loupe --book BOOK[@VERSION] [ROUTE]",
        );
    }
    let remote = released::remote(
        env.warehouse_url.as_deref(),
        env.storage.as_deref(),
        config.warehouse_url.as_deref(),
        config.storage.as_deref(),
        &AnvilSettings::find(&env.cwd),
    )
    .or_else(fail)?;
    if spec.is_empty() {
        return list_books(&remote, net, out).map(Released::Listed);
    }
    let Some(cache) = &env.cache else {
        return fail("no cache directory for released books: set XDG_CACHE_HOME or HOME");
    };
    let opened = released::open(spec, &remote, net, cache).or_else(fail)?;
    if !opened.cached {
        let _ = writeln!(
            err,
            "loupe: fetched {} {} into {}",
            opened.book,
            opened.version,
            opened.dir.display()
        );
    }
    Ok(Released::Opened(opened))
}

/// `loupe --book`: the released books, one per line: name, latest version, title.
fn list_books(
    remote: &released::Remote,
    net: &dyn Net,
    out: &mut dyn Write,
) -> Result<i32, Failure> {
    let books = released::list(remote, net).or_else(fail)?;
    let written = if books.is_empty() {
        writeln!(
            out,
            "no released books in the `{}` storage at {}",
            remote.storage, remote.warehouse
        )
    } else {
        let width = books.iter().map(|b| b.book.len()).max().unwrap_or(0);
        let version_width = books
            .iter()
            .map(|b| b.version.as_deref().map_or(1, str::len))
            .max()
            .unwrap_or(0);
        books.iter().try_for_each(|b| {
            let line = format!(
                "{:width$}  {:version_width$}  {}",
                b.book,
                b.version.as_deref().unwrap_or("-"),
                b.title.as_deref().unwrap_or("")
            );
            writeln!(out, "{}", line.trim_end())
        })
    };
    written.map_err(|e| Failure {
        message: format!("cannot write: {e}"),
        status: 1,
    })?;
    Ok(0)
}

/// The page a book opens at: `route`, else its index, else the first page of its navigation.
fn start_page(book: &Book, route: Option<&str>) -> Result<usize, Failure> {
    if let Some(route) = route {
        return book
            .page(route)
            .map_or_else(|| fail(format!("the book has no page `{route}`")), Ok);
    }
    if let Some(index) = book.page("") {
        return Ok(index);
    }
    match book.nav().walk().first() {
        Some(&(page, _)) => Ok(page),
        None if !book.pages().is_empty() => Ok(0),
        None => fail(format!("the book in {} has no pages", book.dir().display())),
    }
}

fn print_plain(
    markdown: &str,
    settings: &Settings,
    warning: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Outcome, Failure> {
    let rendered = render_page(markdown, settings, &HashSet::new());
    // The one warning of a run, and only for a page that has formulas.
    if let Some(warning) = warning
        && !rendered.formulas.is_empty()
    {
        let _ = writeln!(err, "loupe: {warning}");
    }
    if let Err(e) = out.write_all(to_plain(&rendered).as_bytes()) {
        // A closed pipe (`loupe x.md | head`) is not worth a message.
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            return fail(format!("cannot write: {e}"));
        }
    }
    Ok(Outcome::Exit(0))
}

/// `--check DIR`: the book's problems, one per line; 1 when there are any.
fn check(dir: &Path, out: &mut dyn Write) -> Result<i32, Failure> {
    let book = match Book::open(dir) {
        Ok(book) => book,
        Err(e) => return fail(format!("cannot read the book in {}: {e}", dir.display())),
    };
    let problems = book.problems();
    let written = if problems.is_empty() {
        writeln!(
            out,
            "{}: ok ({} pages, no problems)",
            dir.display(),
            book.pages().len()
        )
    } else {
        problems
            .iter()
            .try_for_each(|p| writeln!(out, "{p}"))
            .and_then(|()| {
                writeln!(
                    out,
                    "{}: {} problem{}",
                    dir.display(),
                    problems.len(),
                    if problems.len() == 1 { "" } else { "s" }
                )
            })
    };
    if let Err(e) = written {
        return fail(format!("cannot write: {e}"));
    }
    Ok(i32::from(!problems.is_empty()))
}
