//! The project list - workbench's entry point.

use crate::domain::project::{self, NewProject};
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, PageAuth, notice_banner, render_page, ui_login_redirect, ui_path,
};
use forge_ui::{
    PageHeader, Variant, button_el, close_modal, empty_state, field_for, form_el, icon, link_card,
    modal, open_modal, submit_button, text_input, textarea_el, txt,
};
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use quench_web::prelude::*;

#[get("/ui/home")]
pub(super) async fn home(
    PageAuth(authenticated): PageAuth,
    Inject(db): Inject<Db>,
    Query(notice): Query<Notice>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    render_home(&db, &notice).await
}

#[get("/ui/home/")]
pub(super) async fn home_slash(
    PageAuth(authenticated): PageAuth,
    Inject(db): Inject<Db>,
    Query(notice): Query<Notice>,
) -> Response {
    if !authenticated {
        return ui_login_redirect();
    }
    render_home(&db, &notice).await
}

async fn render_home(db: &Db, notice: &Notice) -> Response {
    let projects = project::list(db).await.unwrap_or_default();

    let mut grid = div().class("fui-grid");
    for p in &projects {
        grid = grid.child(link_card(
            "table-columns",
            &ui_path(&format!("/projects/{}/board", p.id)),
            txt(format!("{} — {}", p.key, p.name)),
            p.description.as_ref().map(|d| txt(d.clone())),
        ));
    }

    render_page(
        StatusCode::OK,
        div()
            .class("fui-stack fui-stack-lg")
            .child_opt(notice_banner(notice))
            .child(
                PageHeader::new("ui_home_title")
                    .action(
                        button_el(Variant::Primary, "ui_home_new_project")
                            .child(icon("plus"))
                            .on_click(open_modal("new-project")),
                    )
                    .build(),
            )
            .child(if projects.is_empty() {
                empty_state("table-columns", "ui_home_no_projects", None)
            } else {
                grid
            })
            .child(new_project_modal()),
    )
}

fn new_project_modal() -> Element {
    let body = form_el("post", &ui_path("/projects"))
        .attr("id", "new-project-form")
        .child(field_for(
            "wb-key",
            "ui_field_key",
            text_input("key", "text")
                .attr("id", "wb-key")
                .attr("required", "true")
                .attr("maxlength", "10"),
            None,
        ))
        .child(field_for(
            "wb-name",
            "ui_field_name",
            text_input("name", "text")
                .attr("id", "wb-name")
                .attr("required", "true"),
            None,
        ))
        .child(field_for(
            "wb-description",
            "ui_field_description",
            textarea_el("description").attr("id", "wb-description"),
            None,
        ));

    modal(
        "new-project",
        "ui_home_new_project",
        body,
        Some(
            div()
                .class("fui-row")
                .child(
                    button_el(Variant::Secondary, "ui_cancel").on_click(close_modal("new-project")),
                )
                .child(
                    submit_button(Variant::Primary, "ui_home_create_button")
                        .attr("form", "new-project-form"),
                ),
        ),
    )
}

#[derive(Deserialize)]
pub(super) struct CreateProjectForm {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[post("/ui/projects")]
pub(super) async fn create_project(
    actor: ActorOrRedirect,
    Inject(db): Inject<Db>,
    Form(form): Form<CreateProjectForm>,
) -> Response {
    let claims = match actor.or_redirect() {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    if form.key.trim().is_empty() || form.name.trim().is_empty() {
        return redirect_home(Some("empty_fields"));
    }

    if !claims.can("workbench", "write") {
        return redirect_home(Some("forbidden"));
    }

    let new = NewProject {
        key: form.key.trim().to_string(),
        name: form.name.trim().to_string(),
        description: (!form.description.trim().is_empty())
            .then(|| form.description.trim().to_string()),
    };

    match project::create(&db, &new).await {
        Ok(created) => Response::new(StatusCode::FOUND).header(
            "Location",
            ui_path(&format!("/projects/{}/board", created.id)),
        ),
        // The only uniqueness constraint a project can hit is its own `key`.
        Err(error) if error.is_unique_violation() => redirect_home(Some("key_taken")),
        Err(_) => redirect_home(Some("create_failed")),
    }
}

fn redirect_home(error: Option<&str>) -> Response {
    let location = error.map_or_else(
        || ui_path("/home"),
        |code| format!("{}?error={code}", ui_path("/home")),
    );
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _, _) -> _;
    let _ = home_slash as fn(_, _, _) -> _;
    let _ = create_project as fn(_, _, _) -> _;
}
