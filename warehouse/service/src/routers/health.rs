use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, get, web};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(health),
    tags((name = "health", description = "Health endpoints"))
)]
pub struct HealthApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/health")
        .wrap(NormalizePath::trim())
        .service(health)
}

#[utoipa::path(
    get,
    tags = ["health"],
    path = "",
    responses((status = 200, description = "Healthy"))
)]
#[get("")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}
