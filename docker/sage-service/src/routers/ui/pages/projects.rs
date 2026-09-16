use crate::routers::ui::common::RequiredClaims;
use chrono::Utc;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Form, HttpError, Inject, Response, get, http::StatusCode, post};
use quench_starter::common::routes::with_base_path;
use quench_web::prelude::*;
use uuid::Uuid;

#[get("/ui/projects/new-modal")]
pub async fn new_modal() -> Response {
    let modal = div()
        .class("modal-backdrop")
        .attr("id", "new-project-modal")
        .child(
            div()
                .class("modal-content")
                .child(
                    h2().attr("data-i18n", "ui_projects_new_title")
                        .text("Create New Project"),
                )
                .child(
                    form()
                        .attr("hx-post", with_base_path("/ui/projects/create"))
                        .attr("hx-target", "#new-project-modal")
                        .attr("hx-swap", "outerHTML")
                        .child(
                            div()
                                .class("form-group")
                                .child(
                                    label()
                                        .attr("for", "project-name")
                                        .attr("data-i18n", "ui_projects_name_label")
                                        .text("Project Name"),
                                )
                                .child(
                                    input()
                                        .attr("type", "text")
                                        .attr("id", "project-name")
                                        .attr("name", "name")
                                        .attr("required", "required")
                                        .attr("autofocus", "autofocus"),
                                ),
                        )
                        .child(
                            div()
                                .class("modal-actions")
                                .child(
                                    button()
                                        .attr("type", "button")
                                        .class("btn-secondary")
                                        .attr(
                                            "onclick",
                                            "document.getElementById('new-project-modal').remove()",
                                        )
                                        .attr("data-i18n", "ui_common_cancel")
                                        .text("Cancel"),
                                )
                                .child(
                                    button()
                                        .attr("type", "submit")
                                        .class("btn-primary")
                                        .attr("data-i18n", "ui_projects_create")
                                        .text("Create"),
                                ),
                        ),
                ),
        );

    Response::html(StatusCode::OK, modal.render())
}

#[derive(serde::Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
}

#[post("/ui/projects/create")]
pub async fn create_project(
    claims: RequiredClaims,
    Inject(db): Inject<Db>,
    Form(form): Form<CreateProjectRequest>,
) -> Result<Response, HttpError> {
    let username = match claims.or_401() {
        Ok(claims) => claims.sub,
        Err(response) => return Ok(response),
    };

    let project = crate::domain::models::Project {
        id: Uuid::new_v4().to_string(),
        name: form.name.clone(),
        owner: username,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let repo = db.repository::<crate::domain::models::Project>();
    if let Err(e) = repo.create(&project).await {
        tracing::error!("Failed to create project: {}", e);
        return Ok(Response::text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "api_error_internal",
        ));
    }

    Ok(Response::new(StatusCode::OK).header(
        "HX-Redirect",
        with_base_path(&format!("/ui/home?project_id={}", project.id)),
    ))
}

pub fn register_routes() {
    let _ = new_modal as fn() -> _;
    let _ = create_project as fn(_, _, _) -> _;
}
