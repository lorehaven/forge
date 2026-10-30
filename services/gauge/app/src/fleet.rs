//! The screens that look across hosts: storage, fleet, GPUs, network, alerts, power. Each is a function from the list
//! of hosts to a view; the numbers are the ones the hosts screen already has, so they need no request of their own.

use gauge_core::format::{self, Level};
use gauge_core::model::View as Metrics;
use gauge_core::{AlertEvent, DiskForecast, HostCard, HostSummary};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::link;

use crate::screens::{
    alert_text, busiest_gpu, event_text, head, meter, outage, rows, section, stat, state,
    worst_disk,
};

const DASH: &str = "—";

/// The screens, in sidebar order: (path under `/ui`, translation key, element id, icon).
pub const SCREENS: [(&str, &str, &str, &str); 7] = [
    ("/home", "ui_nav_hosts", "nav-hosts", "server"),
    ("/fleet", "ui_nav_fleet", "nav-fleet", "grid"),
    ("/storage", "ui_nav_storage", "nav-storage", "database"),
    ("/gpus", "ui_nav_gpus", "nav-gpus", "cpu"),
    ("/network", "ui_nav_network", "nav-network", "activity"),
    ("/power", "ui_nav_power", "nav-power", "layers"),
    ("/alerts", "ui_nav_alerts", "nav-alerts", "shield"),
];

/// Hosts that have shown numbers at least once, by name; a host that stopped answering keeps its last ones, dimmed.
fn known(hosts: &[HostCard]) -> Vec<(&HostSummary, &Metrics)> {
    let mut out: Vec<_> = hosts
        .iter()
        .filter_map(|c| c.host.view.as_ref().map(|v| (&c.host, v)))
        .collect();
    out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    out
}

fn page(
    t: &I18n,
    id: &str,
    title_key: &str,
    hosts: Option<&[HostCard]>,
    body: impl FnOnce(&[HostCard]) -> Element,
) -> View {
    let Some(hosts) = hosts else {
        return p()
            .attr("id", "waiting")
            .class("fui-empty")
            .text(t.t("ui_waiting"))
            .into_view();
    };
    if known(hosts).is_empty() {
        return p()
            .attr("id", "no-hosts")
            .class("fui-empty")
            .text(t.t("ui_hosts_none"))
            .into_view();
    }
    div()
        .attr("id", id.to_string())
        .class("gg-page")
        .child(element("h3").text(t.t(title_key)))
        .child(body(hosts))
        .into_view()
}

fn host_title(host: &HostSummary, base: &str, t: &I18n, now: f64) -> Element {
    div()
        .class("gg-head")
        .child(
            link(&format!("{base}/ui/hosts/{}", host.name))
                .class("gg-name")
                .text(host.name.clone()),
        )
        .child(state(t, host.up))
        .child(outage(host, now).map(|e| p().class("gg-sub").text(e)))
}

fn host_section(host: &HostSummary, base: &str, t: &I18n, now: f64) -> Element {
    div()
        .class(if host.up {
            "gg-section"
        } else {
            "gg-section gg-stale-block"
        })
        .child(host_title(host, base, t, now))
}

fn dash_or(v: Option<f64>, f: impl Fn(f64) -> String) -> String {
    v.map_or_else(|| DASH.to_string(), f)
}

fn used_pct(f: &gauge_core::Filesystem) -> f64 {
    if f.size > 0.0 {
        (f.size - f.avail) / f.size * 100.0
    } else {
        0.0
    }
}

fn bar(pct: f64) -> Element {
    div()
        .class(format!("gg-bar {}", Level::of(pct).class()))
        .child(span().css("width", move || format!("{:.1}%", pct.clamp(0.0, 100.0))))
}

/// A table cell with a small bar and its number; a dash when the host does not report it.
fn bar_cell(pct: Option<f64>) -> Element {
    element("td").child(match pct {
        Some(p) => div()
            .class("gg-cell")
            .child(bar(p))
            .child(span().text(format::pct(p))),
        None => div()
            .class("gg-cell")
            .child(span().class("gg-muted").text(DASH)),
    })
}

fn table(t: &I18n, headers: &[&str], body: Element) -> Element {
    element("table")
        .class("gg-table gg-table-fixed")
        .child(head(t, headers))
        .child(body)
}

// ── Storage ─────────────────────────────────────────────────────────────────────────────────────

/// How long until a filesystem fills, as the table says it.
fn full_in(t: &I18n, f: Option<&DiskForecast>) -> String {
    match f {
        None => DASH.to_string(),
        Some(f) => match (f.per_day, f.days_left) {
            (None, _) => DASH.to_string(),
            (Some(_), None) => t.t("ui_not_growing").to_string(),
            (Some(_), Some(d)) if d < 1.0 => t.t("ui_day_less").to_string(),
            (Some(_), Some(d)) => format!("{d:.0} {}", t.t("ui_days")),
        },
    }
}

fn growth(f: Option<&DiskForecast>) -> String {
    f.and_then(|f| f.per_day).map_or_else(
        || DASH.to_string(),
        |b| {
            let sign = if b < 0.0 { "−" } else { "+" };
            format!("{sign}{}/day", format::bytes(b.abs()))
        },
    )
}

fn forecast_of<'a>(
    list: &'a [DiskForecast],
    host: &str,
    f: &gauge_core::Filesystem,
) -> Option<&'a DiskForecast> {
    let mount = f.mount.as_deref().unwrap_or(&f.device);
    list.iter().find(|d| d.host == host && d.mount == mount)
}

/// Every filesystem of every host, the host with the fullest one first. `forecast` is what the stored history says
/// about how they fill; `None` while it has not come, or when the service has no history.
pub fn storage(
    t: &I18n,
    hosts: Option<&[HostCard]>,
    forecast: Option<&[DiskForecast]>,
    base: &str,
    now: f64,
) -> View {
    let forecast = forecast.unwrap_or(&[]);
    page(t, "storage", "ui_nav_storage", hosts, |hosts| {
        let mut known = known(hosts);
        known.sort_by(|a, b| {
            worst_disk(b.1)
                .unwrap_or(-1.0)
                .total_cmp(&worst_disk(a.1).unwrap_or(-1.0))
        });
        let (size, avail) = known
            .iter()
            .flat_map(|(_, v)| v.filesystems.iter())
            .fold((0.0, 0.0), |(s, a), f| (s + f.size, a + f.avail));
        // The first of equals, so the one that comes first by name.
        let fullest = known
            .iter()
            .flat_map(|(h, v)| v.filesystems.iter().map(move |f| (h, f)))
            .reduce(|a, b| if used_pct(b.1) > used_pct(a.1) { b } else { a });
        let mut out = div().class("gg-page").child(
            div()
                .class("gg-stats")
                .child(stat(t, "ui_storage_capacity", format::bytes(size)))
                .child(stat(t, "ui_storage_free", format::bytes(avail)))
                .child(
                    forecast
                        .iter()
                        .filter_map(|d| d.days_left.map(|left| (d, left)))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(d, _)| {
                            stat(
                                t,
                                "ui_storage_soonest",
                                format!("{} {}: {}", d.host, d.mount, full_in(t, Some(d))),
                            )
                        }),
                )
                .child(fullest.map(|(h, f)| {
                    stat(
                        t,
                        "ui_storage_fullest",
                        format!(
                            "{} {} {}",
                            h.name,
                            f.mount.as_deref().unwrap_or(&f.device),
                            format::pct(used_pct(f))
                        ),
                    )
                })),
        );
        for (host, view) in known {
            let fs = rows_with_bars(t, view, &host.name, forecast);
            let io = rows(view.disk_stats.iter().map(|d| {
                let ms = |v: Option<f64>| dash_or(v, |v| format!("{v:.1} ms"));
                vec![
                    d.name.clone(),
                    format::pct(d.util_pct),
                    format!("{:.0}", d.iops),
                    format!("{} / {}", ms(d.read_ms), ms(d.write_ms)),
                ]
            }));
            let mut sec = host_section(host, base, t, now).child(table(
                t,
                &[
                    "ui_col_device",
                    "ui_col_used",
                    "",
                    "ui_col_size",
                    "ui_col_growth",
                    "ui_col_full_in",
                ],
                fs,
            ));
            if !view.disk_stats.is_empty() {
                sec = sec.child(table(
                    t,
                    &[
                        "ui_col_device",
                        "ui_col_util",
                        "ui_col_iops",
                        "ui_col_latency",
                    ],
                    io,
                ));
            }
            for f in &view.faults {
                sec = sec.child(p().class("gg-alert gg-alert-crit").text(format!(
                    "{} — {}",
                    t.t("ui_alert_fault"),
                    format_fault(f)
                )));
            }
            out = out.child(sec);
        }
        out
    })
}

fn format_fault(f: &gauge_core::Fault) -> String {
    format!("{}: {}", f.what, f.count)
}

fn rows_with_bars(t: &I18n, view: &Metrics, host: &str, forecast: &[DiskForecast]) -> Element {
    let mut fs: Vec<_> = view.filesystems.iter().collect();
    fs.sort_by(|a, b| used_pct(b).total_cmp(&used_pct(a)));
    let mut body = element("tbody");
    for f in fs {
        let pct = used_pct(f);
        body = body.child(
            element("tr")
                .child(element("td").text(f.mount.clone().unwrap_or_else(|| f.device.clone())))
                .child(element("td").child(bar(pct)))
                .child(element("td").text(format::pct(pct)))
                .child(element("td").text(format!(
                    "{} / {}",
                    format::bytes(f.size - f.avail),
                    format::bytes(f.size)
                )))
                .child(element("td").text(growth(forecast_of(forecast, host, f))))
                .child(element("td").text(full_in(t, forecast_of(forecast, host, f)))),
        );
    }
    body
}

// ── Fleet ───────────────────────────────────────────────────────────────────────────────────────

fn hottest(view: &Metrics) -> Option<f64> {
    view.temps
        .iter()
        .map(|x| x.celsius)
        .chain(view.gpus.iter().filter_map(|g| g.temp_c))
        .reduce(f64::max)
}

/// One row per host, one bar per signal.
pub fn fleet(t: &I18n, hosts: Option<&[HostCard]>, base: &str, _now: f64) -> View {
    page(t, "fleet", "ui_nav_fleet", hosts, |hosts| {
        let mut body = element("tbody");
        for (host, view) in known(hosts) {
            let alerts = view.alerts.len();
            let name = link(&format!("{base}/ui/hosts/{}", host.name))
                .class("gg-name")
                .text(host.name.clone());
            body = body.child(
                element("tr")
                    .class(if host.up { "" } else { "gg-stale-row" })
                    .child(
                        element("td")
                            .child(name)
                            .child(span().text(" "))
                            .child(state(t, host.up)),
                    )
                    .child(bar_cell(view.cpu_pct))
                    .child(bar_cell(view.mem_pct()))
                    .child(bar_cell(
                        view.psi.as_ref().map(|p| p.mem.max(p.io).max(p.cpu)),
                    ))
                    .child(bar_cell(worst_disk(view)))
                    .child(bar_cell(busiest_gpu(view)))
                    .child(element("td").text(dash_or(hottest(view), |c| format!("{c:.0} °C"))))
                    .child(element("td").child(if alerts == 0 {
                        span().class("gg-muted").text(DASH)
                    } else {
                        span()
                            .class(if view.severe_alerts() > 0 {
                                "gg-badge gg-badge-crit"
                            } else {
                                "gg-badge gg-badge-warn"
                            })
                            .text(alerts.to_string())
                    })),
            );
        }
        section(t, "ui_nav_fleet", Some(t.t("ui_fleet_hint").to_string())).child(table(
            t,
            &[
                "ui_col_host",
                "ui_cpu",
                "ui_memory",
                "ui_pressure",
                "ui_disk",
                "ui_gpu",
                "ui_col_temp",
                "ui_alerts",
            ],
            body,
        ))
    })
}

// ── GPUs ────────────────────────────────────────────────────────────────────────────────────────

/// Every GPU of every host.
pub fn gpus(t: &I18n, hosts: Option<&[HostCard]>, base: &str, now: f64) -> View {
    page(t, "gpus", "ui_nav_gpus", hosts, |hosts| {
        let mut out = div().class("gg-page");
        let mut any = false;
        for (host, view) in known(hosts) {
            for gpu in &view.gpus {
                any = true;
                let vram = gpu
                    .vram_used
                    .zip(gpu.vram_total)
                    .filter(|(_, tot)| *tot > 0.0)
                    .map(|(u, tot)| u / tot * 100.0);
                let power = gpu
                    .power_w
                    .zip(gpu.power_cap_w)
                    .filter(|(_, c)| *c > 0.0)
                    .map(|(w, c)| w / c * 100.0);
                let facts = [
                    gpu.vram_used
                        .zip(gpu.vram_total)
                        .map(|(u, tot)| format!("{} / {}", format::bytes(u), format::bytes(tot))),
                    gpu.power_w.map(|w| {
                        format!(
                            "{w:.0} W{}",
                            gpu.power_cap_w
                                .map_or_else(String::new, |c| format!(" / {c:.0} W"))
                        )
                    }),
                    gpu.temp_c.map(|c| format!("{c:.0} °C")),
                    gpu.fan_rpm.map(|r| format!("{r:.0} rpm")),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                out = out.child(
                    host_section(host, base, t, now)
                        .child(
                            p().class("gg-sub")
                                .text(format!("{} · {}", gpu.name, facts)),
                        )
                        .child(meter(t, "ui_gpu_busy", gpu.busy_pct))
                        .child(meter(t, "ui_vram", vram))
                        .child(meter(t, "ui_power", power)),
                );
            }
        }
        if !any {
            out = out.child(
                p().attr("id", "no-gpus")
                    .class("fui-empty")
                    .text(t.t("ui_gpus_none")),
            );
        }
        out
    })
}

// ── Network ─────────────────────────────────────────────────────────────────────────────────────

/// Every interface of every host.
pub fn network(t: &I18n, hosts: Option<&[HostCard]>, base: &str, now: f64) -> View {
    page(t, "network", "ui_nav_network", hosts, |hosts| {
        let known = known(hosts);
        let sum = |f: fn(&gauge_core::Rate) -> f64| -> f64 {
            known.iter().flat_map(|(_, v)| v.nics.iter()).map(f).sum()
        };
        let mut out = div().class("gg-page").child(
            div()
                .class("gg-stats")
                .child(stat(t, "ui_col_rx", format::rate(sum(|r| r.a_per_sec))))
                .child(stat(t, "ui_col_tx", format::rate(sum(|r| r.b_per_sec)))),
        );
        for (host, view) in known {
            let body = rows(view.nics.iter().map(|r| {
                let link = view.links.iter().find(|l| l.name == r.name);
                vec![
                    r.name.clone(),
                    link.map_or_else(
                        || DASH.to_string(),
                        |l| match (l.up, l.speed) {
                            (false, _) => t.t("ui_link_down").to_string(),
                            (true, Some(s)) => format!("{:.0} Mb/s", s * 8.0 / 1e6),
                            (true, None) => t.t("ui_state_up").to_string(),
                        },
                    ),
                    format::rate(r.a_per_sec),
                    format::rate(r.b_per_sec),
                    link.map_or_else(
                        || DASH.to_string(),
                        |l| format!("{:.2}/s", l.errors_per_sec),
                    ),
                ]
            }));
            out = out.child(
                host_section(host, base, t, now)
                    .child(table(
                        t,
                        &[
                            "ui_col_device",
                            "ui_col_link",
                            "ui_col_rx",
                            "ui_col_tx",
                            "ui_col_errors",
                        ],
                        body,
                    ))
                    .child(view.tcp_retrans_per_sec.map(|v| {
                        p().class("gg-sub")
                            .text(format!("{} {v:.1}/s", t.t("ui_tcp_retrans")))
                    })),
            );
        }
        out
    })
}

// ── Power and thermals ──────────────────────────────────────────────────────────────────────────

/// Temperatures, fans and the watts the hosts report.
pub fn power(t: &I18n, hosts: Option<&[HostCard]>, base: &str, now: f64) -> View {
    page(t, "power", "ui_nav_power", hosts, |hosts| {
        let known = known(hosts);
        let gpu_w = |v: &Metrics| -> Option<f64> {
            let watts: Vec<f64> = v.gpus.iter().filter_map(|g| g.power_w).collect();
            (!watts.is_empty()).then(|| watts.iter().sum())
        };
        let total: f64 = known
            .iter()
            .filter(|(h, _)| h.up)
            .map(|(_, v)| v.cpu_watts.unwrap_or(0.0) + gpu_w(v).unwrap_or(0.0))
            .sum();
        let mut out = div().class("gg-page").child(
            div()
                .class("gg-stats")
                .child(stat(t, "ui_power_total", format!("{total:.0} W")))
                .child(p().class("gg-sub").text(t.t("ui_power_note"))),
        );
        for (host, view) in known {
            let mut sec = host_section(host, base, t, now).child(
                div()
                    .class("gg-stats")
                    .child(
                        view.cpu_watts
                            .map(|w| stat(t, "ui_power_cpu", format!("{w:.0} W"))),
                    )
                    .child(gpu_w(view).map(|w| stat(t, "ui_power_gpu", format!("{w:.0} W"))))
                    .child(hottest(view).map(|c| stat(t, "ui_hottest", format!("{c:.0} °C")))),
            );
            let mut sensors: Vec<(String, String)> = view
                .temps
                .iter()
                .map(|x| (x.label.clone(), format!("{:.0} °C", x.celsius)))
                .collect();
            sensors.extend(
                view.fans
                    .iter()
                    .map(|f| (f.label.clone(), format!("{:.0} rpm", f.rpm))),
            );
            sensors.extend(view.gpus.iter().filter_map(|g| {
                g.fan_rpm
                    .map(|r| (format!("{} fan", g.name), format!("{r:.0} rpm")))
            }));
            if !sensors.is_empty() {
                sec = sec.child(table(
                    t,
                    &["ui_col_name", "ui_col_speed"],
                    rows(sensors.into_iter().map(|(a, b)| vec![a, b])),
                ));
            }
            out = out.child(sec);
        }
        out
    })
}

// ── Alerts ──────────────────────────────────────────────────────────────────────────────────────

/// Everything that needs a look, across hosts: unreachable hosts and alerts, severe first. With the alert `log`, each
/// says how long it has been going, and the ones that ended are listed below.
pub fn alerts(
    t: &I18n,
    hosts: Option<&[HostCard]>,
    log: Option<&[AlertEvent]>,
    base: &str,
    now: f64,
) -> View {
    let Some(list) = hosts else {
        return page(t, "alerts", "ui_nav_alerts", None, |_| div());
    };
    let log = log.unwrap_or(&[]);
    let since = |host: &str, key: &str| {
        log.iter()
            .find(|e| e.closed.is_none() && e.host == host && e.key == key)
            .map(|e| {
                format!(
                    " · {} {}",
                    t.t("ui_alert_for"),
                    format::duration(now - e.opened)
                )
            })
            .unwrap_or_default()
    };
    // (severe, host, key, wording)
    let mut items: Vec<(bool, String, String, String)> = Vec::new();
    for card in list {
        let host = &card.host;
        if !host.up {
            items.push((
                true,
                host.name.clone(),
                "down".into(),
                format!(
                    "{} — {}",
                    t.t("ui_alert_down"),
                    outage(host, now).unwrap_or_default()
                ),
            ));
        } else if let Some(view) = &host.view {
            for a in &view.alerts {
                items.push((a.severe, host.name.clone(), a.key.clone(), alert_text(t, a)));
            }
        }
    }
    items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut page = div()
        .attr("id", "alerts-page")
        .class("gg-page")
        .child(element("h3").text(t.t("ui_nav_alerts")))
        .child(if items.is_empty() {
            p().attr("id", "all-clear")
                .class("fui-empty")
                .text(t.t("ui_alerts_none"))
        } else {
            let mut list = div().class("gg-alerts");
            for (severe, host, key, text) in items {
                let how_long = since(&host, &key);
                list = list.child(
                    p().class(if severe {
                        "gg-alert gg-alert-crit"
                    } else {
                        "gg-alert gg-alert-warn"
                    })
                    .child(
                        link(&format!("{base}/ui/hosts/{host}"))
                            .class("gg-name")
                            .text(host),
                    )
                    .child(span().text(format!("  {text}{how_long}"))),
                );
            }
            div().class("gg-section").child(list)
        });
    let ended: Vec<&AlertEvent> = log.iter().filter(|e| e.closed.is_some()).take(30).collect();
    if !ended.is_empty() {
        let mut rows = div().class("gg-log").attr("id", "alert-log");
        for e in ended {
            let closed = e.closed.unwrap_or(e.opened);
            rows = rows.child(
                div()
                    .class("gg-log-row")
                    .child(
                        link(&format!("{base}/ui/hosts/{}", e.host))
                            .class("gg-name")
                            .text(e.host.clone()),
                    )
                    .child(span().text(event_text(t, &e.kind, &e.detail)))
                    .child(span().class("gg-log-when").text(format!(
                        "{} · {} {}",
                        format::ago(now - closed),
                        t.t("ui_alert_lasted"),
                        format::duration(closed - e.opened)
                    ))),
            );
        }
        page = page.child(section(t, "ui_alert_log", None).child(rows));
    }
    page.into_view()
}
