use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use utoipa::OpenApi;

pub mod docker;

#[derive(OpenApi)]
#[openapi(
    paths(
        docker::gc::handle,
    ),
    tags((name = "admin", description = "Admin endpoints"))
)]
pub struct AdminApiDoc;

pub fn scope() -> impl HttpServiceFactory {
    // Admin endpoints
    web::scope("/admin").service(docker::gc::handle)
}
