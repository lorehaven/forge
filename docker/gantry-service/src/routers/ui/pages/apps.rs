//! One application: a package, what it declares, and what is in the cluster of it. The resources can be
//! filtered by kind, state and name; the actions on the whole package sit above them, the actions on one
//! resource beside it.

use crate::domain::actions;
use crate::domain::deployments::{self, DeploymentView, Observed};
use crate::domain::resources::{Row, State};
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, action, action_with, badge, notice_banner, percent_encode, redirect,
    render_page, row, table, tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_web::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub(super) struct Filter {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub q: String,
}

pub(super) fn observed_name(observed: Observed) -> String {
    serde_json::to_value(observed)
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

/// `a=b&c=d`, percent-encoded, for a link that carries a resource.
fn query(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The page with a filter applied: a link to the same application narrowed to `state`.
fn state_link(package: &str, state: &str) -> String {
    ui_path(&format!("/apps/{package}?state={}", percent_encode(state)))
}

#[get("/ui/apps/{name}")]
pub(super) async fn show(
    Path(package): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
    Query(filter): Query<Filter>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &package, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    let groups = match actions::groups(&gantry).await {
        Ok(groups) => groups,
        Err(error) => return redirect("/home", Some(("error", &error.to_string()))),
    };
    let Some(group) = groups.iter().find(|g| g.package == package) else {
        return redirect("/home", Some(("error", "no such package")));
    };
    let summary = group.summary();
    let package = package.as_str();

    let may_scale = authz::can_on_target(Some(&claims), &config, package, "scale");
    let may_deploy = authz::can_on_target(Some(&claims), &config, package, "deploy");

    let mut bar = div().class("gt-group-actions");
    if let (true, Some(latest)) = (may_deploy, &summary.update_to) {
        bar = bar.child(action_with(
            &format!("/targets/{package}/sync"),
            "ui_action_upgrade",
            "primary",
            &[("version", latest.as_str())],
            Some(&format!("Upgrade {package} to {latest}?")),
        ));
    }
    if may_scale && summary.missing > 0 {
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
    bar = bar.child(
        a().attr("href", ui_path(&format!("/targets/{package}")))
            .class("gt-btn")
            .attr("data-i18n", "ui_action_versions"),
    );

    let views: Vec<DeploymentView> = deployments::all(&gantry).await.unwrap_or_default();
    let declared: Vec<&DeploymentView> = views
        .iter()
        .filter(|d| d.declared && d.target == package)
        .collect();
    let mut deps = div().class("gt-deps");
    for view in &declared {
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
                .child_opt((!view.conflicts_with.is_empty()).then(|| {
                    span()
                        .class("gt-desc")
                        .text(format!("⇄ {}", view.conflicts_with.join(", ")))
                }))
                .child(buttons),
        );
    }

    // What the filter offers, and what it leaves.
    let mut kind_select = select().attr("name", "kind").child(
        option()
            .attr("value", "")
            .attr("data-i18n", "ui_filter_all_kinds"),
    );
    for kind in group.kinds() {
        let mut opt = option().attr("value", kind.as_str()).text(kind.clone());
        if kind.eq_ignore_ascii_case(&filter.kind) {
            opt = opt.attr("selected", "true");
        }
        kind_select = kind_select.child(opt);
    }
    let mut state_select = select().attr("name", "state").child(
        option()
            .attr("value", "")
            .attr("data-i18n", "ui_filter_all_states"),
    );
    for state in [
        State::Synced,
        State::Edited,
        State::Missing,
        State::Extra,
        State::Hidden,
    ] {
        let mut opt = option()
            .attr("value", state.as_str())
            .attr("data-i18n", format!("ui_rstate_{}", state.as_str()));
        if state.as_str() == filter.state {
            opt = opt.attr("selected", "true");
        }
        state_select = state_select.child(opt);
    }
    let filtering =
        !(filter.kind.is_empty() && filter.state.is_empty() && filter.q.trim().is_empty());

    let shown = group.filtered(&filter.kind, &filter.state, &filter.q);
    let mut list = table(&[
        "ui_col_kind",
        "ui_col_name",
        "ui_col_namespace",
        "ui_col_status",
        "",
    ]);
    for resource in &shown {
        let state = resource.state;
        let mut buttons = div().class("gt-right");
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
        list = list.child(row(vec![
            span().class("gt-muted").text(resource.kind.clone()),
            span().text(resource.name.clone()),
            span()
                .class("gt-muted")
                .text(resource.namespace.clone().unwrap_or_default()),
            div().child(badge("rstate", state.as_str())).child_opt(
                resource
                    .ready
                    .clone()
                    .map(|r| span().class("gt-desc gt-gap").text(r)),
            ),
            buttons,
        ]));
    }

    // Counts that are also the quickest filters.
    let mut counts = div().class("gt-counts");
    for (state, n) in [
        (State::Synced, summary.synced),
        (State::Edited, summary.edited),
        (State::Missing, summary.missing),
        (State::Extra, summary.extra),
    ] {
        if n == 0 {
            continue;
        }
        counts = counts.child(
            a().attr("href", state_link(package, state.as_str()))
                .class("gt-count")
                .child(badge("rstate", state.as_str()))
                .child(span().class("gt-muted").text(format!(" {n}"))),
        );
    }

    let versions = format!(
        "{}{}",
        group.installed.clone().unwrap_or_else(|| "—".to_string()),
        summary
            .update_to
            .as_ref()
            .map(|o| format!(" → {o}"))
            .unwrap_or_default()
    );

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(tabs("targets"))
                .child_opt(notice_banner(&notice))
                .child(
                    p().class("gt-crumb")
                        .child(
                            a().attr("href", ui_path("/home"))
                                .attr("data-i18n", "ui_nav_targets"),
                        )
                        .child(span().text(format!(" / {package}"))),
                )
                .child(
                    div()
                        .class("gt-group-head")
                        .child(
                            div()
                                .class("gt-group-title")
                                .child(strong().class("gt-app-name").text(package.to_string()))
                                .child(badge("sync", summary.sync.as_str()))
                                .child(span().class("gt-muted").text(versions)),
                        )
                        .child(bar),
                )
                .child_opt(
                    group
                        .description
                        .clone()
                        .map(|text| p().class("gt-desc").text(text)),
                )
                .child(counts)
                .child_opt((!declared.is_empty()).then_some(deps))
                .child(
                    form()
                        .attr("method", "get")
                        .attr("action", ui_path(&format!("/apps/{package}")))
                        .class("gt-form gt-inline-form gt-filter")
                        .child(
                            input()
                                .attr("type", "text")
                                .attr("name", "q")
                                .attr("placeholder", "name")
                                .attr("value", filter.q.clone()),
                        )
                        .child(kind_select)
                        .child(state_select)
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("gt-btn")
                                .attr("data-i18n", "ui_action_filter"),
                        )
                        .child_opt(filtering.then(|| {
                            a().attr("href", ui_path(&format!("/apps/{package}")))
                                .class("gt-btn")
                                .attr("data-i18n", "ui_action_clear")
                        }))
                        .child(span().class("gt-muted").text(format!(
                            "{} / {}",
                            shown.len(),
                            group.rows.len()
                        ))),
                )
                .child(if group.rows.is_empty() {
                    p().class("gt-muted")
                        .attr("data-i18n", "ui_target_no_units")
                } else if shown.is_empty() {
                    p().class("gt-muted")
                        .attr("data-i18n", "ui_filter_no_match")
                } else {
                    list
                }),
        ),
    )
}

pub(super) fn register_routes() {
    let _ = show as fn(_, _, _, _, _, _) -> _;
}
