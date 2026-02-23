use actix_web::{HttpResponse, Responder, get, head};

#[utoipa::path(
    get,
    operation_id = "check",
    tags = ["docker"],
    path = "/",
    responses((status = 200, description = "Registry is available"))
)]
#[get("/")]
pub async fn handle_get() -> impl Responder {
    respond().await
}

#[utoipa::path(
    head,
    operation_id = "check",
    tags = ["docker"],
    path = "/",
    responses((status = 200, description = "Registry is available"))
)]
#[head("/")]
pub async fn handle_head() -> impl Responder {
    respond().await
}

async fn respond() -> HttpResponse {
    HttpResponse::Ok()
        .append_header(("Docker-Distribution-API-Version", "registry/2.0"))
        .finish()
}
