//! What is on the screen, and what a key press does to it. No terminal and no network, so it is tested with
//! synthetic keys.

use gauge_core::{AlertEvent, DiskForecast, HostCard, HostDetail};
use quench_cli::keys::{KeyCode, KeyEvent};

/// The pages, in the order Tab walks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Hosts,
    Fleet,
    Storage,
    Gpus,
    Network,
    Power,
    Alerts,
}

impl Screen {
    pub const ALL: [Self; 7] = [
        Self::Hosts,
        Self::Fleet,
        Self::Storage,
        Self::Gpus,
        Self::Network,
        Self::Power,
        Self::Alerts,
    ];

    /// The pages that list hosts to select from (the others scroll).
    #[must_use]
    pub const fn lists_hosts(self) -> bool {
        matches!(self, Self::Hosts | Self::Fleet)
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Hosts => "Hosts",
            Self::Fleet => "Fleet",
            Self::Storage => "Storage",
            Self::Gpus => "GPUs",
            Self::Network => "Network",
            Self::Power => "Power",
            Self::Alerts => "Alerts",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        let at = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(at + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// One answer of the service.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub hosts: Vec<HostCard>,
    pub forecast: Vec<DiskForecast>,
    pub alerts: Vec<AlertEvent>,
    /// The open host with its live history; `None` while no host is open.
    pub detail: Option<HostDetail>,
}

/// The monitor's state.
#[derive(Debug)]
pub struct App {
    pub screen: Screen,
    /// The row of the host list.
    pub selected: usize,
    /// The host whose page is open; kept by name, since the list can change between two fetches.
    pub detail: Option<String>,
    /// First line shown of a scrolling page.
    pub scroll: usize,
    pub data: Data,
    /// Unix seconds when `data` arrived; `None` before the first answer.
    pub fetched_at: Option<f64>,
    /// Why the last fetch failed; the old data stays on screen.
    pub error: Option<String>,
    pub loading: bool,
    /// Rows of the open page that fit, as the last draw saw them: what Page Up and Page Down move by.
    pub page_rows: usize,
    /// Cards per row of the host grid, as the last draw laid it out: what Up and Down move by.
    pub grid_cols: usize,
    /// Lines of the open scrolling page, as the last draw saw them.
    pub page_len: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            screen: Screen::Hosts,
            selected: 0,
            detail: None,
            scroll: 0,
            data: Data {
                hosts: Vec::new(),
                forecast: Vec::new(),
                alerts: Vec::new(),
                detail: None,
            },
            fetched_at: None,
            error: None,
            loading: false,
            page_rows: 10,
            grid_cols: 1,
            page_len: 0,
        }
    }

    /// Takes what a fetch brought. A failed fetch keeps the numbers and says why.
    pub fn apply(&mut self, result: Result<Data, String>, now: f64) {
        self.loading = false;
        match result {
            Ok(mut data) => {
                data.hosts.sort_by(|a, b| a.host.name.cmp(&b.host.name));
                self.data = data;
                self.fetched_at = Some(now);
                self.error = None;
                self.selected = self.selected.min(self.data.hosts.len().saturating_sub(1));
            }
            Err(why) => self.error = Some(why),
        }
    }

    /// The host whose page is open, if it is still in the list.
    #[must_use]
    pub fn open_host(&self) -> Option<&HostCard> {
        let name = self.detail.as_deref()?;
        self.data.hosts.iter().find(|h| h.host.name == name)
    }

    /// Whether the open page scrolls (every page but the host list).
    #[must_use]
    pub const fn scrolls(&self) -> bool {
        self.detail.is_some() || !self.screen.lists_hosts()
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.detail = None;
        self.scroll = 0;
    }

    /// Moves by `delta` rows: the selection of the host list, or the scroll of a page. Both are clamped by the next
    /// draw, which knows how long the page is.
    fn step(&mut self, delta: isize) {
        if self.scrolls() {
            self.scroll = self.scroll.saturating_add_signed(delta);
        } else {
            self.selected = self
                .selected
                .saturating_add_signed(delta)
                .min(self.data.hosts.len().saturating_sub(1));
        }
    }

    const fn jump(&mut self, to_end: bool) {
        if self.scrolls() {
            self.scroll = if to_end { usize::MAX } else { 0 };
        } else {
            self.selected = if to_end {
                self.data.hosts.len().saturating_sub(1)
            } else {
                0
            };
        }
    }
}

/// What the loop does after a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Quit,
    /// Fetch now instead of waiting for the next tick.
    Refresh,
    /// The screen changed: draw it.
    Redraw,
    Nothing,
}

/// Applies a key press.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Effect {
    let page = isize::try_from(app.page_rows.max(1)).unwrap_or(isize::MAX);
    // The host cards are a grid: left and right move along a row, up and down by a row.
    let grid = app.detail.is_none() && app.screen == Screen::Hosts;
    let row = if grid {
        isize::try_from(app.grid_cols.max(1)).unwrap_or(1)
    } else {
        1
    };
    match key.code {
        KeyCode::Char('c') if key.modifiers.ctrl => return Effect::Quit,
        KeyCode::Char('q') => return Effect::Quit,
        KeyCode::Char('r') => return Effect::Refresh,
        KeyCode::Up | KeyCode::Char('k') => app.step(-row),
        KeyCode::Down | KeyCode::Char('j') => app.step(row),
        KeyCode::Left | KeyCode::Char('h') if grid => app.step(-1),
        KeyCode::Right | KeyCode::Char('l') if grid => app.step(1),
        KeyCode::PageUp => app.step(-page),
        KeyCode::PageDown | KeyCode::Char(' ') => app.step(page),
        KeyCode::Home | KeyCode::Char('g') => app.jump(false),
        KeyCode::End | KeyCode::Char('G') => app.jump(true),
        KeyCode::Tab => app.go(app.screen.next()),
        KeyCode::BackTab => app.go(app.screen.previous()),
        KeyCode::Char(c @ '1'..='7') => {
            if let Some(screen) = c.to_digit(10).and_then(|d| Screen::ALL.get(d as usize - 1)) {
                app.go(*screen);
            }
        }
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l')
            if app.detail.is_none()
                && app.screen.lists_hosts()
                && (!grid || key.code == KeyCode::Enter) =>
        {
            let Some(host) = app.data.hosts.get(app.selected) else {
                return Effect::Nothing;
            };
            app.detail = Some(host.host.name.clone());
            app.scroll = 0;
            // Its history comes with the next fetch: ask for it now.
            return Effect::Refresh;
        }
        KeyCode::Esc | KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h')
            if app.detail.is_some() =>
        {
            app.detail = None;
            app.scroll = 0;
        }
        _ => return Effect::Nothing,
    }
    Effect::Redraw
}
