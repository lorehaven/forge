use actix_web::{HttpResponse, Responder, get, post, web};
use chrono::Utc;
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Crud, Db};
use quench_starter::prelude::with_base_path;
use quench_web::prelude::*;
use uuid::Uuid;

#[get("/new-modal")]
pub async fn new_modal() -> impl Responder {
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

    HttpResponse::Ok()
        .content_type("text/html")
        .body(modal.render())
}

#[derive(serde::Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
}

#[post("/create")]
pub async fn create_project(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    form: web::Form<CreateProjectRequest>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let project = crate::models::Project {
        id: Uuid::new_v4().to_string(),
        name: form.name.clone(),
        owner: username,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };

    let repo = db.repository::<crate::models::Project>();
    if let Err(e) = repo.create(&project).await {
        tracing::error!("Failed to create project: {}", e);
        return HttpResponse::InternalServerError().body("api_error_internal");
    }

    HttpResponse::Ok()
        .append_header((
            "HX-Redirect",
            with_base_path(&format!("/ui/home?project_id={}", project.id)),
        ))
        .finish()
}

pub fn scope() -> actix_web::Scope {
    web::scope("/projects")
        .service(new_modal)
        .service(create_project)
}
