//! The entry point: every package, and every resource of every kind it put in the cluster.

use crate::domain::actions;
use crate::domain::deployments::{self, DeploymentView, Observed};
use crate::domain::resources::{Group, Row, State};
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, action, action_with, badge, notice_banner, render_page, row, table,
    tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Query, Response, get, http::StatusCode};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

#[get("/ui/home")]
pub(super) async fn home(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
) -> Response {
    render_home(actor, &gantry, &config, &notice).await
}

#[get("/ui/home/")]
pub(super) async fn home_slash(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
) -> Response {
    render_home(actor, &gantry, &config, &notice).await
}

fn status_name(group: &Group) -> String {
    serde_json::to_value(group.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn observed_name(observed: Observed) -> String {
    serde_json::to_value(observed)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn state_name(state: State) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The fields that name one resource in a form.
fn fields<'a>(package: &'a str, row: &'a Row) -> Vec<(&'a str, &'a str)> {
    vec![
        ("package", package),
        ("kind", row.kind.as_str()),
        ("name", row.name.as_str()),
        ("namespace", row.namespace.as_deref().unwrap_or("")),
        ("api_version", row.api_version.as_deref().unwrap_or("")),
    ]
}

async fn render_home(
    actor: ActorOrRedirect,
    gantry: &Gantry,
    config: &JwtConfig,
    notice: &Notice,
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
    // Declared deployments, shown under the package that declares them; a failure here is not worth hiding
    // the resources for.
    let deployment_views: Vec<DeploymentView> = deployments::all(gantry).await.unwrap_or_default();
    let busy: Vec<_> = gantry
        .store
        .list(10)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|op| !op.state.is_terminal())
        .collect();

    let visible: Vec<&Group> = groups
        .iter()
        .filter(|g| authz::can_on_target(Some(&claims), config, &g.package, "read"))
        .collect();

    let mut page = div()
        .class("home-container")
        .child(tabs("targets"))
        .child_opt(notice_banner(notice))
        .child_opt(problem.map(|m| p().class("gt-notice gt-notice-error").text(m)))
        .child_opt(busy.first().map(|op| {
            p().class("gt-notice gt-notice-info").child(
                a().attr("href", ui_path(&format!("/operations/{}", op.id)))
                    .text(format!("{} …", op.title)),
            )
        }));

    if visible.is_empty() {
        page = page.child(empty_state("ui_home_no_targets"));
    }

    for group in visible {
        let package = group.package.as_str();
        let may_scale = authz::can_on_target(Some(&claims), config, package, "scale");
        let may_deploy = authz::can_on_target(Some(&claims), config, package, "deploy");
        let missing = group
            .rows
            .iter()
            .filter(|r| r.state == State::Missing)
            .count();

        let mut bar = div().class("gt-group-actions");
        if let (true, Some(latest)) = (
            may_deploy && group.status == crate::domain::targets::Status::UpdateAvailable,
            &group.offered,
        ) {
            bar = bar.child(action_with(
                &format!("/targets/{package}/sync"),
                "ui_action_upgrade",
                "primary",
                &[("version", latest.as_str())],
                Some(&format!("Upgrade {package} to {latest}?")),
            ));
        }
        if may_scale && missing > 0 {
            bar = bar.child(action_with(
                &format!("/packages/{package}/apply"),
                "ui_action_apply_missing",
                "",
                &[],
                None,
            ));
        }
        if !group.inventoried && group.installed.is_some() {
            bar = bar.child(action_with(
                &format!("/packages/{package}/refresh"),
                "ui_action_refresh",
                "",
                &[],
                None,
            ));
        }

        let mine: Vec<&DeploymentView> = deployment_views
            .iter()
            .filter(|d| d.declared && d.target == package)
            .collect();
        let mut deps = div().class("gt-deps");
        for view in &mine {
            let up = view.observed == Observed::Running;
            let installed = view.observed != Observed::Absent;
            let mut buttons = div().class("gt-right");
            if may_scale && installed {
                if up || view.observed == Observed::Partial {
                    buttons = buttons.child(action_with(
                        &format!("/deployments/{}/stop", view.name),
                        "ui_action_stop",
                        "",
                        &[],
                        Some(&format!("Stop {}? Its workloads are deleted.", view.name)),
                    ));
                }
                if !up {
                    buttons = buttons.child(action(
                        &format!("/deployments/{}/start", view.name),
                        "ui_action_start",
                        "",
                    ));
                }
            }
            deps = deps.child(
                div()
                    .class("gt-dep")
                    .child(badge("observed", observed_name(view.observed).as_str()))
                    .child(strong().text(view.name.clone()))
                    .child_opt(view.drift.then(|| {
                        span()
                            .class("gt-desc")
                            .child(span().attr("data-i18n", "ui_expected"))
                            .child(span().text(format!(" {}", view.desired)))
                    }))
                    .child_opt((!view.conflicts_with.is_empty()).then(|| {
                        span()
                            .class("gt-desc")
                            .text(format!("⇄ {}", view.conflicts_with.join(", ")))
                    }))
                    .child(buttons),
            );
        }

        let mut list = table(&[
            "ui_col_kind",
            "ui_col_name",
            "ui_col_namespace",
            "ui_col_status",
            "",
        ]);
        for resource in &group.rows {
            let mut buttons = div().class("gt-right");
            let state = resource.state;
            if may_scale && state == State::Missing {
                buttons = buttons.child(action_with(
                    "/resources/apply",
                    "ui_action_apply",
                    "",
                    &fields(package, resource),
                    None,
                ));
            }
            if may_deploy && resource.editable {
                buttons = buttons.child(
                    a().attr(
                        "href",
                        ui_path(&format!("/resource?{}", query(&fields(package, resource)))),
                    )
                    .class("gt-btn")
                    .attr("data-i18n", "ui_action_edit"),
                );
            }
            if may_scale && !matches!(state, State::Missing | State::Hidden) {
                buttons = buttons.child(action_with(
                    "/resources/delete",
                    "ui_action_delete",
                    "danger",
                    &fields(package, resource),
                    Some(&format!("Delete {}/{}?", resource.kind, resource.name)),
                ));
            }

            let ready = resource
                .ready
                .clone()
                .map(|r| span().class("gt-desc gt-gap").text(r));
            list = list.child(row(vec![
                span().class("gt-muted").text(resource.kind.clone()),
                span().text(resource.name.clone()),
                span()
                    .class("gt-muted")
                    .text(resource.namespace.clone().unwrap_or_default()),
                div()
                    .child(badge("rstate", &state_name(state)))
                    .child_opt(ready),
                buttons,
            ]));
        }

        page = page.child(
            div()
                .class("gt-group")
                .child(
                    div()
                        .class("gt-group-head")
                        .child(
                            div()
                                .class("gt-group-title")
                                .child(
                                    a().attr("href", ui_path(&format!("/targets/{package}")))
                                        .child(strong().text(package.to_string())),
                                )
                                .child(span().class("gt-muted").text(format!(
                                    "{}{}",
                                    group.installed.clone().unwrap_or_else(|| "—".to_string()),
                                    group
                                        .offered
                                        .as_ref()
                                        .filter(|o| Some(*o) != group.installed.as_ref())
                                        .map(|o| format!(" → {o}"))
                                        .unwrap_or_default()
                                )))
                                .child(badge("status", &status_name(group))),
                        )
                        .child(bar),
                )
                .child_opt((!mine.is_empty()).then_some(deps))
                .child(if group.rows.is_empty() {
                    p().class("gt-muted")
                        .attr("data-i18n", "ui_target_no_units")
                } else {
                    list
                }),
        );
    }

    render_page(StatusCode::OK, content().class("home-content").child(page))
}

/// `a=b&c=d`, percent-encoded, for a link that carries a resource.
fn query(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| {
            format!(
                "{key}={}",
                crate::routers::ui::common::percent_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _, _, _) -> _;
    let _ = home_slash as fn(_, _, _, _) -> _;
}
