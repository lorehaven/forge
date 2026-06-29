use crate::prelude::with_base_path;
use actix_web::{HttpResponse, get};

#[get("/swagger-ui")]
async fn swagger_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/swagger-ui/")))
        .finish()
}

#[get("/swagger-ui/")]
async fn swagger_index_redirect() -> HttpResponse {
    HttpResponse::PermanentRedirect()
        .append_header(("Location", with_base_path("/swagger-ui/index.html")))
        .finish()
}
