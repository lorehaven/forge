use crate::domain::jwt::JwtConfig;
use crate::routers::ui::common::{is_ui_authenticated, ui_login_redirect};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};

#[get("/docker/tags/{repository:.+}")]
pub(super) async fn docker_tags(
    req: HttpRequest,
    path: web::Path<String>,
    config: web::Data<JwtConfig>,
) -> impl Responder {
    if !is_ui_authenticated(&req, &config) {
        return ui_login_redirect();
    }
    let repository = path.into_inner();
    HttpResponse::PermanentRedirect()
        .append_header(("Location", format!("/ui/docker/catalog?repo={repository}")))
        .finish()
}
