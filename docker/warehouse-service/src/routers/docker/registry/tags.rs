use crate::domain::docker_error;
use crate::routers::docker::registry::storage::{TagListError, list_tags_for_repository};
use quench_http::prelude::{Path, Query, Response, get, http::StatusCode};
use quench_starter::http::domain::error;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct TagsResponse {
    name: String,
    tags: Vec<String>,
}

#[derive(Deserialize, Default)]
pub struct TagsQuery {
    n: Option<usize>,
    last: Option<String>,
}

#[get("/v2/{name:.+}/tags/list")]
pub async fn handle(Path(name): Path<String>, query: Query<TagsQuery>) -> Response {
    let query = query.0;
    let n = query.n.unwrap_or(100);
    let last = query.last;

    let tags = match list_tags_for_repository(&name) {
        Ok(tags) => tags,
        Err(TagListError::InvalidName) => {
            return error::response(
                StatusCode::BAD_REQUEST,
                docker_error::NAME_UNKNOWN,
                "invalid repository name",
            );
        }
        Err(TagListError::NotFound) => {
            return error::response(
                StatusCode::NOT_FOUND,
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

    let mut response = Response::json(
        StatusCode::OK,
        &TagsResponse {
            name: name.clone(),
            tags: page.clone(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));

    if page.len() == n
        && let Some(last_item) = page.last()
    {
        let link = format!(
            "</v2/{}/tags/list?n={}&last={}>; rel=\"next\"",
            name, n, last_item
        );
        response = response.header("link", link);
    }

    response
}

pub fn register_routes() {
    let _ = handle as fn(_, _) -> _;
}
