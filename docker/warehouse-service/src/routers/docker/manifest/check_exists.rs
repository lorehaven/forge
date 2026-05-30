use super::get_image::resolve_manifest_response;
use actix_web::{HttpRequest, HttpResponse, Responder, head, web};

#[head("/{name:.+}/manifests/{reference}")]
pub async fn handle(req: HttpRequest, path: web::Path<(String, String)>) -> impl Responder {
    let (name, reference) = path.into_inner();

    let resolved = match resolve_manifest_response(&req, &name, &reference).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    HttpResponse::Ok()
        .append_header(("Content-Type", resolved.media_type))
        .append_header(("Docker-Content-Digest", resolved.digest))
        .body(resolved.data)
}
