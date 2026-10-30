//! The two screens, as functions from the numbers to a view.

use gauge_core::chart::{self, Series};
use gauge_core::format::{self, Level};
use gauge_core::model::{Alert, Gpu, Point, View as Metrics};
use gauge_core::{HostCard, HostDetail, HostHistory, HostSummary, Range};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{Router, link};
use std::rc::Rc;

use crate::fleet;

const DASH: &str = "—";

/// A label, a bar and its value. `None` shows an empty bar and a dash.
pub(crate) fn meter(t: &I18n, key: &str, pct: Option<f64>) -> Element {
    let value = pct.unwrap_or(0.0).clamp(0.0, 100.0);
    div()
        .class("gg-meter")
        .child(span().text(t.t(key)))
        .child(
            div()
                .class(format!("gg-bar {}", Level::of(value).class()))
                .attr("role", "progressbar")
                .attr("aria-valuenow", format!("{value:.0}"))
                .child(span().css("width", move || format!("{value:.1}%"))),
        )
        .child(element("b").text(pct.map_or_else(|| DASH.to_string(), format::pct)))
}

pub(crate) fn state(t: &I18n, up: bool) -> Element {
    span()
        .class(if up {
            "gg-state gg-state-up"
        } else {
            "gg-state gg-state-down"
        })
        .child(span().class("gg-dot"))
        .child(span().text(t.t(if up { "ui_state_up" } else { "ui_state_down" })))
}

/// Why a host is not answering, and how long ago it last did; `None` while it is up.
pub(crate) fn outage(host: &HostSummary, now: f64) -> Option<String> {
    let error = host.error.as_ref()?;
    Some(host.last_ok.map_or_else(
        || error.clone(),
        |at| format!("{error} - last seen {}", format::ago(now - at)),
    ))
}

fn chart_image(series: &[Series<'_>], max: f64, slots: usize, label: &str) -> Element {
    div().class("gg-chart-box").child(
        img()
            .class("gg-chart")
            .attr("alt", label.to_string())
            .attr("src", chart::data_uri(&chart::line_svg(series, max, slots))),
    )
}

/// The labels under a chart: how long ago each quarter of it is. The newest point is at the right edge.
fn axis(t: &I18n, range: Range) -> Element {
    let window = range.window_secs() as f64;
    let mut row = div().class("gg-axis");
    for f in [1.0, 0.75, 0.5, 0.25, 0.0] {
        row = row.child(span().text(if f == 0.0 {
            t.t("ui_now").to_string()
        } else {
            format!("−{}", format::span(window * f))
        }));
    }
    row
}

/// A chart with its time axis.
fn chart_with_axis(
    t: &I18n,
    range: Range,
    series: &[Series<'_>],
    max: f64,
    slots: usize,
    label: &str,
) -> Element {
    div()
        .class("gg-charted")
        .child(chart_image(series, max, slots, label))
        .child(axis(t, range))
}

fn load_text(load: Option<[f64; 3]>) -> String {
    load.map_or_else(
        || format!(" {DASH}"),
        |l| format!(" {:.2} {:.2} {:.2}", l[0], l[1], l[2]),
    )
}

pub(crate) fn worst_disk(view: &Metrics) -> Option<f64> {
    view.filesystems
        .iter()
        .filter(|f| f.size > 0.0)
        .map(|f| (f.size - f.avail) / f.size * 100.0)
        .reduce(f64::max)
}

pub(crate) fn busiest_gpu(view: &Metrics) -> Option<f64> {
    view.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max)
}

/// "2 alerts" on a card, red when one is severe; nothing when all is well.
fn alert_badge(t: &I18n, view: Option<&Metrics>) -> Option<Element> {
    let view = view.filter(|v| !v.alerts.is_empty())?;
    let severe = view.severe_alerts() > 0;
    Some(
        span()
            .class(if severe {
                "gg-badge gg-badge-crit"
            } else {
                "gg-badge gg-badge-warn"
            })
            .text(format!("{} {}", view.alerts.len(), t.t("ui_alerts"))),
    )
}

/// One card of the hosts screen. `base` is the service's path prefix (`/gauge`).
fn card(t: &I18n, card: &HostCard, base: &str, now: f64) -> Element {
    let host = &card.host;
    let view = host.view.as_ref();
    let mut sub = format!("{} · {}", host.kind, host.url);
    if let Some(cores) = view.map(|v| v.cores_pct.len()).filter(|n| *n > 0) {
        sub = format!("{cores} cores · {sub}");
    }
    let net = view.map(|v| {
        (
            v.nics.iter().map(|r| r.a_per_sec).sum::<f64>(),
            v.nics.iter().map(|r| r.b_per_sec).sum::<f64>(),
        )
    });
    let mut root = link(&format!("{base}/ui/hosts/{}", host.name))
        .class(if host.up {
            "gg-host"
        } else {
            "gg-host gg-stale"
        })
        .attr("id", format!("host-{}", host.name));
    root = root
        .child(
            div()
                .class("gg-head")
                .child(strong().class("gg-name").text(host.name.clone()))
                .child(alert_badge(t, view))
                .child(state(t, host.up)),
        )
        .child(p().class("gg-sub").text(sub))
        .child(outage(host, now).map(|e| p().class("fui-notice fui-notice-error").text(e)))
        .child(meter(t, "ui_cpu", view.and_then(|v| v.cpu_pct)))
        .child(meter(t, "ui_memory", view.and_then(Metrics::mem_pct)))
        .child(
            view.filter(|v| !v.gpus.is_empty())
                .map(|v| meter(t, "ui_gpu", busiest_gpu(v))),
        )
        .child(meter(t, "ui_disk", view.and_then(worst_disk)))
        .child(chart_image(
            &[Series {
                colour: 1,
                values: &card.cpu,
            }],
            100.0,
            card.cpu.len(),
            "CPU",
        ));
    root.child(
        div()
            .class("gg-foot")
            .child(
                span()
                    .child(span().text(t.t("ui_load")))
                    .child(span().text(load_text(view.and_then(|v| v.load)))),
            )
            .child(span().text(net.map_or_else(String::new, |(rx, tx)| {
                format!("↓ {} ↑ {}", format::rate(rx), format::rate(tx))
            }))),
    )
}

/// The hosts screen: one card per host, by name.
pub fn hosts(t: &I18n, hosts: Option<&[HostCard]>, base: &str, now: f64) -> View {
    let Some(hosts) = hosts else {
        return p()
            .attr("id", "waiting")
            .class("fui-empty")
            .text(t.t("ui_waiting"))
            .into_view();
    };
    if hosts.is_empty() {
        return p()
            .attr("id", "no-hosts")
            .class("fui-empty")
            .text(t.t("ui_hosts_none"))
            .into_view();
    }
    let mut sorted: Vec<&HostCard> = hosts.iter().collect();
    sorted.sort_by(|a, b| a.host.name.cmp(&b.host.name));
    div()
        .class("gg-grid-hosts")
        .children(
            sorted
                .into_iter()
                .map(|c| card(t, c, base, now).into_view()),
        )
        .into_view()
}

pub(crate) fn section(t: &I18n, title_key: &str, aside: Option<String>) -> Element {
    div().class("gg-section").child(
        element("h4")
            .child(span().text(t.t(title_key)))
            .child(aside.map(|a| element("small").text(a))),
    )
}

fn legend(t: &I18n, keys: &[&str]) -> Element {
    let mut row = div().class("gg-legend");
    for (n, key) in keys.iter().enumerate() {
        let swatch = match n {
            0 => element("i"),
            1 => element("i").class("gg-l2"),
            _ => element("i").class("gg-l3"),
        };
        row = row.child(span().child(swatch).child(span().text(t.t(key))));
    }
    row
}

pub(crate) fn stat(t: &I18n, key: &str, value: String) -> Element {
    div()
        .class("gg-stat")
        .child(span().class("gg-muted").text(t.t(key)))
        .child(strong().text(value))
}

fn series(history: &[Point], f: impl Fn(&Point) -> Option<f64>) -> Vec<Option<f64>> {
    history.iter().map(f).collect()
}

pub(crate) fn head(t: &I18n, keys: &[&str]) -> Element {
    let mut row = element("tr");
    for key in keys {
        row = row.child(if key.is_empty() {
            element("th")
        } else {
            element("th").text(t.t(key))
        });
    }
    element("thead").child(row)
}

pub(crate) fn rows(items: impl IntoIterator<Item = Vec<String>>) -> Element {
    let mut body = element("tbody");
    for cells in items {
        let mut row = element("tr");
        for cell in cells {
            row = row.child(element("td").text(cell));
        }
        body = body.child(row);
    }
    body
}

fn overview(t: &I18n, view: &Metrics) -> Element {
    div()
        .class("gg-stats")
        .child(stat(
            t,
            "ui_cpu",
            view.cpu_pct.map_or_else(|| DASH.into(), format::pct),
        ))
        .child(stat(
            t,
            "ui_memory",
            view.mem_pct().map_or_else(|| DASH.into(), format::pct),
        ))
        .child(stat(
            t,
            "ui_load",
            view.load
                .map_or_else(|| DASH.into(), |l| format!("{:.2}", l[0])),
        ))
        .child(stat(
            t,
            "ui_uptime",
            view.uptime_secs
                .map_or_else(|| DASH.into(), format::duration),
        ))
        .child(busiest_gpu(view).map(|g| stat(t, "ui_gpu", format::pct(g))))
}

pub(crate) fn alert_text(t: &I18n, a: &Alert) -> String {
    event_text(t, &a.kind, &a.detail)
}

/// An alert's kind in words, and what it is about.
pub(crate) fn event_text(t: &I18n, kind: &str, detail: &str) -> String {
    format!("{} — {detail}", t.t(&format!("ui_alert_{kind}")))
}

/// What needs a look, first on the host screen; absent when there is nothing.
fn alerts_section(t: &I18n, view: &Metrics) -> Option<Element> {
    if view.alerts.is_empty() {
        return None;
    }
    let mut list = div().class("gg-alerts").attr("id", "alerts");
    for a in &view.alerts {
        list = list.child(
            p().class(if a.severe {
                "gg-alert gg-alert-crit"
            } else {
                "gg-alert gg-alert-warn"
            })
            .text(alert_text(t, a)),
        );
    }
    Some(section(t, "ui_alerts_title", None).child(list))
}

fn pressure_section(t: &I18n, range: Range, history: &[Point], view: &Metrics) -> Option<Element> {
    let psi = view.psi.as_ref()?;
    let cpu = series(history, |p| p.psi_cpu);
    let mem = series(history, |p| p.psi_mem);
    let io = series(history, |p| p.psi_io);
    let top = history
        .iter()
        .flat_map(|p| [p.psi_cpu, p.psi_mem, p.psi_io])
        .flatten()
        .fold(0.0, f64::max)
        .max(5.0);
    Some(
        section(t, "ui_pressure", Some(t.t("ui_pressure_hint").to_string()))
            .child(legend(t, &["ui_cpu", "ui_memory", "ui_io"]))
            .child(chart_with_axis(
                t,
                range,
                &[
                    Series {
                        colour: 1,
                        values: &cpu,
                    },
                    Series {
                        colour: 2,
                        values: &mem,
                    },
                    Series {
                        colour: 3,
                        values: &io,
                    },
                ],
                top,
                history.len(),
                "pressure",
            ))
            .child(
                div()
                    .class("gg-stats")
                    .child(stat(t, "ui_cpu", format::pct(psi.cpu)))
                    .child(stat(t, "ui_memory", format::pct(psi.mem)))
                    .child(stat(t, "ui_memory_full", format::pct(psi.mem_full)))
                    .child(stat(t, "ui_io", format::pct(psi.io)))
                    .child(stat(t, "ui_io_full", format::pct(psi.io_full))),
            ),
    )
}

fn cpu_section(t: &I18n, range: Range, history: &[Point], view: &Metrics) -> Element {
    let cpu = series(history, |p| p.cpu);
    let mut cores = div().class("gg-cores");
    for (id, pct) in &view.cores_pct {
        let pct = *pct;
        cores = cores.child(
            div()
                .class("gg-core")
                .child(span().text(format!("#{id}")))
                .child(
                    div()
                        .class(format!("gg-bar {}", Level::of(pct).class()))
                        .attr("title", format::pct(pct))
                        .child(span().css("width", move || format!("{pct:.1}%"))),
                ),
        );
    }
    let aside = [
        view.load
            .map(|l| format!("load {:.2} · {:.2} · {:.2}", l[0], l[1], l[2])),
        view.cpu_freq_mhz.map(|f| format!("{f:.0} MHz")),
        view.cpu_watts.map(|w| format!("{w:.0} W")),
        view.procs_blocked
            .filter(|b| *b > 0.0)
            .map(|b| format!("{b:.0} blocked")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    section(t, "ui_cpu", (!aside.is_empty()).then_some(aside))
        .child(chart_with_axis(
            t,
            range,
            &[Series {
                colour: 1,
                values: &cpu,
            }],
            100.0,
            history.len(),
            "CPU",
        ))
        .child(cores)
}

/// Anonymous memory, page cache and buffers as one bar of the machine's memory.
fn mem_stack(t: &I18n, parts: &gauge_core::MemParts, total: f64) -> Element {
    let share = |v: f64| (v / total * 100.0).clamp(0.0, 100.0);
    let (anon, cached, buffers) = (share(parts.anon), share(parts.cached), share(parts.buffers));
    div()
        .class("gg-stack-box")
        .child(
            div()
                .class("gg-stack")
                .attr("role", "img")
                .child(
                    span()
                        .class("gg-seg gg-seg-anon")
                        .css("width", move || format!("{anon:.1}%")),
                )
                .child(
                    span()
                        .class("gg-seg gg-seg-cache")
                        .css("width", move || format!("{cached:.1}%")),
                )
                .child(
                    span()
                        .class("gg-seg gg-seg-buf")
                        .css("width", move || format!("{buffers:.1}%")),
                ),
        )
        .child(
            div()
                .class("gg-legend")
                .child(span().child(element("i")).child(span().text(format!(
                    "{} {}",
                    t.t("ui_mem_anon"),
                    format::bytes(parts.anon)
                ))))
                .child(
                    span()
                        .child(element("i").class("gg-l2"))
                        .child(span().text(format!(
                            "{} {}",
                            t.t("ui_mem_cache"),
                            format::bytes(parts.cached)
                        ))),
                )
                .child(
                    span()
                        .child(element("i").class("gg-l3"))
                        .child(span().text(format!(
                            "{} {}",
                            t.t("ui_mem_buffers"),
                            format::bytes(parts.buffers)
                        ))),
                )
                .child(span().text(format!(
                    "{} {}",
                    t.t("ui_mem_dirty"),
                    format::bytes(parts.dirty)
                ))),
        )
}

/// Swap traffic and OOM kills: the signs of a machine short of memory that "used" does not show.
fn memory_activity(t: &I18n, view: &Metrics) -> Option<Element> {
    if view.swap_in_per_sec.is_none() && view.oom_kills.is_none() {
        return None;
    }
    Some(
        div()
            .class("gg-stats")
            .child(
                view.swap_in_per_sec
                    .map(|v| stat(t, "ui_swap_in", format::rate(v))),
            )
            .child(
                view.swap_out_per_sec
                    .map(|v| stat(t, "ui_swap_out", format::rate(v))),
            )
            .child(view.oom_kills.map(|v| stat(t, "ui_oom", format!("{v:.0}")))),
    )
}

fn memory_section(t: &I18n, range: Range, history: &[Point], view: &Metrics) -> Element {
    let mem = series(history, |p| p.mem);
    let aside = view
        .mem_total
        .zip(view.mem_used)
        .map(|(total, used)| format!("{} / {}", format::bytes(used), format::bytes(total)));
    let swap = view
        .swap_total
        .filter(|total| *total > 0.0)
        .zip(view.swap_used)
        .map(|(total, used)| meter(t, "ui_swap", Some(used / total * 100.0)));
    section(t, "ui_memory", aside)
        .child(
            view.mem_parts
                .as_ref()
                .zip(view.mem_total)
                .map(|(parts, total)| mem_stack(t, parts, total)),
        )
        .child(chart_with_axis(
            t,
            range,
            &[Series {
                colour: 1,
                values: &mem,
            }],
            100.0,
            history.len(),
            "memory",
        ))
        .child(swap)
        .child(memory_activity(t, view))
}

fn gpu_section(t: &I18n, range: Range, history: &[Point], gpu: &Gpu) -> Element {
    let busy = series(history, |p| p.gpu);
    let vram = series(history, |p| p.vram);
    let vram_pct = gpu
        .vram_used
        .zip(gpu.vram_total)
        .filter(|(_, total)| *total > 0.0)
        .map(|(used, total)| used / total * 100.0);
    let aside = [
        Some(gpu.name.clone()),
        gpu.vram_used
            .zip(gpu.vram_total)
            .map(|(used, total)| format!("{} / {}", format::bytes(used), format::bytes(total))),
        gpu.temp_c.map(|c| format!("{c:.0} °C")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    section(t, "ui_gpu", Some(aside))
        .child(legend(t, &["ui_gpu_busy", "ui_vram"]))
        .child(chart_with_axis(
            t,
            range,
            &[
                Series {
                    colour: 1,
                    values: &busy,
                },
                Series {
                    colour: 2,
                    values: &vram,
                },
            ],
            100.0,
            history.len(),
            "GPU",
        ))
        .child(meter(t, "ui_gpu_busy", gpu.busy_pct))
        .child(meter(t, "ui_vram", vram_pct))
        .child(gpu.power_w.map(|w| {
            let pct = gpu.power_cap_w.filter(|c| *c > 0.0).map(|c| w / c * 100.0);
            meter(t, "ui_power", pct).child(span().class("gg-muted").text(format!(
                "{w:.0} W{}",
                gpu.power_cap_w.map_or_else(String::new, |c| format!(" / {c:.0} W"))
            )))
        }))
        .child(gpu_facts(t, gpu))
}

/// Fan, clocks and GTT: the card's other numbers, only those it reports.
fn gpu_facts(t: &I18n, gpu: &Gpu) -> Option<Element> {
    let fan = gpu.fan_rpm.map(|rpm| {
        gpu.fan_max_rpm.filter(|m| *m > 0.0).map_or_else(
            || format!("{rpm:.0} rpm"),
            |m| format!("{rpm:.0} rpm ({:.0}%)", rpm / m * 100.0),
        )
    });
    let clocks = gpu
        .sclk_mhz
        .zip(gpu.mclk_mhz)
        .map(|(s, m)| format!("{s:.0} / {m:.0} MHz"));
    let gtt = gpu
        .gtt_used
        .zip(gpu.gtt_total)
        .map(|(u, tot)| format!("{} / {}", format::bytes(u), format::bytes(tot)));
    if fan.is_none() && clocks.is_none() && gtt.is_none() {
        return None;
    }
    Some(
        div()
            .class("gg-stats")
            .child(fan.map(|v| stat(t, "ui_fan", v)))
            .child(clocks.map(|v| stat(t, "ui_clocks", v)))
            .child(gtt.map(|v| stat(t, "ui_gtt", v))),
    )
}

fn peak(
    history: &[Point],
    a: impl Fn(&Point) -> Option<f64>,
    b: impl Fn(&Point) -> Option<f64>,
) -> f64 {
    history
        .iter()
        .map(|p| a(p).unwrap_or(0.0).max(b(p).unwrap_or(0.0)))
        .fold(0.0, f64::max)
        .max(1.0)
}

fn disks_section(t: &I18n, range: Range, history: &[Point], view: &Metrics) -> Element {
    let read = series(history, |p| p.read);
    let write = series(history, |p| p.write);
    let mut fs = element("tbody");
    for f in &view.filesystems {
        let used = f.size - f.avail;
        let pct = used / f.size * 100.0;
        fs = fs.child(
            element("tr")
                .child(element("td").text(f.mount.clone().unwrap_or_else(|| f.device.clone())))
                .child(
                    element("td").child(
                        div()
                            .class(format!("gg-bar {}", Level::of(pct).class()))
                            .child(span().css("width", move || format!("{pct:.1}%"))),
                    ),
                )
                .child(element("td").text(format::pct(pct)))
                .child(element("td").text(format!(
                    "{} / {}",
                    format::bytes(used),
                    format::bytes(f.size)
                ))),
        );
    }
    let ms = |v: Option<f64>| v.map_or_else(|| DASH.to_string(), |v| format!("{v:.1} ms"));
    let io = rows(view.disks.iter().map(|r| {
        let stat = view.disk_stats.iter().find(|d| d.name == r.name);
        vec![
            r.name.clone(),
            format::rate(r.a_per_sec),
            format::rate(r.b_per_sec),
            stat.map_or_else(|| DASH.to_string(), |d| format::pct(d.util_pct)),
            stat.map_or_else(|| DASH.to_string(), |d| format!("{:.0}", d.iops)),
            stat.map_or_else(
                || DASH.to_string(),
                |d| format!("{} / {}", ms(d.read_ms), ms(d.write_ms)),
            ),
        ]
    }));
    section(t, "ui_disk", None)
        .child(legend(t, &["ui_col_read", "ui_col_write"]))
        .child(chart_with_axis(
            t,
            range,
            &[
                Series {
                    colour: 1,
                    values: &read,
                },
                Series {
                    colour: 2,
                    values: &write,
                },
            ],
            peak(history, |p| p.read, |p| p.write),
            history.len(),
            "disk throughput",
        ))
        .child(
            div()
                .class("gg-columns")
                .child(
                    element("table")
                        .class("gg-table")
                        .child(head(
                            t,
                            &["ui_col_device", "ui_col_used", "", "ui_col_size"],
                        ))
                        .child(fs),
                )
                .child(
                    element("table")
                        .class("gg-table")
                        .child(head(
                            t,
                            &[
                                "ui_col_device",
                                "ui_col_read",
                                "ui_col_write",
                                "ui_col_util",
                                "ui_col_iops",
                                "ui_col_latency",
                            ],
                        ))
                        .child(io),
                ),
        )
}

fn network_section(t: &I18n, range: Range, history: &[Point], view: &Metrics) -> Element {
    let rx = series(history, |p| p.rx);
    let tx = series(history, |p| p.tx);
    let nics = rows(view.nics.iter().map(|r| {
        let link = view.links.iter().find(|l| l.name == r.name);
        vec![
            r.name.clone(),
            format::rate(r.a_per_sec),
            format::rate(r.b_per_sec),
            link.map_or_else(
                || DASH.to_string(),
                |l| match (l.up, l.speed) {
                    (false, _) => t.t("ui_link_down").to_string(),
                    (true, Some(s)) => format!("{:.0} Mb/s", s * 8.0 / 1e6),
                    (true, None) => t.t("ui_state_up").to_string(),
                },
            ),
            link.map_or_else(
                || DASH.to_string(),
                |l| format!("{:.2}/s", l.errors_per_sec),
            ),
        ]
    }));
    section(t, "ui_network", None)
        .child(legend(t, &["ui_col_rx", "ui_col_tx"]))
        .child(chart_with_axis(
            t,
            range,
            &[
                Series {
                    colour: 1,
                    values: &rx,
                },
                Series {
                    colour: 2,
                    values: &tx,
                },
            ],
            peak(history, |p| p.rx, |p| p.tx),
            history.len(),
            "network throughput",
        ))
        .child(
            element("table")
                .class("gg-table")
                .child(head(
                    t,
                    &[
                        "ui_col_device",
                        "ui_col_rx",
                        "ui_col_tx",
                        "ui_col_link",
                        "ui_col_errors",
                    ],
                ))
                .child(nics),
        )
        .child(view.tcp_retrans_per_sec.map(|v| {
            div()
                .class("gg-stats")
                .child(stat(t, "ui_tcp_retrans", format!("{v:.1}/s")))
        }))
}

fn temps_section(t: &I18n, view: &Metrics) -> Element {
    let body = rows(
        view.temps
            .iter()
            .map(|x| vec![x.label.clone(), format!("{:.0} °C", x.celsius)]),
    );
    section(t, "ui_temps", None).child(
        element("table")
            .class("gg-table")
            .child(head(t, &["ui_col_name", "ui_col_temp"]))
            .child(body),
    )
}

fn fans_section(t: &I18n, view: &Metrics) -> Element {
    let body = rows(
        view.fans
            .iter()
            .map(|f| vec![f.label.clone(), format!("{:.0} rpm", f.rpm)]),
    );
    section(t, "ui_fans", None).child(
        element("table")
            .class("gg-table")
            .child(head(t, &["ui_col_name", "ui_col_speed"]))
            .child(body),
    )
}

/// How far back the host screen's charts look, and what it has to show for it.
pub struct Looking<'a> {
    pub range: Range,
    /// The stored history that came for `range`, if it has come.
    pub stored: Option<&'a HostHistory>,
    /// The service has no database to keep history in.
    pub unavailable: bool,
    /// The person picked a range.
    pub pick: Rc<dyn Fn(Range)>,
}

/// The buttons that choose the range, and a line about what the charts show.
fn ranges(t: &I18n, looking: &Looking<'_>, waiting: bool) -> Element {
    let mut row = div().class("gg-ranges").attr("role", "group");
    for r in Range::ALL {
        let pick = Rc::clone(&looking.pick);
        let mut b = button()
            .attr("type", "button")
            .attr("id", format!("range-{}", r.as_str()))
            .class(if r == looking.range {
                "gg-range gg-range-on"
            } else {
                "gg-range"
            })
            .attr("aria-pressed", (r == looking.range).to_string())
            .text(r.as_str())
            .on_click(move || pick(r));
        if r == Range::Live {
            b = b.attr("title", t.t("ui_range_live"));
        }
        row = row.child(b);
    }
    let note = if looking.unavailable && looking.range != Range::Live {
        Some(t.t("ui_range_unavailable").to_string())
    } else if waiting {
        Some(t.t("ui_waiting").to_string())
    } else if looking.range == Range::Live {
        None
    } else {
        Some(format!(
            "{} {} · {}",
            t.t("ui_range_last"),
            looking.range.as_str(),
            format::duration(looking.range.step_secs() as f64) + " " + &t.t("ui_range_per_point")
        ))
    };
    div()
        .class("gg-range-bar")
        .attr("id", "ranges")
        .child(row)
        .child(note.map(|n| p().class("gg-sub").attr("id", "range-note").text(n)))
}

/// The host screen: every number Gauge has for it, and charts of the last hour, or of the stored history.
pub fn host(t: &I18n, detail: &HostDetail, base: &str, now: f64, looking: &Looking<'_>) -> View {
    let summary = &detail.host;
    let stored = looking
        .stored
        .filter(|h| h.name == summary.name && h.range == looking.range.as_str());
    let history = if looking.range == Range::Live {
        detail.history.as_slice()
    } else {
        stored.map_or(&[][..], |h| h.points.as_slice())
    };
    let mut page = div()
        .class(if summary.up {
            "gg-page"
        } else {
            "gg-page gg-stale"
        })
        .child(
            div()
                .class("gg-head")
                .child(
                    div()
                        .child(
                            link(&format!("{base}/ui/home"))
                                .class("gg-back")
                                .text(t.t("ui_back")),
                        )
                        .child(element("h3").text(summary.name.clone()))
                        .child(
                            p().class("gg-sub")
                                .text(format!("{} · {}", summary.kind, summary.url)),
                        ),
                )
                .child(state(t, summary.up)),
        )
        .child(outage(summary, now).map(|e| p().class("fui-notice fui-notice-error").text(e)));
    let Some(view) = summary.view.as_ref() else {
        return page
            .child(
                p().attr("id", "waiting")
                    .class("fui-empty")
                    .text(t.t("ui_waiting")),
            )
            .into_view();
    };
    page = page
        .child(overview(t, view))
        .child(alerts_section(t, view))
        .child(ranges(
            t,
            looking,
            looking.range != Range::Live && stored.is_none() && !looking.unavailable,
        ))
        .child(cpu_section(t, looking.range, history, view))
        .child(memory_section(t, looking.range, history, view))
        .child(pressure_section(t, looking.range, history, view));
    for gpu in &view.gpus {
        page = page.child(gpu_section(t, looking.range, history, gpu));
    }
    page = page
        .child(disks_section(t, looking.range, history, view))
        .child(network_section(t, looking.range, history, view));
    if !view.temps.is_empty() {
        page = page.child(temps_section(t, view));
    }
    if !view.fans.is_empty() {
        page = page.child(fans_section(t, view));
    }
    page.into_view()
}

/// The sidebar: the hosts, then the screens that look across them. The host screen counts as the hosts.
pub(crate) fn sidebar(t: &Rc<I18n>, router: &Router, base: &str) -> View {
    let (t, router, base) = (Rc::clone(t), router.clone(), base.to_string());
    View::dynamic(move || {
        let here = router.location().path;
        let mut nav = div();
        for (path, key, id, icon) in fleet::SCREENS {
            let href = format!("{base}/ui{path}");
            let on_hosts = path == "/home"
                && (here == format!("{base}/ui")
                    || here == format!("{base}/ui/")
                    || here.starts_with(&format!("{base}/ui/hosts")));
            let mut item = link(&href)
                .attr("id", id.to_string())
                .class("fui-nav-item")
                .child(forge_shell::icon(icon))
                .child(span().text(t.t(key)));
            if here.trim_end_matches('/') == href || on_hosts {
                item = item.attr("aria-current", "page").class("gg-current");
            }
            nav = nav.child(item);
        }
        nav.into_view()
    })
}

/// The screen of a host that does not exist.
pub fn not_found(t: &I18n, base: &str) -> View {
    div()
        .class("gg-page")
        .child(
            link(&format!("{base}/ui/home"))
                .class("gg-back")
                .text(t.t("ui_back")),
        )
        .child(
            p().attr("id", "not-found")
                .class("fui-empty")
                .text(t.t("ui_host_not_found")),
        )
        .into_view()
}
