//! One host: every metric Gauge has for it, with the last hour of the headline ones.

use super::{auto_refresh, meter, outage, state};
use crate::domain::model::{Gpu, View};
use crate::domain::store::{Host, Store};
use crate::routers::ui::common::chart::{self, Series};
use crate::routers::ui::common::{ActorOrRedirect, format, render_page, ui_path};
use forge_ui::empty_state;
use quench_http::prelude::{Inject, Path, Response, get, http::StatusCode};
use quench_web::prelude::*;

#[get("/ui/hosts/{name}")]
pub(super) async fn host_page(
    actor: ActorOrRedirect,
    Inject(store): Inject<Store>,
    Path(name): Path<String>,
) -> Response {
    if let Err(response) = actor.or_redirect() {
        return response;
    }
    let Some(host) = store.get(&name) else {
        return render_page(
            StatusCode::NOT_FOUND,
            content().child(empty_state("server", "ui_host_not_found", None)),
        );
    };
    render_page(
        StatusCode::OK,
        content().class("home-content").child(page(&host)),
    )
}

fn section(title_key: &str, aside: Option<String>) -> Element {
    div().class("gg-section").child(
        element("h4")
            .child(span().attr("data-i18n", title_key.to_string()))
            .child_opt(aside.map(|a| element("small").text(a))),
    )
}

fn series(
    host: &Host,
    f: impl Fn(&crate::domain::store::Point) -> Option<f64>,
) -> Vec<Option<f64>> {
    host.history.iter().map(f).collect()
}

fn legend(first: &str, second: &str) -> Element {
    div()
        .class("gg-legend")
        .child(
            span()
                .child(element("i"))
                .child(span().attr("data-i18n", first.to_string())),
        )
        .child(
            span()
                .child(element("i").class("gg-l2"))
                .child(span().attr("data-i18n", second.to_string())),
        )
}

fn stat(key: &str, value: String) -> Element {
    forge_ui::stat(forge_ui::t(key), forge_ui::txt(value))
}

fn page(host: &Host) -> Element {
    let mut page = div()
        .class(if host.up() {
            "home-container"
        } else {
            "home-container gg-stale"
        })
        .child(
            div()
                .class("gg-head")
                .child(
                    div()
                        .child(
                            a().attr("href", ui_path("/home"))
                                .class("gg-back")
                                .attr("data-i18n", "ui_back"),
                        )
                        .child(element("h3").text(host.name.clone()))
                        .child(p().class("gg-sub").text(format!(
                            "{} · {}",
                            host.kind.as_str(),
                            host.url
                        ))),
                )
                .child(state(host.up())),
        )
        .child_opt(outage(host).map(|e| p().class("gg-notice").text(e)));

    let Some(view) = host.view.as_ref() else {
        return page
            .child(empty_state("hourglass-half", "ui_waiting", None))
            .child(auto_refresh(5));
    };

    page = page
        .child(overview(host, view))
        .child(cpu_section(host, view))
        .child(memory_section(host, view));
    for gpu in &view.gpus {
        page = page.child(gpu_section(host, gpu));
    }
    page = page
        .child(disks_section(host, view))
        .child(network_section(host, view));
    if !view.temps.is_empty() {
        page = page.child(temps_section(view));
    }
    page.child(auto_refresh(10))
}

fn overview(host: &Host, view: &View) -> Element {
    div()
        .class("gg-stats")
        .child(stat(
            "ui_cpu",
            view.cpu_pct.map_or_else(|| "—".into(), format::pct),
        ))
        .child(stat(
            "ui_memory",
            view.mem_pct().map_or_else(|| "—".into(), format::pct),
        ))
        .child(stat(
            "ui_load",
            view.load
                .map_or_else(|| "—".into(), |l| format!("{:.2}", l[0])),
        ))
        .child(stat(
            "ui_uptime",
            view.uptime_secs
                .map_or_else(|| "—".into(), format::duration),
        ))
        .child_opt(
            host.view
                .as_ref()
                .and_then(|v| v.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max))
                .map(|g| stat("ui_gpu", format::pct(g))),
        )
}

fn cpu_section(host: &Host, view: &View) -> Element {
    let cpu = series(host, |p| p.cpu);
    let mut cores = div().class("gg-cores");
    for (id, pct) in &view.cores_pct {
        cores = cores.child(
            div()
                .class("gg-core")
                .child(span().text(format!("#{id}")))
                .child(
                    div()
                        .class(format!("gg-bar {}", format::level(*pct)))
                        .attr("title", format::pct(*pct))
                        .child(span().attr("style", format!("width:{pct:.1}%"))),
                ),
        );
    }
    let aside = view
        .load
        .map(|l| format!("load {:.2} · {:.2} · {:.2}", l[0], l[1], l[2]));
    section("ui_cpu", aside)
        .child(chart::line(
            &[Series {
                class: "gg-s1",
                values: &cpu,
            }],
            100.0,
            host.history.len(),
            "CPU",
        ))
        .child(cores)
}

fn memory_section(host: &Host, view: &View) -> Element {
    let mem = series(host, |p| p.mem);
    let aside = view
        .mem_total
        .zip(view.mem_used)
        .map(|(t, u)| format!("{} / {}", format::bytes(u), format::bytes(t)));
    let swap = view
        .swap_total
        .filter(|t| *t > 0.0)
        .zip(view.swap_used)
        .map(|(t, u)| meter("ui_swap", Some(u / t * 100.0)));
    section("ui_memory", aside)
        .child(chart::line(
            &[Series {
                class: "gg-s1",
                values: &mem,
            }],
            100.0,
            host.history.len(),
            "memory",
        ))
        .child_opt(swap)
}

fn gpu_section(host: &Host, gpu: &Gpu) -> Element {
    let busy = series(host, |p| p.gpu);
    let vram = series(host, |p| p.vram);
    let vram_pct = gpu
        .vram_used
        .zip(gpu.vram_total)
        .filter(|(_, t)| *t > 0.0)
        .map(|(u, t)| u / t * 100.0);
    let aside = [
        Some(gpu.name.clone()),
        gpu.vram_used
            .zip(gpu.vram_total)
            .map(|(u, t)| format!("{} / {}", format::bytes(u), format::bytes(t))),
        gpu.temp_c.map(|t| format!("{t:.0} °C")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    section("ui_gpu", Some(aside))
        .child(legend("ui_gpu_busy", "ui_vram"))
        .child(chart::line(
            &[
                Series {
                    class: "gg-s1",
                    values: &busy,
                },
                Series {
                    class: "gg-s2",
                    values: &vram,
                },
            ],
            100.0,
            host.history.len(),
            "GPU",
        ))
        .child(meter("ui_gpu_busy", gpu.busy_pct))
        .child(meter("ui_vram", vram_pct))
}

fn disks_section(host: &Host, view: &View) -> Element {
    let read = series(host, |p| p.read);
    let write = series(host, |p| p.write);
    let max = host
        .history
        .iter()
        .map(|p| p.read.unwrap_or(0.0).max(p.write.unwrap_or(0.0)))
        .fold(0.0, f64::max);

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
                            .class(format!("gg-bar {}", format::level(pct)))
                            .child(span().attr("style", format!("width:{pct:.1}%"))),
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
    let mut io = element("tbody");
    for r in &view.disks {
        io = io.child(
            element("tr")
                .child(element("td").text(r.name.clone()))
                .child(element("td").text(format::rate(r.a_per_sec)))
                .child(element("td").text(format::rate(r.b_per_sec))),
        );
    }
    section("ui_disk", None)
        .child(legend("ui_col_read", "ui_col_write"))
        .child(chart::line(
            &[
                Series {
                    class: "gg-s1",
                    values: &read,
                },
                Series {
                    class: "gg-s2",
                    values: &write,
                },
            ],
            max.max(1.0),
            host.history.len(),
            "disk throughput",
        ))
        .child(
            div()
                .class("gg-columns")
                .child(
                    element("table")
                        .class("gg-table")
                        .child(head(&["ui_col_device", "ui_col_used", "", "ui_col_size"]))
                        .child(fs),
                )
                .child(
                    element("table")
                        .class("gg-table")
                        .child(head(&["ui_col_device", "ui_col_read", "ui_col_write"]))
                        .child(io),
                ),
        )
}

fn network_section(host: &Host, view: &View) -> Element {
    let rx = series(host, |p| p.rx);
    let tx = series(host, |p| p.tx);
    let max = host
        .history
        .iter()
        .map(|p| p.rx.unwrap_or(0.0).max(p.tx.unwrap_or(0.0)))
        .fold(0.0, f64::max);
    let mut rows = element("tbody");
    for r in &view.nics {
        rows = rows.child(
            element("tr")
                .child(element("td").text(r.name.clone()))
                .child(element("td").text(format::rate(r.a_per_sec)))
                .child(element("td").text(format::rate(r.b_per_sec))),
        );
    }
    section("ui_network", None)
        .child(legend("ui_col_rx", "ui_col_tx"))
        .child(chart::line(
            &[
                Series {
                    class: "gg-s1",
                    values: &rx,
                },
                Series {
                    class: "gg-s2",
                    values: &tx,
                },
            ],
            max.max(1.0),
            host.history.len(),
            "network throughput",
        ))
        .child(
            element("table")
                .class("gg-table")
                .child(head(&["ui_col_device", "ui_col_rx", "ui_col_tx"]))
                .child(rows),
        )
}

fn temps_section(view: &View) -> Element {
    let mut rows = element("tbody");
    for t in &view.temps {
        rows = rows.child(
            element("tr")
                .child(element("td").text(t.label.clone()))
                .child(element("td").text(format!("{:.0} °C", t.celsius))),
        );
    }
    section("ui_temps", None).child(
        element("table")
            .class("gg-table")
            .child(head(&["ui_col_name", "ui_col_temp"]))
            .child(rows),
    )
}

fn head(keys: &[&str]) -> Element {
    let mut row = element("tr");
    for key in keys {
        row = if key.is_empty() {
            row.child(element("th"))
        } else {
            row.child(element("th").attr("data-i18n", (*key).to_string()))
        };
    }
    element("thead").child(row)
}

pub(super) fn register_routes() {
    let _ = host_page as fn(_, _, _) -> _;
}
