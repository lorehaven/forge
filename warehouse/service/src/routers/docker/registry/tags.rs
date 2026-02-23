use crate::domain::docker_error;
use crate::routers::docker::registry::storage::{TagListError, list_tags_for_repository};
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
    path = "/{name}/tags/list",
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

    let tags = match list_tags_for_repository(&name) {
        Ok(tags) => tags,
        Err(TagListError::InvalidName) => {
            return docker_error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                docker_error::NAME_UNKNOWN,
                "invalid repository name",
            );
        }
        Err(TagListError::NotFound) => {
            return docker_error::response(
                actix_web::http::StatusCode::NOT_FOUND,
                docker_error::NAME_UNKNOWN,
                "repository name not known to registry",
            );
        }
    };

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
