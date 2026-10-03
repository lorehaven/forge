//! Doing things to one resource: edit its live YAML, delete it, apply it again from the package.
//! Each acts at once and lands on the operation, whose log is the record.

use crate::domain::actions::{self, ActionError, Target};
use crate::domain::service::Gantry;
use crate::routers::api::authz;
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, notice_banner, redirect, render_page, tabs, ui_path,
};
use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Form, Inject, Path, Query, Response, get, http::StatusCode, post};
use quench_web::prelude::*;
use serde::Deserialize;

/// What a button posts to name a resource.
#[derive(Deserialize)]
pub(super) struct ResourceForm {
    pub package: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub api_version: String,
    #[serde(default)]
    pub yaml: String,
}

impl ResourceForm {
    fn target(&self) -> Target {
        Target {
            api_version: Some(self.api_version.clone()).filter(|v| !v.is_empty()),
            kind: self.kind.clone(),
            name: self.name.clone(),
            namespace: Some(self.namespace.clone()).filter(|v| !v.is_empty()),
        }
    }
}

/// The same fields, as a query string (the edit page is a plain link).
#[derive(Deserialize)]
pub(super) struct ResourceQuery {
    pub package: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub api_version: String,
    #[serde(default)]
    pub ok: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

fn failed(error: &ActionError) -> Response {
    redirect("/home", Some(("error", &error.to_string())))
}

fn started(operation: &crate::domain::operation::Operation) -> Response {
    redirect(&format!("/operations/{}", operation.id), None)
}

#[get("/ui/resource")]
pub(super) async fn edit_page(
    Query(query): Query<ResourceQuery>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &query.package, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    let current = ResourceForm {
        package: query.package.clone(),
        kind: query.kind.clone(),
        name: query.name.clone(),
        namespace: query.namespace.clone(),
        api_version: query.api_version.clone(),
        yaml: String::new(),
    };
    let text = match actions::yaml(&gantry, &current.package, &current.target()).await {
        Ok(text) => text,
        Err(error) => return failed(&error),
    };
    let may_edit = authz::can_on_target(Some(&claims), &config, &query.package, "deploy");
    let notice = Notice {
        ok: query.ok.clone(),
        error: query.error.clone(),
    };

    let hidden = |name: &str, value: &str| {
        input()
            .attr("type", "hidden")
            .attr("name", name)
            .attr("value", value)
    };
    let mut area = textarea()
        .attr("name", "yaml")
        .attr("spellcheck", "false")
        .attr("rows", "28")
        .class("gt-yaml")
        .text(text);
    if !may_edit {
        area = area.attr("readonly", "true");
    }

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child(tabs("targets"))
                .child_opt(notice_banner(&notice))
                .child(
                    p().class("gt-crumb").child(
                        a().attr("href", ui_path("/home"))
                            .text(query.package.clone()),
                    ),
                )
                .child(
                    div()
                        .class("home-header")
                        .child(h3().text(format!("{}/{}", query.kind, query.name)))
                        .child(span().class("gt-muted").text(query.namespace.clone())),
                )
                .child(
                    form()
                        .attr("method", "post")
                        .attr("action", ui_path("/resource/edit"))
                        .class("gt-form")
                        .child(hidden("package", &query.package))
                        .child(hidden("kind", &query.kind))
                        .child(hidden("name", &query.name))
                        .child(hidden("namespace", &query.namespace))
                        .child(hidden("api_version", &query.api_version))
                        .child(area)
                        .child(p().class("gt-desc").attr("data-i18n", "ui_edit_note"))
                        .child(
                            div()
                                .class("gt-bar gt-bar-end")
                                .child(
                                    a().attr("href", ui_path("/home"))
                                        .class("gt-btn")
                                        .attr("data-i18n", "ui_back"),
                                )
                                .child_opt(may_edit.then(|| {
                                    button()
                                        .attr("type", "submit")
                                        .class("gt-btn gt-btn-primary")
                                        .attr("data-i18n", "ui_edit_save")
                                })),
                        ),
                ),
        ),
    )
}

#[post("/ui/resource/edit")]
pub(super) async fn edit(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<ResourceForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &form.package, "deploy") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match actions::edit(
        &gantry,
        &form.package,
        &form.target(),
        &form.yaml,
        &claims.sub,
    )
    .await
    {
        Ok(operation) => started(&operation),
        // Back to the editor with the reason, the text typed so far is not lost by the browser's own
        // form restore; the message says what to fix.
        Err(error) => redirect(
            &format!(
                "/resource?package={}&kind={}&name={}&namespace={}&api_version={}",
                crate::routers::ui::common::percent_encode(&form.package),
                crate::routers::ui::common::percent_encode(&form.kind),
                crate::routers::ui::common::percent_encode(&form.name),
                crate::routers::ui::common::percent_encode(&form.namespace),
                crate::routers::ui::common::percent_encode(&form.api_version),
            ),
            Some(("error", &error.to_string())),
        ),
    }
}

#[post("/ui/resources/delete")]
pub(super) async fn delete(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<ResourceForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &form.package, "scale") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match actions::delete(&gantry, &form.package, &form.target(), &claims.sub).await {
        Ok(operation) => started(&operation),
        Err(error) => failed(&error),
    }
}

#[post("/ui/resources/apply")]
pub(super) async fn apply(
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
    Form(form): Form<ResourceForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &form.package, "scale") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match actions::apply(&gantry, &form.package, Some(&form.target()), &claims.sub).await {
        Ok(operation) => started(&operation),
        Err(error) => failed(&error),
    }
}

#[post("/ui/packages/{name}/apply")]
pub(super) async fn apply_missing(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &name, "scale") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match actions::apply(&gantry, &name, None, &claims.sub).await {
        Ok(operation) => started(&operation),
        Err(error) => failed(&error),
    }
}

#[post("/ui/packages/{name}/refresh")]
pub(super) async fn refresh(
    Path(name): Path<String>,
    actor: ActorOrRedirect,
    Inject(gantry): Inject<Gantry>,
    Inject(config): Inject<JwtConfig>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };
    if !authz::can_on_target(Some(&claims), &config, &name, "read") {
        return redirect("/home", Some(("error", "not allowed")));
    }
    match actions::refresh(&gantry, &name, &claims.sub).await {
        Ok(operation) => started(&operation),
        Err(error) => failed(&error),
    }
}

pub(super) fn register_routes() {
    let _ = edit_page as fn(_, _, _, _) -> _;
    let _ = edit as fn(_, _, _, _) -> _;
    let _ = delete as fn(_, _, _, _) -> _;
    let _ = apply as fn(_, _, _, _) -> _;
    let _ = apply_missing as fn(_, _, _, _) -> _;
    let _ = refresh as fn(_, _, _, _) -> _;
}
