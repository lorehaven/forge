//! The two screens, as functions from the numbers to a view.

use gauge_core::chart::{self, Series};
use gauge_core::format::{self, Level};
use gauge_core::model::{Gpu, Point, View as Metrics};
use gauge_core::{HostCard, HostDetail, HostSummary};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::link;

const DASH: &str = "—";

/// A label, a bar and its value. `None` shows an empty bar and a dash.
fn meter(t: &I18n, key: &str, pct: Option<f64>) -> Element {
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

fn state(t: &I18n, up: bool) -> Element {
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
fn outage(host: &HostSummary, now: f64) -> Option<String> {
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

fn load_text(load: Option<[f64; 3]>) -> String {
    load.map_or_else(
        || format!(" {DASH}"),
        |l| format!(" {:.2} {:.2} {:.2}", l[0], l[1], l[2]),
    )
}

fn worst_disk(view: &Metrics) -> Option<f64> {
    view.filesystems
        .iter()
        .filter(|f| f.size > 0.0)
        .map(|f| (f.size - f.avail) / f.size * 100.0)
        .reduce(f64::max)
}

fn busiest_gpu(view: &Metrics) -> Option<f64> {
    view.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max)
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

fn section(t: &I18n, title_key: &str, aside: Option<String>) -> Element {
    div().class("gg-section").child(
        element("h4")
            .child(span().text(t.t(title_key)))
            .child(aside.map(|a| element("small").text(a))),
    )
}

fn legend(t: &I18n, first: &str, second: &str) -> Element {
    div()
        .class("gg-legend")
        .child(span().child(element("i")).child(span().text(t.t(first))))
        .child(
            span()
                .child(element("i").class("gg-l2"))
                .child(span().text(t.t(second))),
        )
}

fn stat(t: &I18n, key: &str, value: String) -> Element {
    div()
        .class("gg-stat")
        .child(span().class("gg-muted").text(t.t(key)))
        .child(strong().text(value))
}

fn series(history: &[Point], f: impl Fn(&Point) -> Option<f64>) -> Vec<Option<f64>> {
    history.iter().map(f).collect()
}

fn head(t: &I18n, keys: &[&str]) -> Element {
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

fn rows(items: impl IntoIterator<Item = Vec<String>>) -> Element {
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

fn cpu_section(t: &I18n, history: &[Point], view: &Metrics) -> Element {
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
    let aside = view
        .load
        .map(|l| format!("load {:.2} · {:.2} · {:.2}", l[0], l[1], l[2]));
    section(t, "ui_cpu", aside)
        .child(chart_image(
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

fn memory_section(t: &I18n, history: &[Point], view: &Metrics) -> Element {
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
        .child(chart_image(
            &[Series {
                colour: 1,
                values: &mem,
            }],
            100.0,
            history.len(),
            "memory",
        ))
        .child(swap)
}

fn gpu_section(t: &I18n, history: &[Point], gpu: &Gpu) -> Element {
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
        .child(legend(t, "ui_gpu_busy", "ui_vram"))
        .child(chart_image(
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

fn disks_section(t: &I18n, history: &[Point], view: &Metrics) -> Element {
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
    let io = rows(view.disks.iter().map(|r| {
        vec![
            r.name.clone(),
            format::rate(r.a_per_sec),
            format::rate(r.b_per_sec),
        ]
    }));
    section(t, "ui_disk", None)
        .child(legend(t, "ui_col_read", "ui_col_write"))
        .child(chart_image(
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
                        .child(head(t, &["ui_col_device", "ui_col_read", "ui_col_write"]))
                        .child(io),
                ),
        )
}

fn network_section(t: &I18n, history: &[Point], view: &Metrics) -> Element {
    let rx = series(history, |p| p.rx);
    let tx = series(history, |p| p.tx);
    let nics = rows(view.nics.iter().map(|r| {
        vec![
            r.name.clone(),
            format::rate(r.a_per_sec),
            format::rate(r.b_per_sec),
        ]
    }));
    section(t, "ui_network", None)
        .child(legend(t, "ui_col_rx", "ui_col_tx"))
        .child(chart_image(
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
                .child(head(t, &["ui_col_device", "ui_col_rx", "ui_col_tx"]))
                .child(nics),
        )
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

/// The host screen: every number Gauge has for it, and the last hour of the headline ones.
pub fn host(t: &I18n, detail: &HostDetail, base: &str, now: f64) -> View {
    let summary = &detail.host;
    let history = detail.history.as_slice();
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
        .child(cpu_section(t, history, view))
        .child(memory_section(t, history, view));
    for gpu in &view.gpus {
        page = page.child(gpu_section(t, history, gpu));
    }
    page = page
        .child(disks_section(t, history, view))
        .child(network_section(t, history, view));
    if !view.temps.is_empty() {
        page = page.child(temps_section(t, view));
    }
    page.into_view()
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
