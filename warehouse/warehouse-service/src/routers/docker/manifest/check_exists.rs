use super::get_image::resolve_manifest_response;
use actix_web::{HttpRequest, HttpResponse, Responder, head, web};

#[utoipa::path(
    head,
    operation_id = "check_exists",
    tags = ["docker - manifest"],
    path = "/{name}/manifests/{reference}",
    params(
        ("Accept" = Option<String>, Header, description = "Manifest media types supported by client"),
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("reference" = String, Path, description = "Tag or digest of the target manifest"),
    ),
    responses(
        (
            status = 200,
            description = "Manifest exists",
            headers(
                ("Docker-Content-Digest" = String),
                ("Content-Length" = u64),
                ("Content-Type" = String),
            )
        ),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Manifest not found"),
        (status = 429, description = "Too many requests"),
    )
)]
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
        .append_header(("Content-Length", resolved.data.len()))
        .finish()
}
