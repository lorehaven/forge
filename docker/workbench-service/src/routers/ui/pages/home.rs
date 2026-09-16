//! The project list - workbench's entry point.

use crate::domain::project::{self, NewProject};
use crate::routers::ui::common::{
    ActorOrRedirect, Notice, PageAuth, notice_banner, render_page, ui_login_redirect, ui_path,
};
use quench_db::prelude::Db;
use quench_http::prelude::{Form, Inject, Query, Response, get, http::StatusCode, post};
use quench_web::framework::dom::toggle_modal;
use quench_web::prelude::*;
use quench_web_components::containers::empty_state;
use serde::Deserialize;

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

    let mut grid = div().class("home-grid");
    if projects.is_empty() {
        grid = grid.child(empty_state("ui_home_no_projects"));
    }
    for p in &projects {
        grid = grid.child(
            a().attr("href", ui_path(&format!("/projects/{}/board", p.id)))
                .class("home-card")
                .child(
                    div()
                        .class("home-card-body")
                        .child(
                            div()
                                .class("home-card-title")
                                .text(format!("{} — {}", p.key, p.name)),
                        )
                        .child_opt(
                            p.description
                                .as_ref()
                                .map(|d| div().class("home-card-desc").text(d.clone())),
                        ),
                )
                .child(div().class("home-card-arrow").text("→")),
        );
    }

    let toggle = toggle_modal("modal-overlay", "modal-center", "show");

    render_page(
        StatusCode::OK,
        content().class("home-content").child(
            div()
                .class("home-container")
                .child_opt(notice_banner(notice))
                .child(
                    div()
                        .class("home-header")
                        .child(h3().attr("data-i18n", "ui_home_title"))
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("onclick", &toggle)
                                .attr("title", "New project")
                                .class("wb-add-button")
                                .text("+"),
                        ),
                )
                .child(grid)
                .child(div().class("modal-overlay").on_click(&toggle))
                .child(
                    div()
                        .class("modal-center")
                        .child(new_project_modal_content(&toggle)),
                ),
        ),
    )
}

fn new_project_modal_content(toggle: &str) -> Element {
    div()
        .class("modal-content")
        .child(
            div()
                .class("wb-modal-header")
                .child(
                    div()
                        .class("panel-title")
                        .attr("data-i18n", "ui_home_new_project"),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("onclick", toggle)
                        .class("wb-modal-close")
                        .text("×"),
                ),
        )
        .child(
            form()
                .attr("method", "post")
                .attr("action", ui_path("/projects"))
                .class("wb-form")
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-key")
                                .attr("data-i18n", "ui_field_key"),
                        )
                        .child(
                            input()
                                .attr("type", "text")
                                .attr("id", "wb-key")
                                .attr("name", "key")
                                .attr("required", "true")
                                .attr("maxlength", "10"),
                        ),
                )
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-name")
                                .attr("data-i18n", "ui_field_name"),
                        )
                        .child(
                            input()
                                .attr("type", "text")
                                .attr("id", "wb-name")
                                .attr("name", "name")
                                .attr("required", "true"),
                        ),
                )
                .child(
                    div()
                        .class("wb-field-row")
                        .child(
                            label()
                                .attr("for", "wb-description")
                                .attr("data-i18n", "ui_field_description"),
                        )
                        .child(
                            textarea()
                                .attr("id", "wb-description")
                                .attr("name", "description"),
                        ),
                )
                .child(
                    button()
                        .attr("type", "submit")
                        .class("wb-submit")
                        .attr("data-i18n", "ui_home_create_button"),
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
    let location = match error {
        Some(code) => format!("{}?error={code}", ui_path("/home")),
        None => ui_path("/home"),
    };
    Response::new(StatusCode::FOUND).header("Location", location)
}

pub(super) fn register_routes() {
    let _ = home as fn(_, _, _) -> _;
    let _ = home_slash as fn(_, _, _) -> _;
    let _ = create_project as fn(_, _, _) -> _;
}
