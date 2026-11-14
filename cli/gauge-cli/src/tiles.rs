//! Sparklines and stat tiles: a big number, a bar and the last hour as a line of blocks.

use crate::state::Data;
use crate::view::{bar, level_tone, mem_pct, net_totals, tone_color};
use forge_tui::{Line, Style, Theme, Tone};
use gauge_core::format::{bytes, pct, rate};
use gauge_core::{HostCard, HostDetail, Level, Point};

/// Rows a tile takes.
pub const TILE_ROWS: usize = 4;
/// The narrowest a tile is drawn; fewer tiles are shown rather than narrower ones.
const MIN_TILE: usize = 20;
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// One stat: what it is, its value, a bar and a history.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub label: String,
    pub value: String,
    pub sub: String,
    /// How full the bar is, 0 to 100; `None` draws an empty track.
    pub bar: Option<f64>,
    pub tone: Tone,
    /// The history, oldest first; `None` entries are gaps.
    pub series: Vec<Option<f64>>,
    /// What the top of the sparkline stands for.
    pub scale: f64,
}

impl Tile {
    fn percent(label: &str, value: Option<f64>, sub: String, series: Vec<Option<f64>>) -> Self {
        Self {
            label: label.to_string(),
            value: value.map_or_else(|| "—".to_string(), pct),
            sub,
            bar: value,
            tone: value.map_or(Tone::Gray, |v| level_tone(Level::of(v))),
            series,
            scale: 100.0,
        }
    }
}

/// The series as `width` columns: each the mean of its share of the points, `None` where there was nothing. A short
/// series is not stretched: it has fewer columns, and the caller right-aligns it so the newest is at the right edge.
fn resample(series: &[Option<f64>], width: usize) -> Vec<Option<f64>> {
    let cells = width.min(series.len());
    (0..cells)
        .map(|i| {
            let from = i * series.len() / cells;
            let to = ((i + 1) * series.len() / cells)
                .max(from + 1)
                .min(series.len());
            let values: Vec<f64> = series[from..to].iter().flatten().copied().collect();
            #[allow(clippy::cast_precision_loss, reason = "a handful of points")]
            (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
        })
        .collect()
}

/// `width` columns of blocks for a series, one line tall.
#[must_use]
pub fn spark(
    theme: &Theme,
    series: &[Option<f64>],
    width: usize,
    scale: f64,
    bg: forge_tui::Rgb,
) -> Line {
    let base = Style::new().bg(bg);
    if series.iter().all(Option::is_none) || width == 0 {
        return Line::styled("no history", base.fg(theme.text_disabled));
    }
    let columns = resample(series, width);
    let text: String = columns
        .iter()
        .map(|v| v.map_or(' ', |v| glyph(v, scale)))
        .collect();
    let last = columns
        .iter()
        .rev()
        .flatten()
        .next()
        .copied()
        .unwrap_or(0.0);
    let tone = if (scale - 100.0).abs() < f64::EPSILON {
        level_tone(Level::of(last))
    } else {
        Tone::Info
    };
    Line::new()
        .space(width - columns.len(), base)
        .push(text, base.fg(tone_color(theme, tone)))
}

/// A chart `rows` lines tall (the web card's CPU chart): each column a bar of blocks to the eighth of a line.
#[must_use]
pub fn chart(
    theme: &Theme,
    series: &[Option<f64>],
    width: usize,
    rows: usize,
    scale: f64,
    bg: forge_tui::Rgb,
) -> Vec<Line> {
    let base = Style::new().bg(bg);
    if series.iter().all(Option::is_none) || width == 0 {
        let mut out = vec![Line::new().space(width, base); rows];
        if let Some(mid) = out.get_mut(rows / 2) {
            *mid = Line::styled("no history", base.fg(theme.text_disabled));
        }
        return out;
    }
    let columns = resample(series, width);
    let color = base.fg(tone_color(theme, Tone::Brand));
    let pad = width - columns.len();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a few rows of eighths"
    )]
    let eighths: Vec<usize> = columns
        .iter()
        .map(|v| {
            v.map_or(0, |v| {
                ((v / scale).clamp(0.0, 1.0) * (rows * 8) as f64).round() as usize
            })
        })
        .collect();
    (0..rows)
        .rev()
        .map(|row| {
            let text: String = eighths
                .iter()
                .map(|h| match h.saturating_sub(row * 8).min(8) {
                    0 => ' ',
                    n => BLOCKS[n - 1],
                })
                .collect();
            Line::new().space(pad, base).push(text, color)
        })
        .collect()
}

fn glyph(value: f64, scale: f64) -> char {
    let ratio = if scale > 0.0 {
        (value / scale).clamp(0.0, 1.0)
    } else {
        0.0
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0..=7 by construction"
    )]
    let at = (ratio * 7.0).round() as usize;
    BLOCKS[at.min(7)]
}

/// A row of tiles across `width` columns, [`TILE_ROWS`] lines tall. Tiles that do not fit are left out, last first.
#[must_use]
pub fn row(theme: &Theme, tiles: &[Tile], width: usize) -> Vec<Line> {
    let mut n = tiles.len();
    if n == 0 {
        return Vec::new();
    }
    while n > 1 && width.saturating_sub(n - 1) / n < MIN_TILE {
        n -= 1;
    }
    let w = (width.saturating_sub(n - 1) / n).max(8);
    let mut rows = vec![Line::new(); TILE_ROWS];
    for (i, tile) in tiles.iter().take(n).enumerate() {
        for (row, part) in rows.iter_mut().zip(tile_lines(theme, tile, w)) {
            let head = std::mem::take(row);
            *row = if i > 0 {
                head.space(1, Style::new())
            } else {
                head
            }
            .append(part);
        }
    }
    rows
}

fn tile_lines(theme: &Theme, tile: &Tile, w: usize) -> [Line; TILE_ROWS] {
    let bg = theme.surface_2;
    let base = Style::new().bg(bg);
    let inner = w.saturating_sub(2);
    let pad = |line: Line| {
        Line::new()
            .space(1, base)
            .append(line.truncate(inner))
            .pad_to(w, base)
    };
    let color = tone_color(theme, tile.tone);
    [
        pad(Line::styled(
            &tile.label,
            base.fg(theme.text_tertiary).bold(),
        )),
        pad(Line::styled(&tile.value, base.fg(color).bold())
            .push(format!("  {}", tile.sub), base.fg(theme.text_tertiary))),
        pad(bar(theme, tile.bar, color, inner, bg)),
        pad(spark(theme, &tile.series, inner, tile.scale, bg)),
    ]
}

fn cpu_series(host: &HostCard, detail: Option<&HostDetail>) -> Vec<Option<f64>> {
    detail
        .filter(|d| d.host.name == host.host.name && !d.history.is_empty())
        .map_or_else(
            || host.cpu.clone(),
            |d| d.history.iter().map(|p| p.cpu).collect(),
        )
}

fn points(
    detail: Option<&HostDetail>,
    name: &str,
    pick: impl Fn(&Point) -> Option<f64>,
) -> Vec<Option<f64>> {
    detail
        .filter(|d| d.host.name == name)
        .map(|d| d.history.iter().map(pick).collect())
        .unwrap_or_default()
}

/// The tiles of one host's page.
#[must_use]
pub fn host_tiles(host: &HostCard, detail: Option<&HostDetail>) -> Vec<Tile> {
    let name = host.host.name.as_str();
    let Some(view) = &host.host.view else {
        return vec![Tile::percent(
            "CPU",
            None,
            String::new(),
            cpu_series(host, detail),
        )];
    };
    let mut tiles = vec![Tile::percent(
        "CPU",
        view.cpu_pct,
        view.load
            .map_or_else(String::new, |l| format!("load {:.2}", l[0])),
        cpu_series(host, detail),
    )];
    tiles.push(Tile::percent(
        "Memory",
        mem_pct(view),
        view.mem_used
            .zip(view.mem_total)
            .map_or_else(String::new, |(u, t)| {
                format!("{} of {}", bytes(u), bytes(t))
            }),
        points(detail, name, |p| p.mem),
    ));
    if !view.gpus.is_empty() {
        let busy = view.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max);
        let temp = view.gpus.iter().filter_map(|g| g.temp_c).reduce(f64::max);
        tiles.push(Tile::percent(
            "GPU",
            busy,
            temp.map_or_else(String::new, |t| format!("{t:.0} °C")),
            points(detail, name, |p| p.gpu),
        ));
    }
    let fullest = view
        .filesystems
        .iter()
        .filter(|f| f.size > 0.0)
        .map(|f| (f, (f.size - f.avail) / f.size * 100.0))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((f, used)) = fullest {
        tiles.push(Tile::percent(
            "Disk",
            Some(used),
            f.mount.clone().unwrap_or_else(|| f.device.clone()),
            points(detail, name, |p| p.disk_util),
        ));
    }
    let (rx, tx) = net_totals(view);
    let series = points(detail, name, |p| p.rx);
    let scale = series.iter().flatten().copied().fold(1.0, f64::max);
    tiles.push(Tile {
        label: "Network".to_string(),
        value: format!("↓ {}", rate(rx)),
        sub: format!("↑ {}", rate(tx)),
        bar: None,
        tone: Tone::Info,
        series,
        scale,
    });
    tiles
}

/// The tiles of the host list: the fleet as one.
#[must_use]
pub fn fleet_tiles(data: &Data) -> Vec<Tile> {
    let total = data.hosts.len();
    let up = data.hosts.iter().filter(|h| h.host.up).count();
    let views: Vec<_> = data
        .hosts
        .iter()
        .filter_map(|h| h.host.view.as_ref())
        .collect();
    #[allow(clippy::cast_precision_loss, reason = "a handful of hosts")]
    let share = |n: usize, of: usize| {
        if of == 0 {
            0.0
        } else {
            n as f64 / of as f64 * 100.0
        }
    };
    let mut tiles = vec![Tile {
        label: "Hosts".to_string(),
        value: format!("{up}/{total} up"),
        sub: String::new(),
        bar: Some(share(up, total)),
        tone: if up == total {
            Tone::Success
        } else if up == 0 {
            Tone::Error
        } else {
            Tone::Warning
        },
        series: Vec::new(),
        scale: 100.0,
    }];

    let cpus: Vec<f64> = views.iter().filter_map(|v| v.cpu_pct).collect();
    #[allow(clippy::cast_precision_loss, reason = "a handful of hosts")]
    let cpu = (!cpus.is_empty()).then(|| cpus.iter().sum::<f64>() / cpus.len() as f64);
    tiles.push(Tile::percent(
        "CPU (average)",
        cpu,
        String::new(),
        average_cpu(data),
    ));

    let (used, size) = views
        .iter()
        .filter_map(|v| v.mem_used.zip(v.mem_total))
        .fold((0.0, 0.0), |(u, t), (hu, ht)| (u + hu, t + ht));
    tiles.push(Tile::percent(
        "Memory",
        (size > 0.0).then(|| used / size * 100.0),
        if size > 0.0 {
            format!("{} of {}", bytes(used), bytes(size))
        } else {
            String::new()
        },
        Vec::new(),
    ));

    let gpus: Vec<f64> = views
        .iter()
        .flat_map(|v| v.gpus.iter().filter_map(|g| g.busy_pct))
        .collect();
    if !gpus.is_empty() {
        tiles.push(Tile::percent(
            "GPU (busiest)",
            gpus.iter().copied().reduce(f64::max),
            format!(
                "{} {}",
                gpus.len(),
                if gpus.len() == 1 { "GPU" } else { "GPUs" }
            ),
            Vec::new(),
        ));
    }

    let alerts = crate::view::current_alerts(data);
    let severe = alerts.iter().filter(|(_, a)| a.severe).count();
    tiles.push(Tile {
        label: "Alerts".to_string(),
        value: alerts.len().to_string(),
        sub: if severe > 0 {
            format!("{severe} severe")
        } else {
            String::new()
        },
        bar: None,
        tone: if severe > 0 {
            Tone::Error
        } else if alerts.is_empty() {
            Tone::Success
        } else {
            Tone::Warning
        },
        series: Vec::new(),
        scale: 100.0,
    });
    tiles
}

/// The mean CPU of the hosts at each point of their histories, lined up at the newest end.
fn average_cpu(data: &Data) -> Vec<Option<f64>> {
    let len = data.hosts.iter().map(|h| h.cpu.len()).max().unwrap_or(0);
    (0..len)
        .map(|back| {
            let values: Vec<f64> = data
                .hosts
                .iter()
                .filter_map(|h| h.cpu.len().checked_sub(len - back).and_then(|i| h.cpu[i]))
                .collect();
            #[allow(clippy::cast_precision_loss, reason = "a handful of hosts")]
            (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
        })
        .collect()
}
