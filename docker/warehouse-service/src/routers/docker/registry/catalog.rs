use crate::routers::docker::registry::storage::list_repositories;
use quench_http::prelude::{Query, Response, get, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
pub struct CatalogQuery {
    n: Option<usize>,
    last: Option<String>,
}

#[derive(Serialize)]
struct CatalogResponse {
    repositories: Vec<String>,
}

#[get("/v2/_catalog")]
pub async fn handle(query: Query<CatalogQuery>) -> Response {
    let query = query.0;

    let n = query.n.unwrap_or(100);
    let last = query.last;

    let repos = list_repositories();

    let start = last
        .as_ref()
        .and_then(|l| repos.iter().position(|r| r == l))
        .map(|i| i + 1)
        .unwrap_or(0);

    let page: Vec<String> = repos.into_iter().skip(start).take(n).collect();

    let mut response = Response::json(
        StatusCode::OK,
        &CatalogResponse {
            repositories: page.clone(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR));

    if page.len() == n
        && let Some(last_item) = page.last()
    {
        let link = format!("</v2/_catalog?n={}&last={}>; rel=\"next\"", n, last_item);
        response = response.header("link", link);
    }

    response
}

pub fn register_routes() {
    let _ = handle as fn(_) -> _;
}
