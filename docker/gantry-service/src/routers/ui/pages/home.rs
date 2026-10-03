//! The entry point: one card per package (an application), each with whether the cluster is what the
//! package says it should be. The resources are one click in, on the application's own page.

use crate::domain::actions;
use crate::domain::resources::{Group, Summary, SyncState};
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, badge, notice_banner, percent_encode, render_page, tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Query, Response, get, http::StatusCode};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub(super) struct Filter {
    /// A sync state (`out_of_sync`, `missing`, ...), or empty for every application.
    #[serde(default)]
    pub sync: String,
    #[serde(default)]
    pub q: String,
}

#[get("/ui/home")]
pub(super) async fn home(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
    Query(filter): Query<Filter>,
) -> Response {
    render_home(actor, &gantry, &config, &notice, &filter).await
}

#[get("/ui/home/")]
pub(super) async fn home_slash(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
    Query(filter): Query<Filter>,
) -> Response {
    render_home(actor, &gantry, &config, &notice, &filter).await
}

/// The applications to show: the ones that pass the filter, those needing attention first.
fn matching<'a>(
    apps: &'a [(&'a Group, Summary)],
    filter: &Filter,
) -> Vec<&'a (&'a Group, Summary)> {
    let query = filter.q.trim().to_ascii_lowercase();
    let mut found: Vec<_> = apps
        .iter()
        .filter(|(_, s)| filter.sync.is_empty() || s.sync.as_str() == filter.sync)
        .filter(|(g, _)| query.is_empty() || g.package.to_ascii_lowercase().contains(&query))
        .collect();
    found.sort_by_key(|(g, s)| (s.sync == SyncState::Synced, g.package.clone()));
    found
}

fn card(group: &Group, summary: &Summary) -> Element {
    let package = group.package.as_str();
    let version = group.installed.clone().unwrap_or_else(|| "—".to_string());

    let mut notes = div().class("gt-app-notes");
    for (n, key) in [
        (summary.missing, "ui_rstate_missing"),
        (summary.edited, "ui_rstate_edited"),
        (summary.extra, "ui_rstate_extra"),
    ] {
        if n > 0 {
            notes = notes.child(
                span()
                    .class("gt-note")
                    .child(span().text(format!("{n} ")))
                    .child(span().attr("data-i18n", key)),
            );
        }
    }
    if summary.not_ready > 0 {
        notes = notes.child(
            span()
                .class("gt-note")
                .child(span().text(format!("{} ", summary.not_ready)))
                .child(span().attr("data-i18n", "ui_app_not_ready")),
        );
    }

    a().attr("href", ui_path(&format!("/apps/{package}")))
        .class("gt-app")
        .child(
            div()
                .class("gt-app-head")
                .child(strong().class("gt-app-name").text(package.to_string()))
                .child(badge("sync", summary.sync.as_str())),
        )
        .child(
            p().class("gt-app-version")
                .child(span().text(version))
                .child_opt(
                    summary
                        .update_to
                        .as_ref()
                        .map(|latest| span().class("gt-update").text(format!(" → {latest}"))),
                ),
        )
        .child_opt(
            group
                .description
                .clone()
                .map(|text| p().class("gt-desc gt-app-desc").text(text)),
        )
        .child(
            div()
                .class("gt-app-foot")
                .child(
                    span()
                        .class("gt-muted")
                        .child(span().text(format!("{} ", summary.total)))
                        .child(span().attr("data-i18n", "ui_app_resources")),
                )
                .child(notes),
        )
}

/// A link that narrows the list to one sync state.
fn chip(state: &str, label_key: &str, count: usize, active: bool) -> Element {
    let href = if state.is_empty() {
        ui_path("/home")
    } else {
        ui_path(&format!("/home?sync={}", percent_encode(state)))
    };
    a().attr("href", href)
        .class(if active {
            "gt-chip gt-chip-active"
        } else {
            "gt-chip"
        })
        .child(span().attr("data-i18n", label_key.to_string()))
        .child(span().class("gt-muted").text(format!(" {count}")))
}

async fn render_home(
    actor: ActorOrRedirect,
    gantry: &Gantry,
    config: &JwtConfig,
    notice: &Notice,
    filter: &Filter,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    // Warehouse or the cluster being unreachable is shown, not hidden behind an empty list.
    let (groups, problem) = match actions::groups(gantry).await {
        Ok(groups) => (groups, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let busy: Vec<_> = gantry
        .store
        .list(10)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|op| !op.state.is_terminal())
        .collect();

    let apps: Vec<(&Group, Summary)> = groups
        .iter()
        .filter(|g| authz::can_on_target(Some(&claims), config, &g.package, "read"))
        .map(|g| (g, g.summary()))
        .collect();
    let count = |state: SyncState| apps.iter().filter(|(_, s)| s.sync == state).count();
    let shown = matching(&apps, filter);

    let mut chips = div().class("gt-chips").child(chip(
        "",
        "ui_filter_all",
        apps.len(),
        filter.sync.is_empty(),
    ));
    for (state, key) in [
        (SyncState::Synced, "ui_sync_synced"),
        (SyncState::OutOfSync, "ui_sync_out_of_sync"),
        (SyncState::Missing, "ui_sync_missing"),
        (SyncState::NotInstalled, "ui_sync_not_installed"),
        (SyncState::Unlisted, "ui_sync_unlisted"),
    ] {
        let n = count(state);
        if n > 0 || filter.sync == state.as_str() {
            chips = chips.child(chip(state.as_str(), key, n, filter.sync == state.as_str()));
        }
    }

    let mut grid = div().class("gt-grid");
    for (group, summary) in &shown {
        grid = grid.child(card(group, summary));
    }

    let page = div()
        .class("home-container")
        .child(tabs("targets"))
        .child_opt(notice_banner(notice))
        .child_opt(problem.map(|m| p().class("gt-notice gt-notice-error").text(m)))
        .child_opt(busy.first().map(|op| {
            p().class("gt-notice gt-notice-info").child(
                a().attr("href", ui_path(&format!("/operations/{}", op.id)))
                    .text(format!("{} …", op.title)),
            )
        }))
        .child(
            form()
                .attr("method", "get")
                .attr("action", ui_path("/home"))
                .class("gt-form gt-inline-form gt-filter")
                .child(chips)
                .child(
                    input()
                        .attr("type", "text")
                        .attr("name", "q")
                        .attr("placeholder", "name")
                        .attr("value", filter.q.clone()),
                )
                .child_opt((!filter.sync.is_empty()).then(|| {
                    input()
                        .attr("type", "hidden")
                        .attr("name", "sync")
                        .attr("value", filter.sync.clone())
                })),
        )
        .child(if apps.is_empty() {
            empty_state("ui_home_no_targets")
        } else if shown.is_empty() {
            p().class("gt-muted")
                .attr("data-i18n", "ui_filter_no_match")
        } else {
            grid
        });

    render_page(StatusCode::OK, content().class("home-content").child(page))
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _, _, _, _) -> _;
    let _ = home_slash as fn(_, _, _, _, _) -> _;
}
