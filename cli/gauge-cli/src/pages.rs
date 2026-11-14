//! The pages that scroll and read across hosts: GPUs, Network and Power.

use crate::state::Data;
use crate::view::{kv, meter_row, opt, section};
use forge_tui::{Line, Style, Theme};
use gauge_core::View;
use gauge_core::format::{bytes, rate};

fn views(data: &Data) -> impl Iterator<Item = (&str, &View)> {
    data.hosts
        .iter()
        .filter_map(|h| h.host.view.as_ref().map(|v| (h.host.name.as_str(), v)))
}

fn empty(theme: &Theme, what: &str) -> Vec<Line> {
    vec![
        Line::new(),
        Line::styled(format!("   {what}"), Style::new().fg(theme.text_tertiary)),
    ]
}

/// Every GPU of every host: busy, memory and power meters, and its facts.
#[must_use]
pub fn gpu_lines(data: &Data, theme: &Theme) -> Vec<Line> {
    let mut out = Vec::new();
    for (host, view) in views(data) {
        for gpu in &view.gpus {
            out.push(Line::new());
            out.push(section(theme, &format!("{host} · {}", gpu.name)));
            out.push(meter_row(theme, "Busy", gpu.busy_pct, ""));
            let vram = gpu.vram_used.zip(gpu.vram_total);
            out.push(meter_row(
                theme,
                "VRAM",
                vram.map(|(u, t)| u / t.max(1.0) * 100.0),
                &opt(vram, |(u, t)| format!("{} of {}", bytes(u), bytes(t))),
            ));
            let gtt = gpu.gtt_used.zip(gpu.gtt_total).filter(|(_, t)| *t > 0.0);
            if let Some((u, t)) = gtt {
                out.push(meter_row(
                    theme,
                    "GTT",
                    Some(u / t * 100.0),
                    &format!("{} of {}", bytes(u), bytes(t)),
                ));
            }
            if let Some(w) = gpu.power_w {
                out.push(meter_row(
                    theme,
                    "Power",
                    gpu.power_cap_w.filter(|c| *c > 0.0).map(|c| w / c * 100.0),
                    &gpu.power_cap_w
                        .map_or_else(|| format!("{w:.0} W"), |c| format!("{w:.0} of {c:.0} W")),
                ));
            }
            let fan = gpu.fan_rpm.map(|r| {
                gpu.fan_max_rpm.map_or_else(
                    || format!("{r:.0} rpm"),
                    |m| format!("{r:.0} of {m:.0} rpm"),
                )
            });
            for (label, value) in [
                ("Temperature", gpu.temp_c.map(|t| format!("{t:.0} °C"))),
                ("Core clock", gpu.sclk_mhz.map(|c| format!("{c:.0} MHz"))),
                ("Memory clock", gpu.mclk_mhz.map(|c| format!("{c:.0} MHz"))),
                ("Fan", fan),
            ] {
                if let Some(value) = value {
                    out.push(kv(theme, label, value));
                }
            }
        }
    }
    if out.is_empty() {
        return empty(theme, "No GPUs reported.");
    }
    out
}

/// Totals, then every interface of every host.
#[must_use]
pub fn network_page(data: &Data, theme: &Theme, inner: usize) -> Vec<Line> {
    let (mut rx, mut tx, mut retrans) = (0.0, 0.0, 0.0);
    for (_, v) in views(data) {
        for n in &v.nics {
            rx += n.a_per_sec;
            tx += n.b_per_sec;
        }
        retrans += v.tcp_retrans_per_sec.unwrap_or(0.0);
    }
    let mut out = vec![Line::new(), section(theme, "Total")];
    out.push(kv(theme, "Received", rate(rx)));
    out.push(kv(theme, "Sent", rate(tx)));
    out.push(kv(theme, "Retransmits", format!("{retrans:.1} per second")));
    for (host, v) in views(data) {
        if v.nics.is_empty() {
            continue;
        }
        out.push(Line::new());
        out.push(section(theme, host));
        for n in &v.nics {
            let link = v.links.iter().find(|l| l.name == n.name);
            let state = link.map_or_else(String::new, |l| {
                let speed = l
                    .speed
                    .map_or_else(String::new, |s| format!(" · {}", rate(s)));
                let errors = if l.errors_per_sec > 0.0 {
                    format!(" · {:.1} errors/s", l.errors_per_sec)
                } else {
                    String::new()
                };
                format!("   {}{speed}{errors}", if l.up { "up" } else { "down" })
            });
            out.push(
                kv(
                    theme,
                    &n.name,
                    format!(
                        "↓ {:<12} ↑ {:<12}{state}",
                        rate(n.a_per_sec),
                        rate(n.b_per_sec)
                    ),
                )
                .truncate(inner),
            );
        }
    }
    out
}

/// Measured draw per host (CPU package plus GPUs; not what the wall socket sees) and the fans.
#[must_use]
pub fn power_lines(data: &Data, theme: &Theme) -> Vec<Line> {
    let tertiary = Style::new().fg(theme.text_tertiary);
    let mut rows = Vec::new();
    let mut total = 0.0;
    for (host, v) in views(data) {
        let gpu: f64 = v.gpus.iter().filter_map(|g| g.power_w).sum();
        let cpu = v.cpu_watts;
        if cpu.is_none() && gpu == 0.0 {
            continue;
        }
        let draw = cpu.unwrap_or(0.0) + gpu;
        total += draw;
        let mut parts = Vec::new();
        if let Some(c) = cpu {
            parts.push(format!("CPU {c:.0} W"));
        }
        for g in v.gpus.iter().filter(|g| g.power_w.is_some()) {
            parts.push(format!("{} {:.0} W", g.name, g.power_w.unwrap_or(0.0)));
        }
        rows.push(kv(
            theme,
            host,
            format!("{draw:.0} W   {}", parts.join(" · ")),
        ));
    }
    let mut out = vec![Line::new(), section(theme, "Measured draw")];
    if rows.is_empty() {
        out.push(Line::styled("   No power readings.", tertiary));
    } else {
        out.push(kv(theme, "Total", format!("{total:.0} W")));
        out.extend(rows);
        out.push(Line::styled(
            "   CPU package and GPUs only: not what the wall socket draws.",
            tertiary,
        ));
    }
    let mut fans = Vec::new();
    for (host, v) in views(data) {
        for f in &v.fans {
            fans.push(kv(theme, host, format!("{} · {:.0} rpm", f.label, f.rpm)));
        }
    }
    out.push(Line::new());
    out.push(section(theme, "Fans"));
    if fans.is_empty() {
        out.push(Line::styled("   No fans reported.", tertiary));
    }
    out.extend(fans);
    out
}
