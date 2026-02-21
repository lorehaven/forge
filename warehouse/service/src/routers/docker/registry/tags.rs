use crate::routers::docker::repository_path;
use crate::shared::docker_error;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
struct TagsResponse {
    name: String,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct TagsQuery {
    n: Option<usize>,
    last: Option<String>,
}

#[utoipa::path(
    get,
    operation_id = "get_tags",
    path = "/v2/{name}/tags/list",
    tag = "docker",
    params(
        ("name" = String, Path, description = "Repository name"),
        ("n" = Option<usize>, Query, description = "Maximum number of repositories to return"),
        ("last" = Option<String>, Query, description = "Last repository from previous page"),
    ),
    responses(
        (
            status = 200,
            description = "List of tags",
            body = TagsResponse
        ),
        (status = 404, description = "Repository not found")
    )
)]
#[get("/{name:.+}/tags/list")]
pub async fn handle(req: HttpRequest, path: web::Path<String>) -> impl Responder {
    let name = path.into_inner();

    let query = web::Query::<TagsQuery>::from_query(req.query_string()).ok();
    let n = query.as_ref().and_then(|q| q.n).unwrap_or(100);
    let last = query.as_ref().and_then(|q| q.last.clone());

    let Some(repo_root) = repository_path(&name) else {
        return docker_error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        );
    };
    let repo_path = repo_root.join("tags");

    if !repo_path.exists() {
        return docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::NAME_UNKNOWN,
            "repository name not known to registry",
        );
    }

    let mut tags = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&repo_path) {
        for entry in entries.flatten() {
            if let Some(tag) = entry.file_name().to_str() {
                tags.push(tag.to_string());
            }
        }
    }

    tags.sort();

    let start = last
        .as_ref()
        .and_then(|l| tags.iter().position(|r| r == l))
        .map(|i| i + 1)
        .unwrap_or(0);

    let page: Vec<String> = tags.into_iter().skip(start).take(n).collect();

    let mut response = HttpResponse::Ok();

    if page.len() == n
        && let Some(last_item) = page.last()
    {
        let link = format!(
            "</v2/{}/tags/list?n={}&last={}>; rel=\"next\"",
            name, n, last_item
        );
        response.append_header(("Link", link));
    }

    response.json(TagsResponse { name, tags: page })
}
