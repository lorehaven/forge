use actix_web::dev::HttpServiceFactory;
use actix_web::web;

pub mod crates;
pub mod docker;

pub fn scope() -> impl HttpServiceFactory {
    // Admin endpoints
    web::scope("/admin")
        .service(crates::gc::handle)
        .service(docker::gc::handle)
}
