use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use utoipa::OpenApi;

pub mod crates;
pub mod docker;

#[derive(OpenApi)]
#[openapi(
    paths(
        crates::gc::handle,
        docker::gc::handle,
    ),
    tags((name = "admin", description = "Admin endpoints"))
)]
pub struct AdminApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    // Admin endpoints
    web::scope("/admin")
        .service(crates::gc::handle)
        .service(docker::gc::handle)
}
