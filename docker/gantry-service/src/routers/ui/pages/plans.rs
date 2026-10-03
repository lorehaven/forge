//! A plan: what would happen, said plainly, with the mechanical steps one click away.

use crate::domain::planner;
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, notice_banner, redirect, render_page, tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;

#[get("/ui/plans/{id}")]
pub(super) async fn show(
    Path(id): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let Ok(Some(stored)) = gantry.store.plan(&id).await else {
        return redirect("/home", Some(("error", "no such plan")));
    };
    if !authz::can_on_target(Some(&claims), &config, &stored.target, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    let may_confirm = authz::can_confirm(Some(&claims), &config, &stored).is_ok();

    // A start, stop or swap belongs under Deployments; everything else is a change to a package.
    let (tab, back) = if stored.plan.deployments.is_empty() {
        ("targets", format!("/targets/{}", stored.target))
    } else {
        ("targets", "/home".to_string())
    };

    let mut summary = ul();
    for line in &stored.plan.summary {
        summary = summary.child(li().text(line.clone()));
    }
    let mut steps = element("ol").class("gt-list");
    for step in &stored.plan.steps {
        steps = steps.child(li().text(step.describe()));
    }

    let confirmed = stored
        .operation_id
        .as_deref()
        .filter(|op| *op != crate::domain::operation::CLAIMING);

    let bar = match confirmed {
        Some(operation) => div().class("gt-bar").child(
            a().attr("href", ui_path(&format!("/operations/{operation}")))
                .attr("data-i18n", "ui_plan_confirmed"),
        ),
        None if may_confirm => div()
            .class("gt-bar gt-bar-end")
            .child(
                a().attr("href", ui_path(&back))
                    .class("gt-btn")
                    .attr("data-i18n", "ui_back"),
            )
            .child(
                form()
                    .attr("method", "post")
                    .attr("action", ui_path(&format!("/plans/{id}/confirm")))
                    .class("gt-inline")
                    .child(
                        button()
                            .attr("type", "submit")
                            .class("gt-btn gt-btn-primary")
                            .attr("data-i18n", "ui_plan_confirm"),
                    ),
            ),
        None => div().class("gt-bar").child(
            p().class("gt-muted")
                .attr("data-i18n", "ui_plan_not_allowed"),
        ),
    };

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(tabs(tab))
                .child_opt(notice_banner(&notice))
                .child(
                    p().class("gt-crumb")
                        .child(a().attr("href", ui_path(&back)).text(stored.target.clone())),
                )
                .child(
                    div()
                        .class("home-header")
                        .child(h3().attr("data-i18n", format!("ui_action_{}", stored.action)))
                        .child(
                            span()
                                .class("gt-muted")
                                .text(stored.version.clone().unwrap_or_default()),
                        ),
                )
                .child(div().class("gt-card").child(summary))
                .child(
                    element("details")
                        .class("gt-details")
                        .child(
                            element("summary")
                                .child(span().attr("data-i18n", "ui_plan_steps"))
                                .child(span().text(format!(" ({})", stored.plan.steps.len()))),
                        )
                        .child(steps),
                )
                .child(bar),
        ),
    )
}

#[post("/ui/plans/{id}/confirm")]
pub(super) async fn confirm(
    Path(id): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    let Ok(Some(stored)) = gantry.store.plan(&id).await else {
        return redirect("/home", Some(("error", "no such plan")));
    };
    if authz::can_confirm(Some(&claims), &config, &stored).is_err() {
        return redirect(&format!("/plans/{id}"), Some(("error", "not allowed")));
    }

    match planner::confirm(&gantry, &id, &claims.sub).await {
        Ok(operation) => redirect(&format!("/operations/{}", operation.id), None),
        Err(error) => redirect(&format!("/plans/{id}"), Some(("error", &error.to_string()))),
    }
}

pub(super) fn register_routes() {
    let _ = show as fn(_, _, _, _, _) -> _;
    let _ = confirm as fn(_, _, _, _) -> _;
}
