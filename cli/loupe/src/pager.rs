//! The pager as a state machine over a screen model: keys in, a [`Screen`] and [`Effect`]s out.
//!
//! No terminal
//! I/O happens here (see `terminal.rs` for that), so tests feed keys and assert on the screen's text.

use crate::render::{Settings, render_page};
use docs_book::{Book, Target};
use quench_image::Image;
use quench_markdown_term::{FormulaSpot, Line, LinkKind, Rendered, Role, Style, char_width};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A key, as the pager sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// A control key: `Ctrl('c')`.
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

/// What the pager asks of the world outside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Quit,
    /// Open a URL or a file with `xdg-open` (the reader has confirmed).
    Open(String),
    /// Copy text to the clipboard (OSC 52).
    Copy(String),
    /// Suspend the program (Ctrl-Z, SIGTSTP): restore the terminal, stop, and redraw on return.
    Suspend,
    /// Set the terminal up again and redraw (SIGCONT: the shell may have reset it while stopped).
    Redraw,
    /// Restore the terminal and exit with this status (SIGINT 130, SIGTERM 143).
    Exit(i32),
}

/// A signal from outside, as the pager sees it: delivered like a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// SIGINT.
    Interrupt,
    /// SIGTERM.
    Terminate,
    /// SIGTSTP.
    Stop,
    /// SIGCONT.
    Continue,
    /// SIGWINCH, with the terminal's new size (columns, rows).
    Resize(usize, usize),
}

/// The document shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doc {
    /// A Markdown file (or stdin, without a path).
    File {
        name: String,
        path: Option<PathBuf>,
        markdown: String,
    },
    /// A page of the book, by index.
    Book(usize),
}

/// A place to come back to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Location {
    doc: Doc,
    top: usize,
}

/// What the keys act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Typing a search after `/`.
    Search(String),
    /// The table of contents, with the selected heading.
    Toc(usize),
    /// The footnotes, with the selected one.
    Footnotes(usize),
    /// The book's navigation: the filter typed and the selected entry among those it keeps.
    Nav {
        filter: String,
        selected: usize,
    },
    /// Asking before opening something outside: the target.
    Confirm(String),
}

/// How a piece of the screen is marked over its own style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    None,
    /// A search match.
    Match,
    /// The current search match.
    Current,
    /// The focused link.
    Focus,
    /// The selected entry of a list (table of contents, footnotes, navigation).
    Selected,
}

/// A run of text in one style and mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub style: Style,
    pub mark: Mark,
}

/// One row of the screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub pieces: Vec<Piece>,
}

impl Row {
    /// The row's text without styles.
    #[must_use]
    pub fn text(&self) -> String {
        self.pieces.iter().map(|p| p.text.as_str()).collect()
    }

    fn push(&mut self, text: &str, style: Style, mark: Mark) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.pieces.last_mut()
            && last.style == style
            && last.mark == mark
        {
            last.text.push_str(text);
            return;
        }
        self.pieces.push(Piece {
            text: text.to_string(),
            style,
            mark,
        });
    }

    fn plain(text: &str, mark: Mark) -> Self {
        let mut row = Self::default();
        row.push(text, Style::default(), mark);
        row
    }
}

/// What the terminal shows: the content rows and the status line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Screen {
    pub rows: Vec<Row>,
    pub status: String,
}

impl Screen {
    /// The rows' text, one per line, then the status line.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(row.text().trim_end());
            out.push('\n');
        }
        out.push_str(&self.status);
        out
    }
}

/// A link's place on the page: a line, the characters it covers there, and its index in `Rendered::links`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSpot {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub link: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Search {
    query: String,
    /// (line, first char, end char).
    matches: Vec<(usize, usize, usize)>,
    current: Option<usize>,
}

/// Something the terminal side may draw as a picture: a formula or an image of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Item {
    /// Index into `Rendered::formulas`.
    Formula(usize),
    /// Index into [`Pager::images`].
    Image(usize),
}

/// What a picture is made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A formula to typeset.
    Tex { tex: String, display: bool },
    /// A decoded image, drawn scaled to its cells.
    Image(Arc<Image>),
}

/// A picture on screen now, in normal mode: at least one of its rows is visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub item: Item,
    pub content: Content,
    /// Cell column of its left edge, and the screen row of its first visible row.
    pub column: usize,
    pub row: usize,
    /// Cells across and rows down, whole.
    pub width: usize,
    pub height: usize,
    /// Rows cut off above the screen, and rows shown.
    pub skip: usize,
    pub shown: usize,
}

/// An image of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSpot {
    /// Index into `Rendered::links`.
    pub link: usize,
    /// The line of its `[image: alt]` text.
    pub line: usize,
    pub state: ImageState,
}

/// Whether an image is drawn, and where; or why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageState {
    /// Drawn in `rows` rows reserved under its text, from `first_row`, `columns` cells across from `column`.
    Shown {
        path: PathBuf,
        image: Arc<Image>,
        first_row: usize,
        column: usize,
        columns: usize,
        rows: usize,
    },
    /// Shown as its text only, and why.
    Text(String),
}

/// The most rows an image takes.
pub const IMAGE_MAX_ROWS: usize = 20;

/// The pager.
#[derive(Debug)]
pub struct Pager {
    book: Option<Book>,
    doc: Doc,
    settings: Settings,
    /// `--width`: lay out at this width whatever the terminal's.
    fixed_width: Option<usize>,
    columns: usize,
    rows: usize,
    rendered: Rendered,
    links: Vec<LinkSpot>,
    top: usize,
    focus: Option<usize>,
    search: Option<Search>,
    mode: Mode,
    history: Vec<Location>,
    message: Option<String>,
    warning: Option<String>,
    source_tex: HashSet<String>,
    generation: u64,
    /// The terminal's cell size in pixels when it shows images: images of the page are drawn then.
    cell: Option<(u32, u32)>,
    images: Vec<ImageSpot>,
    image_cache: HashMap<PathBuf, Result<Arc<Image>, String>>,
}

impl Pager {
    /// A pager over a Markdown file (`path` is `None` for stdin) on a screen of `size` (columns, rows).
    #[must_use]
    pub fn file(
        name: &str,
        path: Option<PathBuf>,
        markdown: String,
        settings: Settings,
        fixed_width: Option<usize>,
        size: (usize, usize),
    ) -> Self {
        let doc = Doc::File {
            name: name.to_string(),
            path,
            markdown,
        };
        Self::new(None, doc, settings, fixed_width, size)
    }

    /// A pager over page `page` of `book`.
    #[must_use]
    pub fn book(
        book: Book,
        page: usize,
        mut settings: Settings,
        fixed_width: Option<usize>,
        size: (usize, usize),
    ) -> Self {
        settings.baseurl = book.baseurl().to_string();
        Self::new(Some(book), Doc::Book(page), settings, fixed_width, size)
    }

    fn new(
        book: Option<Book>,
        doc: Doc,
        settings: Settings,
        fixed_width: Option<usize>,
        (columns, rows): (usize, usize),
    ) -> Self {
        let mut pager = Self {
            book,
            doc,
            settings,
            fixed_width,
            columns: columns.max(1),
            rows: rows.max(2),
            rendered: Rendered::default(),
            links: Vec::new(),
            top: 0,
            focus: None,
            search: None,
            mode: Mode::Normal,
            history: Vec::new(),
            message: None,
            warning: None,
            source_tex: HashSet::new(),
            generation: 0,
            cell: None,
            images: Vec::new(),
            image_cache: HashMap::new(),
        };
        pager.layout();
        pager
    }

    // ---- reading the state -------------------------------------------------------------------------------------

    /// The laid-out page.
    #[must_use]
    pub const fn rendered(&self) -> &Rendered {
        &self.rendered
    }

    /// The first line on screen.
    #[must_use]
    pub const fn top(&self) -> usize {
        self.top
    }

    #[must_use]
    pub const fn mode(&self) -> &Mode {
        &self.mode
    }

    #[must_use]
    pub const fn doc(&self) -> &Doc {
        &self.doc
    }

    /// The width pages are laid out at.
    #[must_use]
    pub fn width(&self) -> usize {
        self.fixed_width.unwrap_or(self.columns).max(4)
    }

    /// Bumped each time the page is laid out again: cached images of its formulas are stale then.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The focused link, if any.
    #[must_use]
    pub fn focused(&self) -> Option<LinkSpot> {
        self.focus.and_then(|f| self.links.get(f).copied())
    }

    /// Where the links are on the page.
    #[must_use]
    pub fn links(&self) -> &[LinkSpot] {
        &self.links
    }

    /// How many places `Backspace` can go back to.
    #[must_use]
    pub const fn history_len(&self) -> usize {
        self.history.len()
    }

    /// The images of the page: drawn, or why not.
    #[must_use]
    pub fn images(&self) -> &[ImageSpot] {
        &self.images
    }

    /// The terminal shows images, with cells of `cell` pixels (or `None`: it does not). Lays the page out again
    /// when that changes, since drawn images take rows.
    pub fn set_graphics(&mut self, cell: Option<(u32, u32)>) {
        if self.cell != cell {
            self.cell = cell;
            self.relayout();
        }
    }

    /// A message for the status line, until the next key.
    pub fn set_message(&mut self, message: Option<String>) {
        self.message = message;
    }

    /// A warning kept on the status line (formulas falling back to text, a book's problems).
    pub fn set_warning(&mut self, warning: Option<String>) {
        self.warning = warning;
    }

    fn content_rows(&self) -> usize {
        self.rows.saturating_sub(1).max(1)
    }

    fn max_top(&self) -> usize {
        self.rendered
            .lines
            .len()
            .saturating_sub(self.content_rows())
    }

    // ---- layout ------------------------------------------------------------------------------------------------

    fn markdown(&self) -> (String, bool) {
        match &self.doc {
            Doc::File { markdown, .. } => (markdown.clone(), true),
            Doc::Book(page) => (
                self.book
                    .as_ref()
                    .and_then(|b| b.pages().get(*page))
                    .map(|p| p.body.clone())
                    .unwrap_or_default(),
                false,
            ),
        }
    }

    /// Lays the current document out again at the current width, from the top.
    fn layout(&mut self) {
        let mut settings = self.settings.clone();
        settings.width = self.width();
        let (markdown, whole_file) = self.markdown();
        let text = if whole_file {
            markdown
        } else {
            // A book page's body has no front matter left; keep a leading `---` from being taken for one.
            format!("\n{markdown}")
        };
        self.rendered = render_page(&text, &settings, &self.source_tex);
        self.place_images();
        self.links = link_spots(&self.rendered);
        self.focus = None;
        self.generation += 1;
        if let Some(search) = &self.search {
            let query = search.query.clone();
            self.search = Some(Search {
                matches: find_matches(&self.rendered, &query),
                query,
                current: None,
            });
        }
        self.top = self.top.min(self.max_top());
    }

    /// The directory a relative link of the page starts from, and the root a `/` path starts from.
    fn link_base(&self) -> (PathBuf, PathBuf) {
        match (&self.doc, &self.book) {
            (Doc::Book(page), Some(book)) => {
                let from = book.pages().get(*page).map_or("", |p| p.path.as_str());
                let dir = book
                    .dir()
                    .join(Path::new(from).parent().unwrap_or_else(|| Path::new("")));
                (dir, book.dir().to_path_buf())
            }
            (Doc::File { path, .. }, _) => {
                let dir = path
                    .as_deref()
                    .and_then(Path::parent)
                    .filter(|d| !d.as_os_str().is_empty())
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                (dir, PathBuf::from("/"))
            }
            (Doc::Book(_), None) => (PathBuf::from("."), PathBuf::from("/")),
        }
    }

    /// Reads the image at `destination` (a link of the page): decoded, or why not.
    fn load_image(&mut self, destination: &str) -> Result<(PathBuf, Arc<Image>), String> {
        let path = destination.split(['#', '?']).next().unwrap_or_default();
        let path = path.replace("%20", " ");
        let (dir, root) = self.link_base();
        let file = if let Some(rooted) = path.strip_prefix('/') {
            let base = self.settings.baseurl.trim_matches('/');
            let rooted = if base.is_empty() {
                rooted
            } else {
                rooted
                    .strip_prefix(base)
                    .map_or(rooted, |r| r.trim_start_matches('/'))
            };
            root.join(rooted)
        } else {
            dir.join(&path)
        };
        let loaded = self
            .image_cache
            .entry(file.clone())
            .or_insert_with(|| {
                let bytes = std::fs::read(&file)
                    .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
                quench_image::decode(&bytes)
                    .map(Arc::new)
                    .map_err(|e| format!("{}: {e}", file.display()))
            })
            .clone();
        loaded.map(|image| (file, image))
    }

    /// Finds the images of the page, and reserves rows under the text of each one that is drawn.
    fn place_images(&mut self) {
        let spots = link_spots(&self.rendered);
        let width = self.width();
        let max_rows = IMAGE_MAX_ROWS
            .min(self.content_rows().saturating_sub(1))
            .max(1);
        let mut found = Vec::new();
        for (link, target) in self.rendered.links.iter().enumerate() {
            if target.kind != LinkKind::Image {
                continue;
            }
            let Some(spot) = spots.iter().find(|s| s.link == link) else {
                continue;
            };
            let column = self
                .rendered
                .lines
                .get(spot.line)
                .map_or(0, |line| cells_before(line, spot.start));
            found.push((
                link,
                spot.line,
                column,
                target.destination.clone(),
                target.is_url(),
            ));
        }
        let mut images = Vec::new();
        let mut inserted = 0;
        for (link, line, column, destination, remote) in found {
            let line = line + inserted;
            let state = if remote {
                ImageState::Text(format!("{destination}: remote images are not fetched"))
            } else if destination.is_empty() {
                ImageState::Text("the image has no destination".to_string())
            } else {
                match (self.load_image(&destination), self.cell) {
                    (Err(why), _) => ImageState::Text(why),
                    (Ok((path, _)), None) => ImageState::Text(format!(
                        "{}: the terminal shows no images (see loupe doctor)",
                        path.display()
                    )),
                    (Ok((path, image)), Some(cell)) => {
                        let (columns, rows) = image_cells(
                            &image,
                            cell,
                            width.saturating_sub(column).max(1),
                            max_rows,
                        );
                        // The rows of every image above have already been inserted.
                        insert_blank(&mut self.rendered, line + 1, rows);
                        inserted += rows;
                        ImageState::Shown {
                            path,
                            image,
                            first_row: line + 1,
                            column,
                            columns,
                            rows,
                        }
                    }
                }
            };
            images.push(ImageSpot { link, line, state });
        }
        self.images = images;
    }

    /// Lays the page out again keeping the reader's place: the heading at or above the top line, and how far
    /// below it the top was; or the same fraction of the page when there is no heading above.
    fn relayout(&mut self) {
        let before = self.anchor();
        self.layout();
        self.top = match before {
            Anchor::Heading(k, offset) => {
                self.rendered.headings.get(k).map_or(0, |h| h.line + offset)
            }
            Anchor::Fraction(f) => {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let top = (f * self.rendered.lines.len() as f64).round() as usize;
                top
            }
        }
        .min(self.max_top());
    }

    fn anchor(&self) -> Anchor {
        let heading = self
            .rendered
            .headings
            .iter()
            .enumerate()
            .rev()
            .find(|(_, h)| h.line <= self.top);
        if let Some((k, h)) = heading {
            let next = self
                .rendered
                .headings
                .get(k + 1)
                .map_or(usize::MAX, |n| n.line);
            // Keep the offset within the section, so a reflow cannot push the top into the next one.
            Anchor::Heading(k, (self.top - h.line).min(next.saturating_sub(h.line + 1)))
        } else {
            let total = self.rendered.lines.len().max(1);
            #[allow(clippy::cast_precision_loss)]
            let f = self.top as f64 / total as f64;
            Anchor::Fraction(f)
        }
    }

    /// The terminal is now `columns` × `rows`.
    pub fn resize(&mut self, columns: usize, rows: usize) {
        let columns = columns.max(1);
        let rows = rows.max(2);
        if (columns, rows) == (self.columns, self.rows) {
            return;
        }
        let width_before = self.width();
        self.columns = columns;
        self.rows = rows;
        let images_drawn = self
            .images
            .iter()
            .any(|i| matches!(i.state, ImageState::Shown { .. }));
        if self.width() == width_before && !images_drawn {
            self.top = self.top.min(self.max_top());
        } else {
            self.relayout();
        }
    }

    // ---- moving ------------------------------------------------------------------------------------------------

    /// Scrolls to `top`, within the page; a top past the end (left by a jump to a heading) is kept, not
    /// increased.
    fn scroll_to(&mut self, top: usize) {
        self.top = top.min(self.max_top().max(self.top));
        if let Some(spot) = self.focused()
            && !self.on_screen(spot.line)
        {
            self.focus = None;
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        self.scroll_to(self.top.saturating_add_signed(delta));
    }

    fn on_screen(&self, line: usize) -> bool {
        line >= self.top && line < self.top + self.content_rows()
    }

    /// Shows `line` near the top of the screen (two lines of context above it).
    fn show_line(&mut self, line: usize) {
        if !self.on_screen(line) {
            self.scroll_to(line.saturating_sub(2));
        }
    }

    /// Jumps to `line` as a place to come back from.
    fn jump(&mut self, line: usize) {
        self.push_history();
        self.scroll_to(line);
    }

    /// Jumps to the heading at `line`, which becomes the top line even near the end of the page (the screen
    /// is padded below the page).
    fn jump_heading(&mut self, line: usize) {
        self.push_history();
        self.show_heading(line);
    }

    fn show_heading(&mut self, line: usize) {
        self.top = line.min(self.rendered.lines.len().saturating_sub(1));
        self.focus = None;
    }

    fn push_history(&mut self) {
        self.history.push(Location {
            doc: self.doc.clone(),
            top: self.top,
        });
    }

    fn back(&mut self) {
        let Some(location) = self.history.pop() else {
            self.message = Some("nothing to go back to".to_string());
            return;
        };
        if location.doc != self.doc {
            self.doc = location.doc;
            self.top = 0;
            self.layout();
        }
        self.top = location
            .top
            .min(self.rendered.lines.len().saturating_sub(1));
        self.focus = None;
    }

    /// Opens `doc` (a place to come back from), at the heading `anchor` when given.
    fn open(&mut self, doc: Doc, anchor: Option<&str>) {
        self.push_history();
        if doc != self.doc {
            self.doc = doc;
            self.top = 0;
            self.layout();
        }
        self.top = 0;
        if let Some(anchor) = anchor.filter(|a| !a.is_empty()) {
            match self.rendered.heading_line(anchor) {
                Some(line) => self.show_heading(line),
                None => self.message = Some(format!("no heading #{anchor} on this page")),
            }
        }
    }

    // ---- keys --------------------------------------------------------------------------------------------------

    /// Handles a key press.
    pub fn key(&mut self, key: Key) -> Option<Effect> {
        if matches!(key, Key::Ctrl('c')) {
            return Some(Effect::Quit);
        }
        if matches!(key, Key::Ctrl('z')) {
            return Some(Effect::Suspend);
        }
        self.message = None;
        match self.mode.clone() {
            Mode::Normal => self.normal_key(key),
            Mode::Search(query) => {
                self.search_key(key, query);
                None
            }
            Mode::Toc(selected) => {
                self.toc_key(key, selected);
                None
            }
            Mode::Footnotes(selected) => {
                self.footnotes_key(key, selected);
                None
            }
            Mode::Nav { filter, selected } => {
                self.nav_key(key, filter, selected);
                None
            }
            Mode::Confirm(target) => {
                self.mode = Mode::Normal;
                if matches!(key, Key::Char('y' | 'Y')) {
                    Some(Effect::Open(target))
                } else {
                    self.message = Some("not opened".to_string());
                    None
                }
            }
        }
    }

    fn normal_key(&mut self, key: Key) -> Option<Effect> {
        let page = self.content_rows();
        #[allow(clippy::cast_possible_wrap)]
        let (page, half) = (page as isize, (page / 2).max(1) as isize);
        match key {
            Key::Char('q') => return Some(Effect::Quit),
            Key::Char('j') | Key::Down => self.scroll_by(1),
            Key::Char('k') | Key::Up => self.scroll_by(-1),
            Key::Char(' ') | Key::PageDown => self.scroll_by(page),
            Key::Char('b') | Key::PageUp => self.scroll_by(-page),
            Key::Char('d') => self.scroll_by(half),
            Key::Char('u') => self.scroll_by(-half),
            Key::Char('g') | Key::Home => self.scroll_to(0),
            Key::Char('G') | Key::End => {
                self.top = self.max_top();
                self.scroll_to(self.top);
            }
            Key::Char('/') => self.mode = Mode::Search(String::new()),
            Key::Char('n') => self.next_match(true),
            Key::Char('N') => self.next_match(false),
            Key::Tab => self.move_focus(true),
            Key::BackTab => self.move_focus(false),
            Key::Enter => match self.focused() {
                Some(spot) => return self.follow(spot.link),
                None => self.scroll_by(1),
            },
            Key::Backspace | Key::Char('h') | Key::Left => self.back(),
            Key::Char('t') => {
                if self.rendered.headings.is_empty() {
                    self.message = Some("this page has no headings".to_string());
                } else {
                    self.mode = Mode::Toc(self.current_heading());
                }
            }
            Key::Char('f') => {
                if self.rendered.footnotes.is_empty() {
                    self.message = Some("this page has no footnotes".to_string());
                } else {
                    self.mode = Mode::Footnotes(0);
                }
            }
            Key::Char('o') => {
                if self.book.is_some() {
                    let selected = self
                        .nav_entries("")
                        .iter()
                        .position(|&(p, _)| Doc::Book(p) == self.doc);
                    self.mode = Mode::Nav {
                        filter: String::new(),
                        selected: selected.unwrap_or(0),
                    };
                } else {
                    self.message =
                        Some("not a book: open a directory to browse its navigation".to_string());
                }
            }
            Key::Char('[') => self.neighbour(false),
            Key::Char(']') => self.neighbour(true),
            Key::Char('m') => self.toggle_formula(),
            Key::Char('y') => return self.copy_code(),
            Key::Esc => {
                self.focus = None;
                if let Some(search) = &mut self.search {
                    search.current = None;
                }
                self.search = None;
            }
            _ => {}
        }
        None
    }

    /// The line the "cursor" is on: the focused link's, else the top line.
    fn cursor_line(&self) -> usize {
        self.focused().map_or(self.top, |s| s.line)
    }

    // ---- search ------------------------------------------------------------------------------------------------

    fn search_key(&mut self, key: Key, mut query: String) {
        match key {
            Key::Esc => self.mode = Mode::Normal,
            Key::Enter => {
                self.mode = Mode::Normal;
                if query.is_empty() {
                    // `/` Enter repeats the last search, as in less.
                    self.next_match(true);
                } else {
                    self.start_search(&query);
                }
            }
            Key::Backspace => {
                if query.pop().is_none() {
                    self.mode = Mode::Normal;
                } else {
                    self.mode = Mode::Search(query);
                }
            }
            Key::Char(c) => {
                query.push(c);
                self.mode = Mode::Search(query);
            }
            _ => self.mode = Mode::Search(query),
        }
    }

    fn start_search(&mut self, query: &str) {
        let matches = find_matches(&self.rendered, query);
        if matches.is_empty() {
            self.message = Some(format!("pattern not found: {query}"));
            self.search = None;
            return;
        }
        let first = matches
            .iter()
            .position(|&(line, _, _)| line >= self.top)
            .unwrap_or(0);
        let line = matches[first].0;
        self.search = Some(Search {
            query: query.to_string(),
            matches,
            current: Some(first),
        });
        self.show_line(line);
    }

    fn next_match(&mut self, forward: bool) {
        let top = self.top;
        let Some(search) = &mut self.search else {
            self.message = Some("no search yet: type / and a pattern".to_string());
            return;
        };
        let count = search.matches.len();
        let next = match search.current {
            Some(c) if forward => (c + 1) % count,
            Some(c) => (c + count - 1) % count,
            None => {
                let after = search.matches.iter().position(|&(l, _, _)| l >= top);
                if forward {
                    after.unwrap_or(0)
                } else {
                    after.map_or(count - 1, |a| (a + count - 1) % count)
                }
            }
        };
        let wrapped = search
            .current
            .is_some_and(|c| if forward { next <= c } else { next >= c });
        search.current = Some(next);
        let line = search.matches[next].0;
        if wrapped {
            self.message = Some(format!(
                "search wrapped to the {}",
                if forward { "top" } else { "bottom" }
            ));
        }
        self.show_line(line);
    }

    // ---- links -------------------------------------------------------------------------------------------------

    fn move_focus(&mut self, forward: bool) {
        if self.links.is_empty() {
            self.message = Some("no links on this page".to_string());
            return;
        }
        let count = self.links.len();
        let visible_focus = self.focus.filter(|&f| self.on_screen(self.links[f].line));
        let next = match visible_focus {
            Some(f) if forward => (f + 1) % count,
            Some(f) => (f + count - 1) % count,
            None if forward => self
                .links
                .iter()
                .position(|s| s.line >= self.top)
                .unwrap_or(0),
            None => self
                .links
                .iter()
                .rposition(|s| s.line < self.top + self.content_rows())
                .unwrap_or(count - 1),
        };
        self.focus = Some(next);
        let line = self.links[next].line;
        if !self.on_screen(line) {
            if line < self.top {
                self.top = line.min(self.max_top());
            } else {
                self.top = (line + 1)
                    .saturating_sub(self.content_rows())
                    .min(self.max_top());
            }
        }
    }

    /// Follows link `link` of the page.
    fn follow(&mut self, index: usize) -> Option<Effect> {
        let target = self.rendered.links.get(index)?.clone();
        match target.kind {
            LinkKind::Footnote(n) => {
                if let Some(f) = self.rendered.footnotes.iter().find(|f| f.number == n) {
                    let definition = f.definition_line;
                    self.jump(definition);
                }
                return None;
            }
            LinkKind::FootnoteBack(n) => {
                if let Some(reference) = self
                    .rendered
                    .footnotes
                    .iter()
                    .find(|f| f.number == n)
                    .and_then(|f| f.reference_lines.first().copied())
                {
                    self.jump(reference);
                }
                return None;
            }
            LinkKind::Link | LinkKind::Image => {}
        }
        if target.is_url() {
            self.mode = Mode::Confirm(target.destination);
            return None;
        }
        let destination = target.destination;
        if let Some(anchor) = destination.strip_prefix('#') {
            match self.rendered.heading_line(anchor) {
                Some(heading) => self.jump_heading(heading),
                None => self.message = Some(format!("no heading #{anchor} on this page")),
            }
            return None;
        }
        match &self.doc {
            Doc::Book(page) => self.follow_in_book(*page, &destination),
            Doc::File { path, .. } => {
                let dir = path
                    .as_deref()
                    .and_then(Path::parent)
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                self.follow_file(&dir, &destination);
            }
        }
        None
    }

    fn follow_in_book(&mut self, page: usize, destination: &str) {
        let Some(book) = &self.book else { return };
        match book.resolve(page, destination) {
            Target::Page { page, anchor } => self.open(Doc::Book(page), anchor.as_deref()),
            Target::Anchor(anchor) => match self.rendered.heading_line(&anchor) {
                Some(line) => self.jump_heading(line),
                None => self.message = Some(format!("no heading #{anchor} on this page")),
            },
            Target::Outside(target) => {
                let from = book.pages().get(page).map_or("", |p| p.path.as_str());
                let dir = book
                    .dir()
                    .join(Path::new(from).parent().unwrap_or_else(|| Path::new("")));
                let path = target.split(['#', '?']).next().unwrap_or_default();
                let file = if path.starts_with('/') {
                    book.dir().join(path.trim_start_matches('/'))
                } else {
                    dir.join(path)
                };
                self.mode = Mode::Confirm(file.to_string_lossy().into_owned());
            }
            Target::Missing(target) => {
                self.message = Some(format!("no page in the book for `{target}`"));
            }
        }
    }

    fn follow_file(&mut self, dir: &Path, destination: &str) {
        let (path, anchor) = match destination.split_once('#') {
            Some((p, a)) => (p, Some(a)),
            None => (destination, None),
        };
        let path = path.split('?').next().unwrap_or_default();
        let file = dir.join(path);
        if !file.exists() {
            self.message = Some(format!("missing: `{destination}` (no {})", file.display()));
            return;
        }
        let is_markdown = file
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
        if is_markdown && file.is_file() {
            match std::fs::read_to_string(&file) {
                Ok(markdown) => {
                    let name = file.to_string_lossy().into_owned();
                    self.open(
                        Doc::File {
                            name,
                            path: Some(file),
                            markdown,
                        },
                        anchor,
                    );
                }
                Err(e) => self.message = Some(format!("cannot read {}: {e}", file.display())),
            }
        } else {
            self.mode = Mode::Confirm(file.to_string_lossy().into_owned());
        }
    }

    // ---- table of contents and footnotes ----------------------------------------------------------------------

    fn current_heading(&self) -> usize {
        self.rendered
            .headings
            .iter()
            .rposition(|h| h.line <= self.top)
            .unwrap_or(0)
    }

    fn toc_key(&mut self, key: Key, selected: usize) {
        let count = self.rendered.headings.len();
        match list_move(key, selected, count) {
            ListKey::Moved(s) => self.mode = Mode::Toc(s),
            ListKey::Close | ListKey::Other(Key::Char('t' | 'q')) => self.mode = Mode::Normal,
            ListKey::Choose => {
                self.mode = Mode::Normal;
                if let Some(h) = self.rendered.headings.get(selected) {
                    let line = h.line;
                    self.jump_heading(line);
                }
            }
            ListKey::Other(_) => {}
        }
    }

    fn footnotes_key(&mut self, key: Key, selected: usize) {
        let count = self.rendered.footnotes.len();
        match list_move(key, selected, count) {
            ListKey::Moved(s) => self.mode = Mode::Footnotes(s),
            ListKey::Close | ListKey::Other(Key::Char('f' | 'q')) => self.mode = Mode::Normal,
            ListKey::Choose => {
                self.mode = Mode::Normal;
                if let Some(f) = self.rendered.footnotes.get(selected) {
                    let line = f.definition_line;
                    self.jump(line);
                }
            }
            ListKey::Other(Key::Backspace) => {
                self.mode = Mode::Normal;
                self.back();
            }
            ListKey::Other(_) => {}
        }
    }

    // ---- book navigation ---------------------------------------------------------------------------------------

    /// The navigation's entries (page, depth) whose title or route contains `filter`, ignoring case.
    fn nav_entries(&self, filter: &str) -> Vec<(usize, usize)> {
        let Some(book) = &self.book else {
            return Vec::new();
        };
        let filter = filter.to_lowercase();
        book.nav()
            .walk()
            .into_iter()
            .filter(|&(p, _)| {
                filter.is_empty()
                    || book.pages().get(p).is_some_and(|page| {
                        page.label().to_lowercase().contains(&filter)
                            || page.route().to_lowercase().contains(&filter)
                    })
            })
            .collect()
    }

    fn nav_key(&mut self, key: Key, mut filter: String, selected: usize) {
        let entries = self.nav_entries(&filter);
        let count = entries.len();
        match key {
            Key::Esc => self.mode = Mode::Normal,
            Key::Enter => {
                self.mode = Mode::Normal;
                if let Some(&(page, _)) = entries.get(selected) {
                    self.open(Doc::Book(page), None);
                }
            }
            Key::Up | Key::BackTab => {
                self.mode = Mode::Nav {
                    filter,
                    selected: selected.saturating_sub(1),
                }
            }
            Key::Down | Key::Tab => {
                self.mode = Mode::Nav {
                    filter,
                    selected: (selected + 1).min(count.saturating_sub(1)),
                }
            }
            Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                if let ListKey::Moved(s) = list_move(key, selected, count) {
                    self.mode = Mode::Nav {
                        filter,
                        selected: s,
                    };
                }
            }
            Key::Backspace => {
                filter.pop();
                self.mode = Mode::Nav {
                    filter,
                    selected: 0,
                };
            }
            Key::Char(c) => {
                filter.push(c);
                self.mode = Mode::Nav {
                    filter,
                    selected: 0,
                };
            }
            _ => {}
        }
    }

    fn neighbour(&mut self, next: bool) {
        let (Some(book), Doc::Book(page)) = (&self.book, &self.doc) else {
            self.message = Some("not in a book".to_string());
            return;
        };
        let (before, after) = book.nav().neighbours(*page);
        match if next { after } else { before } {
            Some(p) => self.open(Doc::Book(p), None),
            None => {
                self.message = Some(
                    if next {
                        "this is the last page"
                    } else {
                        "this is the first page"
                    }
                    .to_string(),
                );
            }
        }
    }

    // ---- formulas and code -------------------------------------------------------------------------------------

    /// The formula under the cursor: on the cursor's line or after it on screen, else the first on screen.
    fn formula_at_cursor(&self) -> Option<&FormulaSpot> {
        let cursor = self.cursor_line();
        let bottom = self.top + self.content_rows();
        let covers = |f: &&FormulaSpot| cursor >= f.line && cursor < f.line + f.height.max(1);
        let formulas = &self.rendered.formulas;
        formulas
            .iter()
            .find(covers)
            .or_else(|| {
                formulas
                    .iter()
                    .find(|f| f.line >= cursor && f.line < bottom)
            })
            .or_else(|| {
                formulas
                    .iter()
                    .find(|f| f.line >= self.top && f.line < bottom)
            })
    }

    fn toggle_formula(&mut self) {
        let Some(tex) = self.formula_at_cursor().map(|f| f.tex.trim().to_string()) else {
            self.message = Some("no formula on screen".to_string());
            return;
        };
        let shown = if self.source_tex.remove(&tex) {
            "as text"
        } else {
            self.source_tex.insert(tex);
            "as TeX source"
        };
        self.relayout();
        self.message = Some(format!("formula shown {shown}"));
    }

    /// The code block under the cursor, or the first one on screen after it: its text, as written.
    #[must_use]
    pub fn code_at_cursor(&self) -> Option<String> {
        let blocks = code_blocks(&self.rendered.lines);
        let cursor = self.cursor_line();
        let bottom = self.top + self.content_rows();
        let (start, end) = blocks
            .iter()
            // On a content line or on the frame around it.
            .find(|&&(s, e)| cursor + 1 >= s && cursor <= e + 1)
            .or_else(|| blocks.iter().find(|&&(s, _)| s >= cursor && s < bottom))
            .or_else(|| blocks.iter().find(|&&(s, e)| e >= self.top && s < bottom))
            .copied()?;
        let mut lines: Vec<String> = Vec::new();
        for line in &self.rendered.lines[start..=end] {
            let Some(inner) = code_text(line) else {
                continue;
            };
            let inner = inner.trim_end();
            match (inner.strip_prefix("↪ "), lines.last_mut()) {
                (Some(rest), Some(last)) => last.push_str(rest),
                _ => lines.push(inner.to_string()),
            }
        }
        Some(lines.join("\n"))
    }

    fn copy_code(&mut self) -> Option<Effect> {
        let Some(text) = self.code_at_cursor() else {
            self.message = Some("no code block on screen".to_string());
            return None;
        };
        let lines = text.lines().count().max(1);
        self.message = Some(format!(
            "copied {lines} line{} to the clipboard",
            if lines == 1 { "" } else { "s" }
        ));
        Some(Effect::Copy(text))
    }

    /// The pictures that may be drawn now: formulas shown as text (not switched to their source) and images of
    /// the page, with at least one row on screen, in normal mode. One cut by the top or bottom of the screen is
    /// drawn cropped to its visible rows.
    #[must_use]
    pub fn candidates(&self) -> Vec<Candidate> {
        if !matches!(self.mode, Mode::Normal | Mode::Search(_) | Mode::Confirm(_)) {
            return Vec::new();
        }
        let (top, bottom) = (self.top, self.top + self.content_rows());
        let visible = |first: usize, height: usize| {
            let start = first.max(top);
            let end = (first + height).min(bottom);
            (start < end).then(|| (start - top, start - first, end - start))
        };
        let mut out = Vec::new();
        for (index, f) in self.rendered.formulas.iter().enumerate() {
            if self.source_tex.contains(f.tex.trim()) {
                continue;
            }
            let height = f.height.max(1);
            if let Some((row, skip, shown)) = visible(f.line, height) {
                out.push(Candidate {
                    item: Item::Formula(index),
                    content: Content::Tex {
                        tex: f.tex.clone(),
                        display: f.display,
                    },
                    column: if f.display { 0 } else { f.column },
                    row,
                    width: if f.display { self.width() } else { f.width },
                    height,
                    skip,
                    shown,
                });
            }
        }
        for (index, spot) in self.images.iter().enumerate() {
            let ImageState::Shown {
                image,
                first_row,
                column,
                columns,
                rows,
                ..
            } = &spot.state
            else {
                continue;
            };
            if let Some((row, skip, shown)) = visible(*first_row, *rows) {
                out.push(Candidate {
                    item: Item::Image(index),
                    content: Content::Image(Arc::clone(image)),
                    column: *column,
                    row,
                    width: *columns,
                    height: *rows,
                    skip,
                    shown,
                });
            }
        }
        out
    }

    // ---- the screen --------------------------------------------------------------------------------------------

    /// The screen, with no formula drawn as an image.
    #[must_use]
    pub fn screen(&self) -> Screen {
        self.screen_with(&HashSet::new())
    }

    /// The screen, with the text of the formulas in `drawn` blanked: a picture is drawn over them. (The rows
    /// of images are empty already.)
    #[must_use]
    pub fn screen_with(&self, drawn: &HashSet<Item>) -> Screen {
        let rows = match &self.mode {
            Mode::Toc(selected) => self.list_rows(
                "Contents",
                &self
                    .rendered
                    .headings
                    .iter()
                    .map(|h| {
                        format!(
                            "{}{}",
                            "  ".repeat(usize::from(h.level.saturating_sub(1))),
                            h.text
                        )
                    })
                    .collect::<Vec<_>>(),
                *selected,
            ),
            Mode::Footnotes(selected) => self.list_rows(
                "Footnotes",
                &self
                    .rendered
                    .footnotes
                    .iter()
                    .map(|f| {
                        let text = self
                            .rendered
                            .lines
                            .get(f.definition_line)
                            .map(Line::text)
                            .unwrap_or_default();
                        let text = text.trim();
                        let prefix = format!("{}.", f.number);
                        let body = text.strip_prefix(&prefix).unwrap_or(text).trim();
                        format!("{}. {}", f.number, body.trim_end_matches('↩').trim_end())
                    })
                    .collect::<Vec<_>>(),
                *selected,
            ),
            Mode::Nav { filter, selected } => {
                let entries = self.nav_entries(filter);
                let labels: Vec<String> = entries
                    .iter()
                    .map(|&(p, depth)| {
                        let label = self
                            .book
                            .as_ref()
                            .and_then(|b| b.pages().get(p))
                            .map_or("", |page| page.label());
                        let current = if Doc::Book(p) == self.doc { " ◂" } else { "" };
                        format!("{}{label}{current}", "  ".repeat(depth))
                    })
                    .collect();
                let title = if filter.is_empty() {
                    "Navigation (type to filter)".to_string()
                } else {
                    format!("Navigation: {filter}")
                };
                let mut rows = self.list_rows(&title, &labels, *selected);
                if labels.is_empty()
                    && let Some(row) = rows.get_mut(1)
                {
                    *row = Row::plain("  no page matches", Mark::None);
                }
                rows
            }
            _ => self.page_rows(drawn),
        };
        Screen {
            rows,
            status: self.status(),
        }
    }

    fn page_rows(&self, drawn: &HashSet<Item>) -> Vec<Row> {
        let content = self.content_rows();
        let mut rows = Vec::with_capacity(content);
        for line_no in self.top..self.top + content {
            // Past the end of the page (a short page, or a jump to a heading near the end): empty rows.
            let Some(line) = self.rendered.lines.get(line_no) else {
                rows.push(Row::default());
                continue;
            };
            rows.push(self.page_row(line_no, line, drawn));
        }
        rows
    }

    /// One line of the page with its marks, and the cells under formula images blanked.
    fn page_row(&self, line_no: usize, line: &Line, drawn: &HashSet<Item>) -> Row {
        let mut ranges: Vec<(usize, usize, Mark)> = Vec::new();
        if let Some(search) = &self.search {
            for (k, &(l, s, e)) in search.matches.iter().enumerate() {
                if l == line_no {
                    let mark = if search.current == Some(k) {
                        Mark::Current
                    } else {
                        Mark::Match
                    };
                    ranges.push((s, e, mark));
                }
            }
        }
        if let Some(spot) = self.focused()
            && spot.line == line_no
        {
            ranges.push((spot.start, spot.end, Mark::Focus));
        }
        let mut blank: Vec<(usize, usize)> = Vec::new();
        for item in drawn {
            let Item::Formula(i) = *item else { continue };
            let Some(f) = self.rendered.formulas.get(i) else {
                continue;
            };
            if line_no >= f.line && line_no < f.line + f.height.max(1) {
                if f.display {
                    blank.push((0, usize::MAX));
                } else if line_no == f.line {
                    blank.push((f.column, f.column + f.width));
                }
            }
        }
        let mut row = Row::default();
        let mut index = 0;
        let mut cell = 0;
        let mut buffer = [0u8; 4];
        for span in &line.spans {
            for c in span.text.chars() {
                let width = char_width(c);
                let blanked = blank.iter().any(|&(s, e)| cell >= s && cell < e);
                let mark = ranges
                    .iter()
                    .rev()
                    .find(|&&(s, e, _)| index >= s && index < e)
                    .map_or(Mark::None, |&(_, _, m)| m);
                if blanked {
                    row.push(&" ".repeat(width), Style::default(), Mark::None);
                } else {
                    row.push(c.encode_utf8(&mut buffer), span.style, mark);
                }
                index += 1;
                cell += width;
            }
        }
        row
    }

    fn list_rows(&self, title: &str, items: &[String], selected: usize) -> Vec<Row> {
        let content = self.content_rows();
        let mut rows = vec![Row::plain(title, Mark::None)];
        if let Some(first) = rows.first_mut()
            && let Some(piece) = first.pieces.first_mut()
        {
            piece.style.bold = true;
        }
        let visible = content.saturating_sub(1).max(1);
        let start = (selected + 1).saturating_sub(visible);
        for (i, item) in items.iter().enumerate().skip(start).take(visible) {
            let mark = if i == selected {
                Mark::Selected
            } else {
                Mark::None
            };
            rows.push(Row::plain(&format!("  {item}"), mark));
        }
        while rows.len() < content {
            rows.push(Row::default());
        }
        rows.truncate(content);
        rows
    }

    /// What the status line says of the focused link when it is an image.
    fn focused_image(&self) -> Option<String> {
        let spot = self.focused()?;
        let image = self.images.iter().find(|i| i.link == spot.link)?;
        Some(match &image.state {
            ImageState::Shown { path, image, .. } => {
                format!(
                    "image {} ({}×{} px)",
                    path.display(),
                    image.width,
                    image.height
                )
            }
            ImageState::Text(why) => format!("image not shown: {why}"),
        })
    }

    /// Handles a signal from outside.
    pub fn signal(&mut self, signal: Signal) -> Option<Effect> {
        match signal {
            Signal::Interrupt => Some(Effect::Exit(130)),
            Signal::Terminate => Some(Effect::Exit(143)),
            Signal::Stop => Some(Effect::Suspend),
            Signal::Continue => Some(Effect::Redraw),
            Signal::Resize(columns, rows) => {
                self.resize(columns, rows);
                None
            }
        }
    }

    /// The status line: the document (or the book's breadcrumb), the position, and a message or warning.
    #[must_use]
    pub fn status(&self) -> String {
        match &self.mode {
            Mode::Search(query) => return format!("/{query}"),
            Mode::Confirm(target) => return format!("open {target} ? (y/n)"),
            _ => {}
        }
        let name = match (&self.doc, &self.book) {
            (Doc::File { name, .. }, _) => name.clone(),
            (Doc::Book(page), Some(book)) => {
                let mut trail: Vec<&str> = Vec::new();
                if let Some(title) = book.title() {
                    trail.push(title);
                }
                for p in book.nav().trail(*page) {
                    if let Some(page) = book.pages().get(p) {
                        trail.push(page.label());
                    }
                }
                trail.join(" › ")
            }
            (Doc::Book(_), None) => String::new(),
        };
        let total = self.rendered.lines.len();
        let bottom = (self.top + self.content_rows()).min(total);
        let percent = (bottom * 100).checked_div(total).unwrap_or(100);
        let mut status = format!(
            "{name}  {}-{bottom}/{total} {percent}%",
            (self.top + 1).min(total.max(1))
        );
        if let Some(message) = &self.message {
            let _ = write!(status, "  {message}");
        } else if let Some(about) = self.focused_image() {
            let _ = write!(status, "  {about}");
        } else if let Some(warning) = &self.warning {
            let _ = write!(status, "  ! {warning}");
        }
        status
    }
}

#[derive(Debug, Clone, Copy)]
enum Anchor {
    Heading(usize, usize),
    Fraction(f64),
}

enum ListKey {
    Moved(usize),
    Choose,
    Close,
    Other(Key),
}

fn list_move(key: Key, selected: usize, count: usize) -> ListKey {
    let last = count.saturating_sub(1);
    match key {
        Key::Char('j') | Key::Down | Key::Tab => ListKey::Moved((selected + 1).min(last)),
        Key::Char('k') | Key::Up | Key::BackTab => ListKey::Moved(selected.saturating_sub(1)),
        Key::PageDown | Key::Char(' ') => ListKey::Moved((selected + 10).min(last)),
        Key::PageUp | Key::Char('b') => ListKey::Moved(selected.saturating_sub(10)),
        Key::Char('g') | Key::Home => ListKey::Moved(0),
        Key::Char('G') | Key::End => ListKey::Moved(last),
        Key::Enter => ListKey::Choose,
        Key::Esc => ListKey::Close,
        other => ListKey::Other(other),
    }
}

/// Where the links are: one spot per run of characters of one link on one line.
fn link_spots(rendered: &Rendered) -> Vec<LinkSpot> {
    let mut spots: Vec<LinkSpot> = Vec::new();
    for (line_no, line) in rendered.lines.iter().enumerate() {
        let mut index = 0;
        for span in &line.spans {
            let len = span.text.chars().count();
            if let Some(link) = span.style.link {
                match spots.last_mut() {
                    Some(last)
                        if last.line == line_no && last.link == link && last.end == index =>
                    {
                        last.end = index + len;
                    }
                    _ => spots.push(LinkSpot {
                        line: line_no,
                        start: index,
                        end: index + len,
                        link,
                    }),
                }
            }
            index += len;
        }
    }
    spots
}

/// Every match of `query` in the page's text: (line, first char, end char). Case is ignored unless the query
/// has an upper-case letter.
fn find_matches(rendered: &Rendered, query: &str) -> Vec<(usize, usize, usize)> {
    let fold = !query.chars().any(char::is_uppercase);
    let normal = |c: char| {
        if fold {
            c.to_lowercase().next().unwrap_or(c)
        } else {
            c
        }
    };
    let needle: Vec<char> = query.chars().map(normal).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (line_no, line) in rendered.lines.iter().enumerate() {
        let hay: Vec<char> = line.text().chars().map(normal).collect();
        let mut at = 0;
        while at + needle.len() <= hay.len() {
            if hay[at..at + needle.len()] == needle[..] {
                out.push((line_no, at, at + needle.len()));
                at += needle.len();
            } else {
                at += 1;
            }
        }
    }
    out
}

/// The text inside a code block's frame on `line`: the spans between the frame's two sides (`Role::Rule`
/// spans around `Role::Code` ones), whatever quote bars, list indents or callout bars come before. `None` for a
/// line that is not inside a code block.
fn code_text(line: &Line) -> Option<String> {
    let spans = &line.spans;
    // A long code line goes on after a `↪ ` mark, drawn as a rule.
    let continuation =
        |s: &quench_markdown_term::Span| s.style.role == Role::Rule && s.text.trim() == "↪";
    let code = spans.iter().position(|s| s.style.role == Role::Code)?;
    let left = spans[..code]
        .iter()
        .rposition(|s| s.style.role == Role::Rule && !continuation(s))?;
    let right = code
        + spans[code..]
            .iter()
            .position(|s| s.style.role == Role::Rule)?;
    // Only what lies between the frame's sides: the continuation mark at most, between the left side and the
    // code.
    if !spans[left + 1..code].iter().all(continuation) {
        return None;
    }
    Some(
        spans[left + 1..right]
            .iter()
            .map(|s| s.text.as_str())
            .collect(),
    )
}

/// The code blocks of the page: (first line, last line) of each run of lines inside a frame, between a line
/// with the frame's top (`╭`) and one with its bottom (`╰`). A table's borders are rules too, but not those.
fn code_blocks(lines: &[Line]) -> Vec<(usize, usize)> {
    let has = |i: Option<usize>, corner: char| {
        i.and_then(|i| lines.get(i)).is_some_and(|l| {
            l.spans
                .iter()
                .any(|s| s.style.role == Role::Rule && s.text.contains(corner))
        })
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut open: Option<usize> = None;
    for i in 0..=lines.len() {
        if i < lines.len() && code_text(&lines[i]).is_some() {
            open.get_or_insert(i);
        } else if let Some(start) = open.take()
            && has(start.checked_sub(1), '╭')
            && has(Some(i), '╰')
        {
            out.push((start, i - 1));
        }
    }
    out
}

/// The cells taken by the first `chars` characters of `line`.
fn cells_before(line: &Line, chars: usize) -> usize {
    line.text().chars().take(chars).map(char_width).sum()
}

/// The cells an image takes: scaled down (never up) to at most `max_columns` cells across and `max_rows` rows.
#[must_use]
pub fn image_cells(
    image: &Image,
    (cw, ch): (u32, u32),
    max_columns: usize,
    max_rows: usize,
) -> (usize, usize) {
    let (cw, ch) = (f64::from(cw.max(1)), f64::from(ch.max(1)));
    let (w, h) = (
        f64::from(image.width.max(1)),
        f64::from(image.height.max(1)),
    );
    #[allow(clippy::cast_precision_loss)]
    let scale = (max_columns as f64 * cw / w)
        .min(max_rows as f64 * ch / h)
        .min(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cells = |pixels: f64, cell: f64| ((pixels * scale / cell).ceil() as usize).max(1);
    (cells(w, cw).min(max_columns), cells(h, ch).min(max_rows))
}

/// Inserts `count` empty lines at `at`, moving every spot at or below it down.
fn insert_blank(rendered: &mut Rendered, at: usize, count: usize) {
    if count == 0 {
        return;
    }
    let at = at.min(rendered.lines.len());
    rendered
        .lines
        .splice(at..at, std::iter::repeat_with(Line::default).take(count));
    let shift = |line: &mut usize| {
        if *line >= at {
            *line += count;
        }
    };
    for h in &mut rendered.headings {
        shift(&mut h.line);
    }
    for f in &mut rendered.footnotes {
        shift(&mut f.definition_line);
        f.reference_lines.iter_mut().for_each(shift);
    }
    for f in &mut rendered.formulas {
        shift(&mut f.line);
    }
}
