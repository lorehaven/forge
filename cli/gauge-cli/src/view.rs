//! The screens, drawn like `forge-toolbox`: a page background, a header, cards, badges, key chips and an alert as
//! the status line. Pure string building (a [`Line`] per row), so it is tested without a terminal.

use crate::REFRESH_SECS;
use crate::pages;
use crate::state::{App, Data, Screen};
use crate::tiles;
use forge_tui::widgets::{self, ButtonKind};
use forge_tui::{ColorLevel, Line, Rgb, Style, Theme, Tone, fill_screen};
use gauge_core::format::{ago, bytes, duration, pct, rate};
use gauge_core::{AlertEvent, HostCard, Level, View};

/// The widest the content grows; wider terminals centre it.
const CONTENT_MAX: usize = 120;
/// Rows of the page that are not card body: header, tabs, card chrome, hints and the status line.
const CHROME_ROWS: usize = 14;
/// The shortest card area that still leaves room for the tiles above it.
const TILES_MIN_CAPACITY: usize = 16;
/// Spinner frames of a fetch in flight.
const SPINNER: [&str; 4] = ["⠋", "⠹", "⠼", "⠧"];

/// Renders the screen as one string per terminal row. `now` is unix seconds, `tick` turns the spinner.
#[must_use]
pub fn render(
    app: &mut App,
    now: f64,
    tick: usize,
    term_width: usize,
    term_height: usize,
    theme: &Theme,
    level: ColorLevel,
) -> Vec<String> {
    let page = page_lines(app, now, tick, term_width, term_height, theme);
    let rows = if level == ColorLevel::None {
        page
    } else {
        fill_screen(page, term_width, term_height, theme.bg)
    };
    rows.iter().map(|line| line.render(level)).collect()
}

fn page_lines(
    app: &mut App,
    now: f64,
    tick: usize,
    term_width: usize,
    term_height: usize,
    theme: &Theme,
) -> Vec<Line> {
    let margin = if term_width >= 60 { 2 } else { 1 };
    let content = term_width.saturating_sub(margin * 2).clamp(1, CONTENT_MAX);
    let margin = margin.max(term_width.saturating_sub(content) / 2);
    let indent = |line: Line| Line::new().space(margin, Style::new()).append(line);

    let text = Style::new().fg(theme.text);
    let tertiary = Style::new().fg(theme.text_tertiary);
    let capacity = if term_height == 0 {
        usize::MAX
    } else {
        term_height.saturating_sub(CHROME_ROWS).max(3)
    };
    app.page_rows = if capacity == usize::MAX { 10 } else { capacity };

    let mut page = vec![Line::new()];
    page.push(indent(
        Line::styled("Gauge", text.bold())
            .space(2, Style::new())
            .append(widgets::tag(
                theme,
                Tone::Brand,
                concat!("v", env!("CARGO_PKG_VERSION")),
            )),
    ));
    page.push(indent(
        Line::styled(summary(&app.data), tertiary).truncate(content),
    ));
    page.push(Line::new());
    page.push(indent(tabs(app, theme).truncate(content)));
    page.push(Line::new());

    // Stat tiles when the screen is tall enough for them and the list.
    let tiles_wanted = (app.screen.lists_hosts() || app.detail.is_some())
        && (capacity == usize::MAX || capacity >= TILES_MIN_CAPACITY);
    let tile_rows = if tiles_wanted {
        let list = if let Some(host) = app.open_host() {
            tiles::host_tiles(host, app.data.detail.as_ref())
        } else if app.detail.is_some() {
            Vec::new()
        } else {
            tiles::fleet_tiles(&app.data)
        };
        tiles::row(theme, &list, content)
    } else {
        Vec::new()
    };
    let capacity = if tile_rows.is_empty() || capacity == usize::MAX {
        capacity
    } else {
        capacity.saturating_sub(tiles::TILE_ROWS + 1).max(3)
    };
    app.page_rows = if capacity == usize::MAX { 10 } else { capacity };
    if !tile_rows.is_empty() {
        page.extend(tile_rows.into_iter().map(&indent));
        page.push(Line::new());
    }

    // The pages other than the fleet table have no frame of their own: they are cards, so they get the rows the
    // frame's four lines of chrome would have used.
    let free = if capacity == usize::MAX {
        capacity
    } else {
        capacity + 4
    };
    let inner = widgets::card_inner(column_width(content));
    let card = if let Some(host) = app.open_host().cloned() {
        let lines = host_lines(&host, now, theme, inner);
        scrolling_cards(app, theme, lines, content, free)
    } else if app.detail.is_some() {
        let lines = vec![
            section(theme, "Host"),
            Line::styled("  This host is no longer listed.", tertiary),
        ];
        scrolling_cards(app, theme, lines, content, free)
    } else {
        match app.screen {
            Screen::Hosts => hosts_with_alerts(app, now, theme, content, capacity),
            Screen::Fleet => hosts_card(app, now, theme, content, capacity),
            Screen::Gpus => {
                let lines = pages::gpu_lines(&app.data, theme);
                scrolling_cards(app, theme, lines, content, free)
            }
            Screen::Network => {
                let lines = pages::network_page(&app.data, theme, inner);
                scrolling_cards(app, theme, lines, content, free)
            }
            Screen::Power => {
                let lines = pages::power_lines(&app.data, theme);
                scrolling_cards(app, theme, lines, content, free)
            }
            Screen::Alerts => {
                let lines = alert_lines(&app.data, now, theme, inner);
                scrolling_cards(app, theme, lines, content, free)
            }
            Screen::Storage => {
                let lines = storage_lines(&app.data, theme, inner);
                scrolling_cards(app, theme, lines, content, free)
            }
        }
    };
    page.extend(card.into_iter().map(&indent));
    page.push(Line::new());
    page.push(indent(hints(app, theme).truncate(content)));
    page.push(Line::new());
    page.push(indent(status_line(app, now, tick, theme, content)));
    page
}

/// "3 hosts · 2 up · 1 alert" under the title.
fn summary(data: &Data) -> String {
    let total = data.hosts.len();
    let up = data.hosts.iter().filter(|h| h.host.up).count();
    let alerts = current_alerts(data).len();
    if total == 0 {
        return "Host metrics for the estate.".to_string();
    }
    format!(
        "{total} {} · {up} up · {alerts} {}",
        plural(total, "host", "hosts"),
        plural(alerts, "alert", "alerts")
    )
}

pub(crate) const fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// The page tabs: the open one is a solid button, the others raised ones, each with its number key.
fn tabs(app: &App, theme: &Theme) -> Line {
    let mut line = Line::new();
    for (n, screen) in Screen::ALL.iter().enumerate() {
        let kind = if *screen == app.screen {
            ButtonKind::Primary
        } else {
            ButtonKind::Secondary
        };
        line = line
            .append(widgets::button(
                theme,
                kind,
                &format!("{} {}", n + 1, screen.title()),
            ))
            .space(1, Style::new());
    }
    line
}

fn hints(app: &App, theme: &Theme) -> Line {
    let gap = Style::new();
    let mut items: Vec<(&str, &str)> = Vec::new();
    if app.detail.is_some() {
        items.push(("↑↓", "Scroll"));
        items.push(("Esc", "Back"));
    } else if app.screen == Screen::Hosts {
        items.push(("←↑↓→", "Select"));
        items.push(("↵", "Open"));
    } else if app.screen.lists_hosts() {
        items.push(("↑↓", "Select"));
        items.push(("↵", "Open"));
    } else {
        items.push(("↑↓", "Scroll"));
    }
    items.extend([("Tab", "Next page"), ("r", "Refresh"), ("q", "Quit")]);
    let mut line = Line::new();
    for (n, (key, label)) in items.iter().enumerate() {
        if n > 0 {
            line = line.space(3, gap);
        }
        line = line.append(widgets::hint(theme, key, label));
    }
    if app.scrolls() && app.page_len > app.page_rows {
        line = line.push(
            format!(
                "   {}–{} of {}",
                app.scroll + 1,
                (app.scroll + app.page_rows).min(app.page_len),
                app.page_len
            ),
            Style::new().fg(theme.text_tertiary),
        );
    }
    line
}

fn status_line(app: &App, now: f64, tick: usize, theme: &Theme, width: usize) -> Line {
    if let Some(why) = &app.error {
        return widgets::alert(theme, Tone::Error, why, width);
    }
    let Some(at) = app.fetched_at else {
        let frame = SPINNER[tick % SPINNER.len()];
        return widgets::alert_with_icon(theme, Tone::Brand, frame, "Connecting…", width);
    };
    let age = ago((now - at).max(0.0));
    let message = format!("Updated {age} · refreshes every {REFRESH_SECS} s");
    if app.loading {
        let frame = SPINNER[tick % SPINNER.len()];
        widgets::alert_with_icon(theme, Tone::Brand, frame, &message, width)
    } else {
        widgets::alert(theme, Tone::Success, &message, width)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Building blocks.

pub(crate) fn tone_color(theme: &Theme, tone: Tone) -> Rgb {
    theme.soft(tone).text
}

pub(crate) const fn level_tone(level: Level) -> Tone {
    match level {
        Level::Ok => Tone::Success,
        Level::Warn => Tone::Warning,
        Level::Crit => Tone::Error,
    }
}

/// A bar `width` cells wide in `color`, to the eighth of a cell: whole blocks, one partial block, then the track.
pub(crate) fn bar(theme: &Theme, value: Option<f64>, color: Rgb, width: usize, bg: Rgb) -> Line {
    const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let base = Style::new().bg(bg);
    let track = base.fg(theme.border_strong);
    let Some(value) = value else {
        return Line::styled("░".repeat(width), track);
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "clamped to 0..=100 and scaled to a bar of a few dozen cells"
    )]
    let eighths =
        ((value.clamp(0.0, 100.0) / 100.0 * width as f64 * 8.0).round() as usize).min(width * 8);
    let (full, part) = (eighths / 8, eighths % 8);
    let used = full + usize::from(part > 0);
    Line::styled("█".repeat(full), base.fg(color))
        .push(EIGHTHS[part], base.fg(color))
        .push("░".repeat(width - used), track)
}

/// A [`bar`] coloured by how full it is, then the percentage in 5 columns.
pub(crate) fn meter(theme: &Theme, value: Option<f64>, width: usize, bg: Rgb) -> Line {
    let base = Style::new().bg(bg);
    let Some(value) = value else {
        return Line::styled("─".repeat(width), base.fg(theme.text_disabled))
            .push("    —", base.fg(theme.text_disabled));
    };
    let clamped = value.clamp(0.0, 100.0);
    let color = tone_color(theme, level_tone(Level::of(clamped)));
    bar(theme, Some(clamped), color, width, bg).push(
        format!("{:>5}", pct(clamped)),
        base.fg(theme.text_secondary),
    )
}

pub(crate) fn badge_for(theme: &Theme, host: &HostCard) -> Line {
    let alerts = host.host.view.as_ref().map_or(&[][..], |v| &v.alerts[..]);
    if !host.host.up {
        widgets::badge(theme, Tone::Error, "down")
    } else if alerts.iter().any(|a| a.severe) {
        widgets::badge(theme, Tone::Error, &alert_count(alerts.len()))
    } else if !alerts.is_empty() {
        widgets::badge(theme, Tone::Warning, &alert_count(alerts.len()))
    } else {
        widgets::badge(theme, Tone::Success, "ok")
    }
}

fn alert_count(n: usize) -> String {
    format!("{n} {}", plural(n, "alert", "alerts"))
}

pub(crate) fn mem_pct(view: &View) -> Option<f64> {
    let (used, total) = (view.mem_used?, view.mem_total?);
    (total > 0.0).then(|| used / total * 100.0)
}

pub(crate) fn gpu_pct(view: &View) -> Option<f64> {
    view.gpus
        .iter()
        .filter_map(|g| g.busy_pct)
        .fold(None, |best, v| Some(best.map_or(v, |b: f64| b.max(v))))
}

pub(crate) fn fullest_disk(view: &View) -> Option<f64> {
    view.filesystems
        .iter()
        .filter(|f| f.size > 0.0)
        .map(|f| (f.size - f.avail) / f.size * 100.0)
        .fold(None, |best, v| Some(best.map_or(v, |b: f64| b.max(v))))
}

/// The hottest sensor or GPU, in degrees.
pub(crate) fn hottest(view: &View) -> Option<f64> {
    view.temps
        .iter()
        .map(|t| t.celsius)
        .chain(view.gpus.iter().filter_map(|g| g.temp_c))
        .fold(None, |best, v| Some(best.map_or(v, |b: f64| b.max(v))))
}

pub(crate) fn net_totals(view: &View) -> (f64, f64) {
    view.nics.iter().fold((0.0, 0.0), |(rx, tx), n| {
        (rx + n.a_per_sec, tx + n.b_per_sec)
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Hosts.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Host,
    Status,
    Cpu,
    Mem,
    Disk,
    Gpu,
    Pressure,
    Hottest,
    Alerts,
    Trend,
}

/// A column: what it is, its header, its width with gutter and when it is dropped (higher goes first).
struct Spec {
    col: Col,
    head: &'static str,
    width: usize,
    drop: u8,
}

const MARKER: usize = 2;
/// Cells of a meter's bar in the fleet table, and its width with the percentage.
const BAR: usize = 5;
const METER: usize = BAR + 5;

fn specs(hosts: &[HostCard], inner: usize) -> Vec<Spec> {
    let name = hosts
        .iter()
        .map(|h| h.host.name.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let spec = |col, head, width, drop| Spec {
        col,
        head,
        width,
        drop,
    };
    let mut all = vec![
        spec(Col::Host, "Host", name + 2, 0),
        spec(Col::Status, "Status", 12, 0),
        spec(Col::Cpu, "CPU", METER + 2, 1),
        spec(Col::Trend, "CPU 1h", 12, 8),
        spec(Col::Mem, "Memory", METER + 2, 2),
        spec(Col::Pressure, "Pressure", METER + 2, 3),
        spec(Col::Disk, "Disk", METER + 2, 4),
        spec(Col::Gpu, "GPU", METER + 2, 5),
        spec(Col::Hottest, "Hottest", 8, 6),
        spec(Col::Alerts, "Alerts", 7, 7),
    ];
    let total = |cols: &[Spec]| MARKER + cols.iter().map(|c| c.width).sum::<usize>();
    while total(&all) > inner {
        let Some(worst) = all.iter().map(|c| c.drop).max().filter(|d| *d > 0) else {
            break;
        };
        all.retain(|c| c.drop != worst);
    }
    all
}

fn cell(line: Line, width: usize, bg: Style) -> Line {
    line.truncate(width.saturating_sub(1)).pad_to(width, bg)
}

fn hosts_card(app: &mut App, now: f64, theme: &Theme, width: usize, capacity: usize) -> Vec<Line> {
    let inner = widgets::card_inner(width);
    let hosts = &app.data.hosts;
    let total = hosts.len();
    let cap = capacity.saturating_sub(1).max(1);
    app.selected = app.selected.min(total.saturating_sub(1));
    let start = (app.selected + 1).saturating_sub(cap);
    let end = (start + cap).min(total);

    let count = if end - start < total {
        format!("  {}–{} of {total}", start + 1, end)
    } else {
        format!("  {total} {}", plural(total, "host", "hosts"))
    };
    let header = Line::styled(" Fleet", Style::new().fg(theme.text).bold())
        .push(count, Style::new().fg(theme.text_tertiary));

    let cols = specs(hosts, inner);
    let head_bg = Style::new().bg(theme.surface_2);
    let head_style = head_bg.fg(theme.text_tertiary).bold();
    let mut head = Line::new().space(MARKER, head_bg);
    for c in &cols {
        head = head.append(cell(Line::styled(c.head, head_style), c.width, head_bg));
    }
    let mut body = vec![head.pad_to(inner, head_bg)];
    if hosts.is_empty() {
        let note = if app.fetched_at.is_some() {
            "  No hosts are configured."
        } else {
            "  Waiting for the first answer…"
        };
        body.push(Line::styled(note, Style::new().fg(theme.text_tertiary)));
    }
    for (index, host) in hosts.iter().enumerate().take(end).skip(start) {
        body.push(host_row(
            theme,
            &cols,
            host,
            index == app.selected,
            now,
            inner,
        ));
        if let (false, Some(why)) = (host.host.up, &host.host.error) {
            let soft = theme.soft(Tone::Error);
            body.push(
                Line::styled(
                    format!("  {} {why}", widgets::alert_icon(Tone::Error)),
                    Style::new().fg(soft.text),
                )
                .truncate(inner),
            );
        }
    }
    widgets::card(theme, Some(header), body, width)
}

/// How many alerts the host page lists before pointing at the Alerts page.
const ACTIVE_ALERTS_MAX: usize = 5;

/// The host list with the alerts that are open now under it.
fn hosts_with_alerts(
    app: &mut App,
    now: f64,
    theme: &Theme,
    width: usize,
    capacity: usize,
) -> Vec<Line> {
    let current = current_alerts(&app.data);
    let shown = current.len().min(ACTIVE_ALERTS_MAX);
    let body = shown.max(1) + usize::from(current.len() > shown);
    // The alerts card (its body, four rows of chrome) and the blank row above it. The host grid has no frame, so it
    // gets the rows the hosts card's chrome would have had.
    let alerts_rows = body + 5;
    let avail = if capacity == usize::MAX {
        capacity
    } else {
        capacity + 4
    };
    let full = grid_height(&app.data.hosts, false);
    let show_alerts = avail == usize::MAX || avail >= alerts_rows + full;
    let grid_avail = if show_alerts && avail != usize::MAX {
        avail - alerts_rows
    } else {
        avail
    };
    let mut lines = host_grid(app, theme, width, grid_avail);
    if show_alerts {
        lines.push(Line::new());
        lines.extend(active_alerts_card(&app.data, &current, now, theme, width));
    }
    lines
}

// ---------------------------------------------------------------------------------------------------------------
// The host cards.

/// The narrowest a host card is drawn (the web grid's `22rem`).
const CARD_MIN: usize = 44;

/// A rounded card of `body` rows, `width` columns wide, with a border of its own colour.
fn box_card(theme: &Theme, body: Vec<Line>, width: usize, border: Rgb) -> Vec<Line> {
    let inner = widgets::card_inner(width);
    let edge = Style::new().fg(border).bg(theme.bg);
    let side = Style::new().fg(border).bg(theme.surface);
    let fill = Style::new().bg(theme.surface);
    let rule = "─".repeat(inner);
    let mut lines = vec![Line::styled(format!("╭{rule}╮"), edge)];
    lines.extend(body.into_iter().map(|line| {
        Line::styled("│", side)
            .append(line.truncate(inner).on(theme.surface).pad_to(inner, fill))
            .push("│", side)
    }));
    lines.push(Line::styled(format!("╰{rule}╯"), edge));
    lines
}

/// The rows inside one host card, `iw` columns wide. `compact` leaves out the line under the name and the chart.
fn host_body(theme: &Theme, host: &HostCard, iw: usize, compact: bool) -> Vec<Line> {
    let h = &host.host;
    let view = h.view.as_ref();
    let surface = theme.surface;
    let base = Style::new().bg(surface);
    let room = iw.saturating_sub(2);
    let pad = |line: Line| Line::new().space(1, base).append(line);

    // Name, alert badge, and whether it answers.
    let (state, tone) = if h.up {
        ("● up", Tone::Success)
    } else {
        ("● unreachable", Tone::Error)
    };
    let state_w = state.chars().count();
    let mut left = Line::styled(
        truncate_text(&h.name, room.saturating_sub(state_w + 1).max(1)),
        base.fg(theme.text).bold(),
    );
    let alerts = view.map_or(&[][..], |v| &v.alerts[..]);
    if !alerts.is_empty() {
        let badge = widgets::badge(
            theme,
            if alerts.iter().any(|a| a.severe) {
                Tone::Error
            } else {
                Tone::Warning
            },
            &alert_count(alerts.len()),
        );
        if left.width() + 2 + badge.width() + 1 + state_w <= room {
            left = left.space(2, base).append(badge);
        }
    }
    let gap = room.saturating_sub(left.width() + state_w).max(1);
    let head = left
        .space(gap, base)
        .push(state, base.fg(tone_color(theme, tone)));
    let mut body = vec![pad(head)];

    if !compact {
        let mut sub = format!("{} · {}", h.kind, h.url);
        if let Some(cores) = view.map(|v| v.cores_pct.len()).filter(|n| *n > 0) {
            sub = format!("{cores} cores · {sub}");
        }
        body.push(pad(Line::styled(sub, base.fg(theme.text_tertiary))));
    }
    if let (false, Some(why)) = (h.up, &h.error) {
        body.push(pad(widgets::alert(
            theme,
            Tone::Error,
            &truncate_text(why, room.saturating_sub(6)),
            room,
        )));
    }

    let bar = room.saturating_sub(8 + 5).max(4);
    let row = |label: &str, value: Option<f64>| {
        pad(
            Line::styled(format!("{label:<8}"), base.fg(theme.text_secondary))
                .append(meter(theme, value, bar, surface)),
        )
    };
    body.push(row("CPU", view.and_then(|v| v.cpu_pct)));
    body.push(row("Memory", view.and_then(mem_pct)));
    if view.is_some_and(|v| !v.gpus.is_empty()) {
        body.push(row("GPU", view.and_then(gpu_pct)));
    }
    body.push(row("Disk", view.and_then(fullest_disk)));

    if !compact {
        body.extend(
            tiles::chart(theme, &host.cpu, room, 2, 100.0, surface)
                .into_iter()
                .map(pad),
        );
    }

    let load = view
        .and_then(|v| v.load)
        .map_or_else(|| "—".to_string(), |l| format!("{:.2}", l[0]));
    let net = view.map_or_else(String::new, |v| {
        let (rx, tx) = net_totals(v);
        format!("↓ {} ↑ {}", rate(rx), rate(tx))
    });
    let gap = room
        .saturating_sub(5 + load.chars().count() + net.chars().count())
        .max(1);
    body.push(pad(Line::styled("Load ", base.fg(theme.text_tertiary))
        .push(load, base.fg(theme.text_secondary))
        .space(gap, base)
        .push(net, base.fg(theme.text_secondary))));
    body
}

/// Rows the tallest host card takes, border included.
fn grid_height(hosts: &[HostCard], compact: bool) -> usize {
    hosts
        .iter()
        .map(|h| host_body(&Theme::dark(), h, CARD_MIN - 2, compact).len() + 2)
        .max()
        .unwrap_or(1)
}

/// One card per host, as many to a row as fit, the selected one with a brand border. Cards are cut to whole rows of
/// cards, the selected row kept in view.
fn host_grid(app: &mut App, theme: &Theme, content: usize, avail: usize) -> Vec<Line> {
    let hosts = &app.data.hosts;
    if hosts.is_empty() {
        app.grid_cols = 1;
        let note = if app.fetched_at.is_some() {
            "No hosts are configured."
        } else {
            "Waiting for the first answer…"
        };
        return vec![Line::styled(note, Style::new().fg(theme.text_tertiary))];
    }
    let cols = ((content + 1) / (CARD_MIN + 1)).clamp(1, 4);
    let cw = (content + 1 - cols) / cols;
    app.grid_cols = cols;
    app.selected = app.selected.min(hosts.len() - 1);

    let full = grid_height(hosts, false);
    let compact = avail != usize::MAX && avail < full;
    let card_h = if compact {
        grid_height(hosts, true)
    } else {
        full
    };
    let rows_total = hosts.len().div_ceil(cols);
    let rows_visible = if avail == usize::MAX {
        rows_total
    } else {
        ((avail + 1) / (card_h + 1)).clamp(1, rows_total)
    };
    let selected_row = app.selected / cols;
    let first = (selected_row + 1).saturating_sub(rows_visible);
    app.page_rows = rows_visible * cols;

    let mut out = Vec::new();
    for row in first..(first + rows_visible).min(rows_total) {
        let start = row * cols;
        let end = (start + cols).min(hosts.len());
        let mut bodies: Vec<Vec<Line>> = hosts[start..end]
            .iter()
            .map(|h| host_body(theme, h, cw - 2, compact))
            .collect();
        let height = bodies.iter().map(Vec::len).max().unwrap_or(0);
        let blank = Line::new();
        for body in &mut bodies {
            body.resize(height, blank.clone());
        }
        let boxed: Vec<Vec<Line>> = bodies
            .into_iter()
            .enumerate()
            .map(|(n, body)| {
                let border = if start + n == app.selected {
                    theme.brand_solid
                } else {
                    theme.border
                };
                box_card(theme, body, cw, border)
            })
            .collect();
        if row > first {
            out.push(Line::new());
        }
        for line in 0..height + 2 {
            let mut joined = Line::new();
            for (n, card) in boxed.iter().enumerate() {
                if n > 0 {
                    joined = joined.space(1, Style::new());
                }
                joined = joined.append(card[line].clone());
            }
            out.push(joined);
        }
    }
    out
}

fn active_alerts_card(
    data: &Data,
    current: &[(String, gauge_core::Alert)],
    now: f64,
    theme: &Theme,
    width: usize,
) -> Vec<Line> {
    let inner = widgets::card_inner(width);
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut header = Line::styled(" Active alerts", Style::new().fg(theme.text).bold());
    if !current.is_empty() {
        header = header.space(2, Style::new()).append(widgets::badge(
            theme,
            if current.iter().any(|(_, a)| a.severe) {
                Tone::Error
            } else {
                Tone::Warning
            },
            &current.len().to_string(),
        ));
    }
    let mut body = Vec::new();
    if current.is_empty() {
        body.push(Line::styled(
            "  All clear.",
            Style::new().fg(theme.soft(Tone::Success).text),
        ));
    }
    for (host, a) in current.iter().take(ACTIVE_ALERTS_MAX) {
        body.push(alert_row(theme, data, host, a, now, inner));
    }
    if current.len() > ACTIVE_ALERTS_MAX {
        body.push(Line::styled(
            format!("  and {} more · press 7", current.len() - ACTIVE_ALERTS_MAX),
            tertiary,
        ));
    }
    widgets::card(theme, Some(header), body, width)
}

/// One open alert: its badge, the host, what it says and how long it has been open.
fn alert_row(
    theme: &Theme,
    data: &Data,
    host: &str,
    a: &gauge_core::Alert,
    now: f64,
    inner: usize,
) -> Line {
    let tone = if a.severe { Tone::Error } else { Tone::Warning };
    let open = data
        .alerts
        .iter()
        .find(|e| e.closed.is_none() && e.host == host && e.key == a.key)
        .map(|e| format!("  open {}", ago((now - e.opened).max(0.0))))
        .unwrap_or_default();
    Line::new()
        .space(2, Style::new())
        .append(widgets::badge(theme, tone, &a.kind))
        .push(format!("  {host}"), Style::new().fg(theme.text).bold())
        .push(
            format!("  {}", a.detail),
            Style::new().fg(theme.text_secondary),
        )
        .push(open, Style::new().fg(theme.text_tertiary))
        .truncate(inner)
}

fn host_row(
    theme: &Theme,
    cols: &[Spec],
    host: &HostCard,
    selected: bool,
    _now: f64,
    inner: usize,
) -> Line {
    let row_bg = if selected {
        theme.surface_hover
    } else {
        theme.surface
    };
    let bg = Style::new().bg(row_bg);
    let mut line = if selected {
        Line::styled("▌", bg.fg(theme.brand_solid)).space(1, bg)
    } else {
        Line::new().space(MARKER, bg)
    };
    let view = host.host.view.as_ref();
    let dim = bg.fg(theme.text_disabled);
    let none = || Line::styled("—", dim);
    let name = if selected {
        bg.fg(theme.text).bold()
    } else {
        bg.fg(theme.text)
    };
    for c in cols {
        let content = match c.col {
            Col::Host => Line::styled(&host.host.name, name),
            Col::Status => badge_for(theme, host),
            Col::Cpu => view.map_or_else(none, |v| meter(theme, v.cpu_pct, BAR, row_bg)),
            Col::Mem => view.map_or_else(none, |v| meter(theme, mem_pct(v), BAR, row_bg)),
            Col::Disk => view.map_or_else(none, |v| meter(theme, fullest_disk(v), BAR, row_bg)),
            Col::Gpu => view.map_or_else(none, |v| {
                if v.gpus.is_empty() {
                    none()
                } else {
                    meter(theme, gpu_pct(v), BAR, row_bg)
                }
            }),
            Col::Trend => tiles::spark(theme, &host.cpu, 10, 100.0, row_bg),
            Col::Pressure => view.map_or_else(none, |v| {
                v.psi.as_ref().map_or_else(none, |p| {
                    meter(theme, Some(p.cpu.max(p.mem).max(p.io)), BAR, row_bg)
                })
            }),
            Col::Hottest => view.and_then(hottest).map_or_else(none, |t| {
                let tone = match t {
                    t if t >= 85.0 => Tone::Error,
                    t if t >= 75.0 => Tone::Warning,
                    _ => Tone::Gray,
                };
                let color = if tone == Tone::Gray {
                    theme.text_secondary
                } else {
                    tone_color(theme, tone)
                };
                Line::styled(format!("{t:.0} °C"), bg.fg(color))
            }),
            Col::Alerts => {
                let open = view.map_or(&[][..], |v| &v.alerts[..]);
                let n = open.len() + usize::from(!host.host.up);
                if n == 0 {
                    Line::styled("0", dim)
                } else {
                    let severe = !host.host.up || open.iter().any(|a| a.severe);
                    let tone = if severe { Tone::Error } else { Tone::Warning };
                    Line::styled(n.to_string(), bg.fg(tone_color(theme, tone)).bold())
                }
            }
        };
        line = line.append(cell(content, c.width, bg));
    }
    line.on(row_bg).pad_to(inner, bg)
}

// ---------------------------------------------------------------------------------------------------------------
// Scrolling pages.

/// Marks a line as the title of a card: the page builders write `section(..)` and their lines, and [`split_cards`]
/// cuts the stream there.
const MARK: char = '\u{1}';

/// A page of cards as the builders write it, cut into one `(title, lines)` per card. Blank lines only separated the
/// sections, so they go; lines before the first title make an untitled card.
fn split_cards(lines: Vec<Line>) -> Vec<(String, Vec<Line>)> {
    let mut cards: Vec<(String, Vec<Line>)> = Vec::new();
    for line in lines {
        let plain = line.plain();
        if let Some(title) = plain.strip_prefix(MARK) {
            cards.push((title.to_string(), Vec::new()));
        } else if line.width() > 0 {
            if cards.is_empty() {
                cards.push((String::new(), Vec::new()));
            }
            if let Some(last) = cards.last_mut() {
                last.1.push(line);
            }
        }
    }
    cards.retain(|(_, body)| !body.is_empty());
    cards
}

/// Two columns of cards when there is room for them, as the web's `gg-columns` does.
const TWO_COLUMNS_FROM: usize = 116;

/// The width of one card column of a page `content` columns wide.
const fn column_width(content: usize) -> usize {
    if content >= TWO_COLUMNS_FROM {
        (content - 1) / 2
    } else {
        content
    }
}

/// The cards of a page as lines: one rounded card per section, side by side when wide (each into the shorter column).
fn page_cards(theme: &Theme, cards: Vec<(String, Vec<Line>)>, content: usize) -> Vec<Line> {
    let cw = column_width(content);
    let inner = widgets::card_inner(cw);
    let built: Vec<Vec<Line>> = cards
        .into_iter()
        .map(|(title, body)| {
            let header = (!title.is_empty())
                .then(|| Line::styled(format!(" {title}"), Style::new().fg(theme.text).bold()));
            let body = body.into_iter().map(|l| l.truncate(inner)).collect();
            widgets::card(theme, header, body, cw)
        })
        .collect();
    if cw == content {
        let mut out = Vec::new();
        for (n, card) in built.into_iter().enumerate() {
            if n > 0 {
                out.push(Line::new());
            }
            out.extend(card);
        }
        return out;
    }
    let mut columns: [Vec<Line>; 2] = [Vec::new(), Vec::new()];
    for card in built {
        let at = usize::from(columns[1].len() < columns[0].len());
        if !columns[at].is_empty() {
            columns[at].push(Line::new());
        }
        columns[at].extend(card);
    }
    let height = columns[0].len().max(columns[1].len());
    let blank = Line::new().space(cw, Style::new());
    (0..height)
        .map(|row| {
            let left = columns[0]
                .get(row)
                .cloned()
                .unwrap_or_else(|| blank.clone());
            let right = columns[1]
                .get(row)
                .cloned()
                .unwrap_or_else(|| blank.clone());
            left.pad_to(cw, Style::new())
                .space(1, Style::new())
                .append(right)
        })
        .collect()
}

/// A page of cards scrolled to `app.scroll` (clamped here, where its length is known).
fn scrolling_cards(
    app: &mut App,
    theme: &Theme,
    lines: Vec<Line>,
    content: usize,
    capacity: usize,
) -> Vec<Line> {
    let all = page_cards(theme, split_cards(lines), content);
    let rows = capacity.max(1);
    app.page_len = all.len();
    app.page_rows = rows.min(all.len().max(1));
    app.scroll = app.scroll.min(all.len().saturating_sub(rows));
    all.into_iter().skip(app.scroll).take(rows).collect()
}

pub(crate) fn section(theme: &Theme, title: &str) -> Line {
    Line::styled(
        format!("{MARK}{title}"),
        Style::new().fg(theme.soft(Tone::Brand).text).bold(),
    )
}

/// ` label  value`, the label in a 14-column gutter.
pub(crate) fn kv(theme: &Theme, label: &str, value: impl Into<String>) -> Line {
    Line::styled(
        format!("   {label:<14}"),
        Style::new().fg(theme.text_tertiary),
    )
    .push(value, Style::new().fg(theme.text))
}

pub(crate) fn meter_row(theme: &Theme, label: &str, value: Option<f64>, extra: &str) -> Line {
    Line::styled(
        format!("   {label:<14}"),
        Style::new().fg(theme.text_tertiary),
    )
    .append(meter(theme, value, 20, theme.surface))
    .push(format!("  {extra}"), Style::new().fg(theme.text_secondary))
}

pub(crate) fn opt<T>(value: Option<T>, show: impl Fn(T) -> String) -> String {
    value.map_or_else(|| "—".to_string(), show)
}

fn current_time_label(secs: Option<f64>, now: f64) -> String {
    opt(secs, |t| ago((now - t).max(0.0)))
}

fn host_lines(host: &HostCard, now: f64, theme: &Theme, inner: usize) -> Vec<Line> {
    let h = &host.host;
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut out = vec![Line::new()];
    out.push(section(theme, "Overview"));
    out.push(Line::styled(format!("   {:<14}", "State"), tertiary).append(badge_for(theme, host)));
    out.push(kv(theme, "Kind", format!("{} · {}", h.kind, h.url)));
    if let Some(why) = &h.error {
        out.push(
            Line::styled(
                format!("   {:<14}{why}", "Error"),
                Style::new().fg(theme.soft(Tone::Error).text),
            )
            .truncate(inner),
        );
        out.push(kv(theme, "Last answer", current_time_label(h.last_ok, now)));
    }
    let Some(view) = &h.view else {
        out.push(Line::styled("   No numbers yet.", tertiary));
        return out;
    };
    if let Some(name) = &view.hostname {
        out.push(kv(theme, "Hostname", name));
    }
    out.push(kv(theme, "Uptime", opt(view.uptime_secs, duration)));
    out.push(kv(
        theme,
        "Load",
        opt(view.load, |l| {
            format!("{:.2}  {:.2}  {:.2}", l[0], l[1], l[2])
        }),
    ));

    if !view.alerts.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Alerts"));
        for a in &view.alerts {
            let tone = if a.severe { Tone::Error } else { Tone::Warning };
            out.push(
                Line::new()
                    .space(3, Style::new())
                    .append(widgets::badge(theme, tone, &a.kind))
                    .push(format!("  {}", a.detail), Style::new().fg(theme.text))
                    .truncate(inner),
            );
        }
    }

    out.extend(compute_lines(theme, view, inner));
    out.extend(device_lines(theme, view, inner));
    out
}

/// CPU, memory, pressure and the GPUs.
fn compute_lines(theme: &Theme, view: &View, inner: usize) -> Vec<Line> {
    let mut out = Vec::new();
    out.push(Line::new());
    out.push(section(theme, "CPU"));
    let extra = [
        view.cpu_freq_mhz.map(|f| format!("{f:.0} MHz")),
        view.cpu_watts.map(|w| format!("{w:.0} W")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    out.push(meter_row(theme, "Busy", view.cpu_pct, &extra));
    out.extend(cores(theme, view, inner));

    out.push(Line::new());
    out.push(section(theme, "Memory"));
    let used = view.mem_used.zip(view.mem_total);
    out.push(meter_row(
        theme,
        "Used",
        mem_pct(view),
        &opt(used, |(u, t)| format!("{} of {}", bytes(u), bytes(t))),
    ));
    if let Some(p) = &view.mem_parts {
        out.push(kv(
            theme,
            "Made of",
            format!(
                "programs {} · cache {} · buffers {} · dirty {}",
                bytes(p.anon),
                bytes(p.cached),
                bytes(p.buffers),
                bytes(p.dirty)
            ),
        ));
    }
    if let Some(total) = view.swap_total.filter(|t| *t > 0.0) {
        let used = view.swap_used.unwrap_or(0.0);
        out.push(meter_row(
            theme,
            "Swap",
            Some(used / total * 100.0),
            &format!("{} of {}", bytes(used), bytes(total)),
        ));
    }
    if let Some(oom) = view.oom_kills.filter(|n| *n > 0.0) {
        out.push(kv(theme, "OOM kills", format!("{oom:.0} since boot")));
    }

    if let Some(p) = &view.psi {
        out.push(Line::new());
        out.push(section(theme, "Pressure"));
        out.push(kv(
            theme,
            "Waiting",
            format!(
                "cpu {} · memory {} · io {}",
                pct(p.cpu),
                pct(p.mem),
                pct(p.io)
            ),
        ));
    }

    for gpu in &view.gpus {
        out.push(Line::new());
        out.push(section(theme, &format!("GPU · {}", gpu.name)));
        out.push(meter_row(theme, "Busy", gpu.busy_pct, ""));
        let vram = gpu.vram_used.zip(gpu.vram_total);
        out.push(meter_row(
            theme,
            "VRAM",
            vram.map(|(u, t)| u / t.max(1.0) * 100.0),
            &opt(vram, |(u, t)| format!("{} of {}", bytes(u), bytes(t))),
        ));
        let facts = [
            gpu.temp_c.map(|t| format!("{t:.0} °C")),
            gpu.power_w.map(|w| {
                gpu.power_cap_w.map_or_else(
                    || format!("{w:.0} W"),
                    |cap| format!("{w:.0} of {cap:.0} W"),
                )
            }),
            gpu.sclk_mhz.map(|c| format!("core {c:.0} MHz")),
            gpu.mclk_mhz.map(|c| format!("memory {c:.0} MHz")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        if !facts.is_empty() {
            out.push(kv(theme, "Facts", facts));
        }
    }

    out
}

/// Filesystems, disks, network, sensors and faults.
fn device_lines(theme: &Theme, view: &View, inner: usize) -> Vec<Line> {
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut out = Vec::new();
    if !view.filesystems.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Filesystems"));
        for f in &view.filesystems {
            let used = (f.size - f.avail).max(0.0);
            out.push(meter_row(
                theme,
                &f.mount.clone().unwrap_or_else(|| f.device.clone()),
                (f.size > 0.0).then(|| used / f.size * 100.0),
                &format!("{} of {}", bytes(used), bytes(f.size)),
            ));
        }
    }

    if !view.disk_stats.is_empty() || !view.disks.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Disk activity"));
        for d in &view.disk_stats {
            out.push(meter_row(
                theme,
                &d.name,
                Some(d.util_pct),
                &format!(
                    "{:.0} IOPS · read {} · write {}",
                    d.iops,
                    opt(d.read_ms, |m| format!("{m:.1} ms")),
                    opt(d.write_ms, |m| format!("{m:.1} ms"))
                ),
            ));
        }
        for d in &view.disks {
            out.push(kv(
                theme,
                &d.name,
                format!("read {} · write {}", rate(d.a_per_sec), rate(d.b_per_sec)),
            ));
        }
    }

    if !view.disk_health.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Disk health"));
        for d in &view.disk_health {
            let tone = if d.healthy {
                Tone::Success
            } else {
                Tone::Error
            };
            let facts = [
                d.model.clone(),
                d.temp_c.map(|t| format!("{t:.0} °C")),
                d.power_on_hours
                    .map(|h| format!("on {}", duration(h * 3600.0))),
                d.wear_pct.map(|w| format!("wear {}", pct(w))),
                d.reallocated
                    .filter(|n| *n > 0.0)
                    .map(|n| format!("{n:.0} reallocated")),
                d.pending
                    .filter(|n| *n > 0.0)
                    .map(|n| format!("{n:.0} pending")),
                d.media_errors
                    .filter(|n| *n > 0.0)
                    .map(|n| format!("{n:.0} media errors")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            out.push(
                Line::styled(format!("   {:<14}", d.device), tertiary)
                    .append(widgets::badge(
                        theme,
                        tone,
                        if d.healthy { "healthy" } else { "failing" },
                    ))
                    .push(format!("  {facts}"), Style::new().fg(theme.text_secondary))
                    .truncate(inner),
            );
        }
    }

    out.extend(network_lines(theme, view));
    out
}

/// Network, sensors and hardware faults.
fn network_lines(theme: &Theme, view: &View) -> Vec<Line> {
    let mut out = Vec::new();
    if !view.nics.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Network"));
        for n in &view.nics {
            let link = view.links.iter().find(|l| l.name == n.name);
            let state = link.map_or(String::new(), |l| {
                format!(
                    "  {}{}",
                    if l.up { "up" } else { "down" },
                    if l.errors_per_sec > 0.0 {
                        format!(" · {:.1} errors/s", l.errors_per_sec)
                    } else {
                        String::new()
                    }
                )
            });
            out.push(kv(
                theme,
                &n.name,
                format!("↓ {} · ↑ {}{state}", rate(n.a_per_sec), rate(n.b_per_sec)),
            ));
        }
        if let Some(r) = view.tcp_retrans_per_sec.filter(|r| *r > 0.0) {
            out.push(kv(theme, "Retransmits", format!("{r:.1} per second")));
        }
    }

    if !view.temps.is_empty() || !view.fans.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Sensors"));
        for t in &view.temps {
            out.push(kv(theme, &t.label, format!("{:.0} °C", t.celsius)));
        }
        for f in &view.fans {
            out.push(kv(theme, &f.label, format!("{:.0} rpm", f.rpm)));
        }
    }

    if !view.faults.is_empty() {
        out.push(Line::new());
        out.push(section(theme, "Hardware faults"));
        for f in &view.faults {
            out.push(Line::styled(
                format!(
                    "   {} {} · {:.0}",
                    widgets::alert_icon(Tone::Error),
                    f.what,
                    f.count
                ),
                Style::new().fg(theme.soft(Tone::Error).text),
            ));
        }
    }
    out
}

/// The cores as small cells flowing across the card.
fn cores(theme: &Theme, view: &View, inner: usize) -> Vec<Line> {
    const CELL: usize = 12;
    let per_row = (inner.saturating_sub(17) / CELL).max(1);
    view.cores_pct
        .chunks(per_row)
        .map(|chunk| {
            let mut line = Line::styled(format!("   {:<14}", ""), Style::new());
            for (name, value) in chunk {
                let color = tone_color(theme, level_tone(Level::of(*value)));
                line = line
                    .push(
                        format!("{:<5}", name.trim_start_matches("cpu")),
                        Style::new().fg(theme.text_tertiary),
                    )
                    .push(format!("{:>4}   ", pct(*value)), Style::new().fg(color));
            }
            line
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Alerts.

/// What needs a look now: a host that stopped answering, and the alerts of those that answer. Severe first.
#[must_use]
pub fn current_alerts(data: &Data) -> Vec<(String, gauge_core::Alert)> {
    let mut all = Vec::new();
    for h in &data.hosts {
        if !h.host.up {
            all.push((
                h.host.name.clone(),
                gauge_core::Alert {
                    key: "down".to_string(),
                    kind: "down".to_string(),
                    detail: h
                        .host
                        .error
                        .clone()
                        .unwrap_or_else(|| "not answering".to_string()),
                    severe: true,
                },
            ));
        }
        if let Some(v) = &h.host.view {
            all.extend(v.alerts.iter().map(|a| (h.host.name.clone(), a.clone())));
        }
    }
    all.sort_by(|a, b| b.1.severe.cmp(&a.1.severe).then_with(|| a.0.cmp(&b.0)));
    all
}

fn alert_lines(data: &Data, now: f64, theme: &Theme, inner: usize) -> Vec<Line> {
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut out = vec![Line::new(), section(theme, "Now")];
    let current = current_alerts(data);
    if current.is_empty() {
        out.push(Line::styled(
            "   All clear.",
            Style::new().fg(theme.soft(Tone::Success).text),
        ));
    }
    for (host, a) in &current {
        out.push(alert_row(theme, data, host, a, now, inner));
    }

    let mut ended: Vec<&AlertEvent> = data.alerts.iter().filter(|e| e.closed.is_some()).collect();
    ended.sort_by(|a, b| {
        b.closed
            .partial_cmp(&a.closed)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.push(Line::new());
    out.push(section(theme, "Ended"));
    if ended.is_empty() {
        out.push(Line::styled("   Nothing in the log yet.", tertiary));
    }
    for e in ended.into_iter().take(30) {
        let closed = e.closed.unwrap_or(e.opened);
        out.push(
            Line::styled(format!("   {:<14}", e.host), Style::new().fg(theme.text))
                .push(
                    format!("{}  ", e.kind),
                    Style::new().fg(theme.text_secondary),
                )
                .push(e.detail.clone(), Style::new().fg(theme.text_secondary))
                .push(
                    format!(
                        "  {} · lasted {}",
                        ago((now - closed).max(0.0)),
                        duration(closed - e.opened)
                    ),
                    tertiary,
                )
                .truncate(inner),
        );
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// Storage.

fn storage_lines(data: &Data, theme: &Theme, inner: usize) -> Vec<Line> {
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut out = storage_summary(data, theme);
    for h in &data.hosts {
        let Some(view) = &h.host.view else { continue };
        if view.filesystems.is_empty() && view.faults.is_empty() && view.disk_stats.is_empty() {
            continue;
        }
        out.push(Line::new());
        out.push(section(theme, &h.host.name));
        for f in &view.filesystems {
            let mount = f.mount.clone().unwrap_or_else(|| f.device.clone());
            let used = (f.size - f.avail).max(0.0);
            let forecast = data
                .forecast
                .iter()
                .find(|d| d.host == h.host.name && d.mount == mount);
            let growth = forecast
                .and_then(|d| d.per_day)
                .map_or(String::new(), |p| format!("  {}/day", bytes(p)));
            let full = forecast
                .and_then(|d| d.days_left)
                .map_or(String::new(), |d| {
                    format!("  full in {}", duration(d * 86_400.0))
                });
            out.push(
                Line::styled(
                    format!("   {:<20}", truncate_text(&mount, 19)),
                    Style::new().fg(theme.text),
                )
                .append(meter(
                    theme,
                    (f.size > 0.0).then(|| used / f.size * 100.0),
                    24,
                    theme.surface,
                ))
                .push(
                    format!("  {} of {}", bytes(used), bytes(f.size)),
                    Style::new().fg(theme.text_secondary),
                )
                .push(growth, tertiary)
                .push(full, Style::new().fg(theme.soft(Tone::Warning).text))
                .truncate(inner),
            );
        }
        for d in &view.disk_stats {
            out.push(
                kv(
                    theme,
                    &format!("io {}", d.name),
                    format!(
                        "busy {} · {:.0} IOPS · read {} · write {}",
                        pct(d.util_pct),
                        d.iops,
                        opt(d.read_ms, |m| format!("{m:.1} ms")),
                        opt(d.write_ms, |m| format!("{m:.1} ms")),
                    ),
                )
                .truncate(inner),
            );
        }
        for fault in &view.faults {
            out.push(Line::styled(
                format!(
                    "   {} {} · {:.0}",
                    widgets::alert_icon(Tone::Error),
                    fault.what,
                    fault.count
                ),
                Style::new().fg(theme.soft(Tone::Error).text),
            ));
        }
    }
    if out.is_empty() {
        out.push(Line::new());
        out.push(Line::styled("   No filesystems reported yet.", tertiary));
    }
    out
}

/// Capacity, what is free, the filesystem that fills first and the fullest one.
fn storage_summary(data: &Data, theme: &Theme) -> Vec<Line> {
    let fs: Vec<(&str, &gauge_core::Filesystem)> = data
        .hosts
        .iter()
        .filter_map(|h| h.host.view.as_ref().map(|v| (h.host.name.as_str(), v)))
        .flat_map(|(name, v)| v.filesystems.iter().map(move |f| (name, f)))
        .collect();
    if fs.is_empty() {
        return Vec::new();
    }
    let size: f64 = fs.iter().map(|(_, f)| f.size).sum();
    let avail: f64 = fs.iter().map(|(_, f)| f.avail).sum();
    let mut out = vec![Line::new(), section(theme, "Summary")];
    out.push(kv(theme, "Capacity", bytes(size)));
    out.push(kv(theme, "Free", bytes(avail)));
    let soonest = data
        .forecast
        .iter()
        .filter_map(|d| d.days_left.map(|left| (left, d)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    out.push(kv(
        theme,
        "Fills first",
        soonest.map_or_else(
            || "nothing is filling".to_string(),
            |(left, d)| format!("{} {} in {}", d.host, d.mount, duration(left * 86_400.0)),
        ),
    ));
    let fullest = fs
        .iter()
        .filter(|(_, f)| f.size > 0.0)
        .map(|(h, f)| (*h, f, (f.size - f.avail) / f.size * 100.0))
        .max_by(|a, b| a.2.total_cmp(&b.2));
    if let Some((host, f, used)) = fullest {
        out.push(kv(
            theme,
            "Fullest",
            format!(
                "{host} {} at {}",
                f.mount.clone().unwrap_or_else(|| f.device.clone()),
                pct(used)
            ),
        ));
    }
    out
}

pub(crate) fn truncate_text(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let head: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

/// One repaint, as `forge-toolbox` writes it.
///
/// The cursor goes home, each row is written over the
/// old one and followed by `erase` and "erase to end of line", and everything below the last row is erased, all in
/// one write, so a repaint never blinks. Rows should be one column narrower than the terminal.
#[must_use]
pub fn frame(rows: &[String], erase: &str) -> String {
    let mut out = String::from("\x1b[H");
    let tail = format!("{erase}\x1b[K\x1b[0m\r\n");
    out.push_str(&rows.join(&tail));
    out.push_str(&tail.replace("\r\n", ""));
    out.push_str(erase);
    out.push_str("\x1b[J\x1b[0m");
    out
}
