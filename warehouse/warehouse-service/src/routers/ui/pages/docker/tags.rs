use crate::domain::jwt::JwtConfig;
use crate::routers::ui::common::{is_ui_authenticated, ui_login_redirect};
use crate::routers::with_base_path;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};

#[get("/docker/tags/{repository:.+}")]
pub(in crate::routers::ui::pages) async fn docker_tags(
    req: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    let repository = path.into_inner();
    HttpResponse::PermanentRedirect()
        .append_header((
            "Location",
            with_base_path(&format!("/ui/docker/catalog?repo={repository}")),
        ))
        .finish()
}
