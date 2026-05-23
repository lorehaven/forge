use crate::routers::docker::registry::storage::list_repositories;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
struct CatalogQuery {
    n: Option<usize>,
    last: Option<String>,
}

#[derive(Serialize, ToSchema)]
struct CatalogResponse {
    repositories: Vec<String>,
}

#[utoipa::path(
    get,
    operation_id = "catalog",
    tags = ["docker - registry"],
    path = "/_catalog",
    params(
        ("n" = Option<usize>, Query, description = "Maximum number of repositories to return"),
        ("last" = Option<String>, Query, description = "Last repository from previous page"),
    ),
    responses(
        (
            status = 200,
            description = "List of repositories",
            body = CatalogResponse
        )
    )
)]
#[get("/_catalog")]
pub async fn handle(req: HttpRequest) -> impl Responder {
    let query = web::Query::<CatalogQuery>::from_query(req.query_string()).ok();

    let n = query.as_ref().and_then(|q| q.n).unwrap_or(100);
    let last = query.as_ref().and_then(|q| q.last.clone());

    let repos = list_repositories();

    let start = last
        .as_ref()
        .and_then(|l| repos.iter().position(|r| r == l))
        .map(|i| i + 1)
        .unwrap_or(0);

    let page: Vec<String> = repos.into_iter().skip(start).take(n).collect();

    let mut response = HttpResponse::Ok();

    if page.len() == n
        && let Some(last_item) = page.last()
    {
        let link = format!("</v2/_catalog?n={}&last={}>; rel=\"next\"", n, last_item);
        response.append_header(("Link", link));
    }

    response.json(CatalogResponse { repositories: page })
}
