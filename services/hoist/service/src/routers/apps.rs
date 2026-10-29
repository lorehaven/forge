use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::{Deserialize, Serialize};
use quench_starter::http::domain::api_error::ApiError;

use crate::models::MergedPackage;
use crate::store::{self, Filter, Sort, StoreError};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    /// Only apps with at least this many ratings (an average of two votes means little).
    pub min_votes: Option<i64>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_sort() -> String {
    "downloads".to_string()
}

const fn default_limit() -> i64 {
    30
}

#[derive(Debug, Serialize)]
pub struct AppOut {
    pub package_name: String,
    pub label: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub category: Option<String>,
    pub version_code: i64,
    pub version_name: String,
    pub size_bytes: i64,
    pub download_url: String,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub install_source: String,
    pub rating_avg: Option<f64>,
    pub rating_count: Option<i64>,
    pub downloads: Option<i64>,
    pub malware_rank: Option<String>,
}

impl From<MergedPackage> for AppOut {
    fn from(p: MergedPackage) -> Self {
        Self {
            package_name: p.package_name,
            label: p.label,
            summary: p.summary,
            description: p.description,
            icon_url: p.icon_url,
            category: p.category,
            version_code: p.version_code,
            version_name: p.version_name,
            size_bytes: p.size_bytes,
            download_url: p.download_url,
            sha256: p.sha256,
            md5: p.md5,
            install_source: p.install_source,
            rating_avg: p.rating_avg,
            rating_count: p.rating_count,
            downloads: p.downloads,
            malware_rank: p.malware_rank,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AppListOut {
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub items: Vec<AppOut>,
}

#[derive(Debug, Serialize)]
pub struct CategoryOut {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryListOut {
    pub items: Vec<CategoryOut>,
}

fn failed(error: StoreError) -> ApiError {
    match error {
        StoreError::NotPostgres => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "the catalogue needs Postgres",
        ),
        StoreError::Sql(e) => ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

fn json<T: Serialize>(status: StatusCode, value: &T) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Browse or search the catalogue, a page at a time.
#[get("/v1/apps")]
pub async fn list_apps(
    Inject(db): Inject<Db>,
    Query(params): Query<ListQuery>,
) -> Result<Response, ApiError> {
    let sort = Sort::parse(&params.sort).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("invalid sort: {}", params.sort),
        )
    })?;
    let filter = Filter {
        query: params.query.as_deref().filter(|q| !q.trim().is_empty()),
        category: params.category.as_deref().filter(|c| !c.is_empty()),
        min_votes: params.min_votes,
    };
    let (rows, total) = store::search(&db, &filter, sort, params.offset, params.limit)
        .await
        .map_err(failed)?;
    Ok(json(
        StatusCode::OK,
        &AppListOut {
            total,
            offset: params.offset.max(0),
            limit: params.limit.clamp(1, 100),
            items: rows.into_iter().map(AppOut::from).collect(),
        },
    ))
}

/// What the catalogue can be browsed by: each category and how many apps it holds, the biggest first.
#[get("/v1/categories")]
pub async fn list_categories(Inject(db): Inject<Db>) -> Result<Response, ApiError> {
    let rows = store::categories(&db).await.map_err(failed)?;
    Ok(json(
        StatusCode::OK,
        &CategoryListOut {
            items: rows
                .into_iter()
                .map(|(name, count)| CategoryOut { name, count })
                .collect(),
        },
    ))
}

#[get("/v1/apps/{package_name}")]
pub async fn get_app(
    Inject(db): Inject<Db>,
    Path(package_name): Path<String>,
) -> Result<Response, ApiError> {
    match store::get(&db, &package_name).await.map_err(failed)? {
        Some(app) => Ok(json(StatusCode::OK, &AppOut::from(app))),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, "not found")),
    }
}

pub fn register_routes() {
    let _ = list_apps as fn(_, _) -> _;
    let _ = list_categories as fn(_) -> _;
    let _ = get_app as fn(_, _) -> _;
}
