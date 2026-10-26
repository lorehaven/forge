//! The entry point: one card per host with its headline numbers.

use super::{auto_refresh, meter, outage, state};
use crate::domain::store::{Host, Store};
use crate::routers::ui::common::chart::{self, Series};
use crate::routers::ui::common::{ActorOrRedirect, format, render_page, ui_path};
use forge_ui::empty_state;
use quench_http::prelude::{Inject, Response, get, http::StatusCode};
use quench_web::prelude::*;

#[get("/ui/home")]
pub(super) async fn home(actor: ActorOrRedirect, Inject(store): Inject<Store>) -> Response {
    render_home(actor, &store)
}

#[get("/ui/home/")]
pub(super) async fn home_slash(actor: ActorOrRedirect, Inject(store): Inject<Store>) -> Response {
    render_home(actor, &store)
}

fn card(host: &Host) -> Element {
    let view = host.view.as_ref();
    let mut sub = format!("{} · {}", host.kind.as_str(), host.url);
    if let Some(cores) = view.map(|v| v.cores_pct.len()).filter(|n| *n > 0) {
        sub = format!("{cores} cores · {sub}");
    }

    let cpu: Vec<Option<f64>> = host.history.iter().map(|p| p.cpu).collect();
    let worst_disk = view.and_then(|v| {
        v.filesystems
            .iter()
            .filter(|f| f.size > 0.0)
            .map(|f| (f.size - f.avail) / f.size * 100.0)
            .reduce(f64::max)
    });
    let gpu = view.and_then(|v| v.gpus.iter().filter_map(|g| g.busy_pct).reduce(f64::max));
    let net = view.map(|v| {
        (
            v.nics.iter().map(|r| r.a_per_sec).sum::<f64>(),
            v.nics.iter().map(|r| r.b_per_sec).sum::<f64>(),
        )
    });

    a().attr("href", ui_path(&format!("/hosts/{}", host.name)))
        .class(if host.up() {
            "gg-host"
        } else {
            "gg-host gg-stale"
        })
        .child(
            div()
                .class("gg-head")
                .child(strong().class("gg-name").text(host.name.clone()))
                .child(state(host.up())),
        )
        .child(p().class("gg-sub").text(sub))
        .child_opt(outage(host).map(|e| p().class("gg-notice").text(e)))
        .child(meter("ui_cpu", view.and_then(|v| v.cpu_pct)))
        .child(meter(
            "ui_memory",
            view.and_then(crate::domain::model::View::mem_pct),
        ))
        .child_opt(
            view.and_then(|v| v.gpus.first())
                .map(|_| meter("ui_gpu", gpu)),
        )
        .child(meter("ui_disk", worst_disk))
        .child(chart::line(
            &[Series {
                class: "gg-s1",
                values: &cpu,
            }],
            100.0,
            host.history.len(),
            "CPU",
        ))
        .child(
            div()
                .class("gg-foot")
                .child(
                    span()
                        .child(span().attr("data-i18n", "ui_load"))
                        .child(span().text(view.and_then(|v| v.load).map_or_else(
                            || " —".to_string(),
                            |l| format!(" {:.2} {:.2} {:.2}", l[0], l[1], l[2]),
                        ))),
                )
                .child(span().text(net.map_or_else(String::new, |(rx, tx)| {
                    format!("↓ {} ↑ {}", format::rate(rx), format::rate(tx))
                }))),
        )
}

fn render_home(actor: ActorOrRedirect, store: &Store) -> Response {
    if let Err(response) = actor.or_redirect() {
        return response;
    }
    let mut hosts = store.all();
    hosts.sort_by(|a, b| a.name.cmp(&b.name));

    let body = if hosts.is_empty() {
        empty_state("server", "ui_hosts_none", None)
    } else {
        let mut grid = div().class("gg-grid-hosts");
        for host in &hosts {
            grid = grid.child(card(host));
        }
        grid
    };
    let page = div()
        .class("home-container")
        .child(body)
        .child(auto_refresh(10));
    render_page(StatusCode::OK, content().class("home-content").child(page))
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _) -> _;
    let _ = home_slash as fn(_, _) -> _;
}
