use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, get, web};

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/health")
        .wrap(NormalizePath::trim())
        .service(health)
}

#[get("")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}
