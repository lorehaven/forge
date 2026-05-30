use actix_web::{HttpResponse, Responder, get, head};

#[get("/")]
pub async fn handle_get() -> impl Responder {
    respond().await
}

#[head("/")]
pub async fn handle_head() -> impl Responder {
    respond().await
}

async fn respond() -> HttpResponse {
    HttpResponse::Ok()
        .append_header(("Docker-Distribution-API-Version", "registry/2.0"))
        .finish()
}
