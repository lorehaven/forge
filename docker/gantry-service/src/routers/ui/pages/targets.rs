//! One target: what is running of it, what is published, what was set by hand, and the way to plan a change.

use crate::domain::planner;
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, badge, cell_text, notice_banner, redirect, render_page, row, table,
    tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;
use serde::Deserialize;

fn forbidden(name: &str) -> Response {
    redirect(&format!("/targets/{name}"), Some(("error", "not allowed")))
}

#[get("/ui/targets/{name}")]
pub(super) async fn show(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &name, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }

    let target = match planner::describe(&gantry, &name).await {
        Ok(Some(target)) => target,
        Ok(None) => return redirect("/home", Some(("error", "no such target"))),
        Err(error) => return redirect("/home", Some(("error", &error.to_string()))),
    };
    let versions = gantry.registry.versions(&name).await.unwrap_or_default();

    let status = serde_json::to_value(target.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();

    let mut units = table(&[
        "ui_col_kind",
        "ui_col_name",
        "ui_col_version",
        "ui_col_ready",
    ]);
    for unit in &target.units {
        units = units.child(row(vec![
            span().class("gt-muted").text(unit.kind.clone()),
            span().text(format!("{}/{}", unit.namespace, unit.name)),
            cell_text(unit.version.clone().unwrap_or_else(|| "—".to_string())),
            cell_text(format!("{}/{}", unit.ready, unit.desired)),
        ]));
    }

    let mut version_select = select()
        .attr("name", "version")
        .attr("id", "gt-version")
        .child(
            option()
                .attr("value", "")
                .attr("data-i18n", "ui_field_version_newest"),
        );
    let mut published = table(&["ui_col_version", "ui_col_description", ""]);
    for version in &versions {
        let mut opt = option()
            .attr("value", &version.version)
            .text(version.version.clone());
        if version.yanked {
            opt = opt.attr("disabled", "true");
        }
        version_select = version_select.child(opt);
        published = published.child(row(vec![
            cell_text(&version.version),
            span()
                .class("gt-muted")
                .text(version.description.clone().unwrap_or_default()),
            if version.yanked {
                span()
                    .class("gt-yanked")
                    .attr("data-i18n", "ui_version_yanked")
            } else {
                span()
            },
        ]));
    }

    let may_plan = authz::can_on_target(Some(&claims), &config, &name, "read");

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(tabs("targets"))
                .child_opt(notice_banner(&notice))
                .child(
                    div()
                        .class("home-header")
                        .child(h3().text(target.name.clone()))
                        .child(badge("status", &status)),
                )
                .child_opt(
                    target
                        .description
                        .clone()
                        .map(|text| p().class("gt-desc").text(text)),
                )
                .child(p().class("gt-meta").text(format!(
                    "{} → {}",
                    target.installed.clone().unwrap_or_else(|| "—".to_string()),
                    target.offered.clone().unwrap_or_else(|| "—".to_string())
                )))
                .child_opt(may_plan.then(|| {
                    form()
                        .attr("method", "post")
                        .attr("action", ui_path(&format!("/targets/{name}/sync")))
                        .class("gt-form gt-inline-form")
                        .child(version_select)
                        .child(
                            button()
                                .attr("type", "submit")
                                .class("gt-btn gt-btn-primary")
                                .attr("data-i18n", "ui_action_sync"),
                        )
                }))
                .child(
                    div()
                        .class("gt-section")
                        .attr("data-i18n", "ui_target_units"),
                )
                .child(if target.units.is_empty() {
                    p().class("gt-muted")
                        .attr("data-i18n", "ui_target_no_units")
                } else {
                    units
                })
                .child(
                    element("details")
                        .class("gt-details")
                        .child(
                            element("summary")
                                .child(span().attr("data-i18n", "ui_target_versions"))
                                .child(span().text(format!(" ({})", versions.len()))),
                        )
                        .child(published),
                ),
        ),
    )
}

#[derive(Deserialize)]
pub(super) struct SyncForm {
    #[serde(default)]
    pub version: String,
}

/// Put the package at a version (the newest if none is named), now. Going to an older one is a rollback,
/// which is its own permission.
#[post("/ui/targets/{name}/sync")]
pub(super) async fn sync(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<SyncForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let version = (!form.version.trim().is_empty()).then(|| form.version.trim().to_string());
    let prepared = match planner::prepare(&gantry, &name, version.as_deref()).await {
        Ok(prepared) => prepared,
        Err(error) => {
            return redirect(
                &format!("/targets/{name}"),
                Some(("error", &error.to_string())),
            );
        }
    };
    if !authz::can_on_target(Some(&claims), &config, &name, prepared.action.permission()) {
        return forbidden(&name);
    }
    match prepared.run(&gantry, &claims.sub).await {
        Ok(operation) => redirect(&format!("/operations/{}", operation.id), None),
        Err(error) => redirect(
            &format!("/targets/{name}"),
            Some(("error", &error.to_string())),
        ),
    }
}

pub(super) fn register_routes() {
    let _ = show as fn(_, _, _, _, _) -> _;
    let _ = sync as fn(_, _, _, _, _) -> _;
}
