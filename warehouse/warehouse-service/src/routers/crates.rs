use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, get, web};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(placeholder),
    tags((name = "crates", description = "Crates endpoints"))
)]
pub struct CratesApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/v1/api/crates")
        .wrap(NormalizePath::trim())
        .service(placeholder)
}

#[utoipa::path(
    get,
    tags = ["crates"],
    path = "",
    responses((status = 200, description = "placeholder"))
)]
#[get("")]
async fn placeholder() -> impl Responder {
    HttpResponse::Ok().body("OK")
}
