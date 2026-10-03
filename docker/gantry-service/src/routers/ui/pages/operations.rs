//! The history of changes, and one change as it runs.

use crate::domain::operation::Operation;
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, action, badge, notice_banner, redirect, render_page, row, table, tabs,
    ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;

fn when(time: Option<chrono::DateTime<chrono::Utc>>) -> String {
    time.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

#[get("/ui/operations")]
pub(super) async fn list(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Query(notice): Query<Notice>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can(Some(&claims), &config, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }

    let operations = gantry.store.list(100).await.unwrap_or_default();
    let mut history = table(&[
        "ui_col_title",
        "ui_col_state",
        "ui_col_requested_by",
        "ui_col_started",
    ]);
    for operation in &operations {
        history = history.child(row(vec![
            a().attr("href", ui_path(&format!("/operations/{}", operation.id)))
                .text(operation.title.clone()),
            badge("state", operation.state.as_str()),
            span()
                .class("gt-muted")
                .text(operation.requested_by.clone()),
            span()
                .class("gt-muted")
                .text(when(Some(operation.created_at))),
        ]));
    }

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(tabs("operations"))
                .child_opt(notice_banner(&notice))
                .child(if operations.is_empty() {
                    empty_state("ui_ops_none")
                } else {
                    history
                }),
        ),
    )
}

#[get("/ui/operations/{id}")]
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
    if !authz::can(Some(&claims), &config, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    let Ok(Some(operation)) = gantry.store.get(&id).await else {
        return redirect("/operations", Some(("error", "no such operation")));
    };

    // While it runs the log is the runner's own output now; afterwards it is the copy kept at the end.
    let log = if operation.state.is_terminal() {
        operation.log.clone().unwrap_or_default()
    } else {
        match &operation.runner_job {
            Some(job) => gantry.executor.log(job).await.unwrap_or_default(),
            None => String::new(),
        }
    };

    let may_cancel = !operation.state.is_terminal() && authz::can(Some(&claims), &config, "deploy");
    render_page(
        StatusCode::OK,
        operation_page(&operation, &log, may_cancel, &notice),
    )
}

fn operation_page(operation: &Operation, log: &str, may_cancel: bool, notice: &Notice) -> Element {
    let mut steps = element("ol").class("gt-list");
    for step in &operation.plan.steps {
        steps = steps.child(li().text(step.describe()));
    }

    let mut page = div()
        .class("home-container")
        .child(tabs("operations"))
        .child_opt(notice_banner(notice))
        .child(
            div()
                .class("home-header")
                .child(h3().text(operation.title.clone()))
                .child(badge("state", operation.state.as_str())),
        )
        .child(p().class("gt-meta").text(format!(
            "{} · {}",
            operation.requested_by,
            when(Some(operation.created_at))
        )))
        .child_opt(
            operation
                .error
                .as_ref()
                .map(|error| p().class("gt-notice gt-notice-error").text(error.clone())),
        )
        .child(
            element("details")
                .class("gt-details")
                .child(
                    element("summary")
                        .child(span().attr("data-i18n", "ui_plan_steps"))
                        .child(span().text(format!(" ({})", operation.plan.steps.len()))),
                )
                .child(steps),
        )
        .child(div().class("gt-section").attr("data-i18n", "ui_op_log"))
        .child(pre().class("gt-log").text(if log.is_empty() {
            "—".to_string()
        } else {
            log.to_string()
        }));

    if may_cancel {
        page = page.child(div().class("gt-bar").child(action(
            &format!("/operations/{}/cancel", operation.id),
            "ui_op_cancel",
            "danger",
        )));
    }
    // Still going: look again shortly. A reload is the whole mechanism; the page is server-rendered.
    if !operation.state.is_terminal() {
        page = page.child(script(
            "setTimeout(function () { window.location.reload(); }, 3000);".to_string(),
        ));
    }
    content().class("home-content").child(page)
}

#[post("/ui/operations/{id}/cancel")]
pub(super) async fn cancel(
    Path(id): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can(Some(&claims), &config, "deploy") {
        return redirect(&format!("/operations/{id}"), Some(("error", "not allowed")));
    }
    match gantry.cancel(&id).await {
        Ok(Some(_)) => redirect(&format!("/operations/{id}"), Some(("ok", "cancelling"))),
        Ok(None) => redirect("/operations", Some(("error", "no such operation"))),
        Err(error) => redirect(
            &format!("/operations/{id}"),
            Some(("error", &error.to_string())),
        ),
    }
}

pub(super) fn register_routes() {
    let _ = list as fn(_, _, _, _) -> _;
    let _ = show as fn(_, _, _, _, _) -> _;
    let _ = cancel as fn(_, _, _, _) -> _;
}
