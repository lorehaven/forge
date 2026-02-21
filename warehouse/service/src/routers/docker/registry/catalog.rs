use crate::routers::DOCKER_STORAGE_ROOT;
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    tags = ["docker"],
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

    let mut repos = Vec::new();
    let root = PathBuf::from(DOCKER_STORAGE_ROOT.as_str());

    collect_repositories(&root, "", &mut repos);

    repos.sort();

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

fn collect_repositories(dir: &Path, prefix: &str, repos: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if prefix.is_empty()
                    && matches!(
                        entry.file_name().to_str(),
                        Some("blobs" | "manifests" | "_uploads")
                    )
                {
                    continue;
                }

                let name = entry.file_name();
                let name = name.to_string_lossy();
                let repo_name = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", prefix, name)
                };

                // Repositories are identified by tag namespace in this storage layout.
                if path.join("tags").exists() {
                    repos.push(repo_name.clone());
                }

                collect_repositories(&path, &repo_name, repos);
            }
        }
    }
}
