//! `GET /api/v1/hosts` and `GET /api/v1/hosts/{name}` - what the dashboard shows, as JSON.

use crate::domain::history::History;
use crate::domain::store::{Host, Store};
use crate::routers::api::json_error;
use gauge_core::{HostCard, HostDetail, HostHistory, HostSummary, Range};
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::{Deserialize, Serialize};

fn json(host: &Host) -> HostSummary {
    HostSummary {
        name: host.name.clone(),
        kind: host.kind.as_str().to_string(),
        url: host.url.clone(),
        up: host.up(),
        error: host.error.clone(),
        last_ok: host.last_ok,
        view: host.view.clone(),
    }
}

fn ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/hosts")]
pub async fn list(Inject(store): Inject<Store>) -> Response {
    let hosts: Vec<HostCard> = store
        .all()
        .iter()
        .map(|host| HostCard {
            host: json(host),
            cpu: host.history.iter().map(|p| p.cpu).collect(),
        })
        .collect();
    ok(&hosts)
}

#[get("/api/v1/hosts/{name}")]
pub async fn detail(Inject(store): Inject<Store>, Path(name): Path<String>) -> Response {
    match store.get(&name) {
        Some(host) => ok(&HostDetail {
            host: json(&host),
            history: host.history.iter().cloned().collect(),
        }),
        None => json_error(StatusCode::NOT_FOUND, "no such host"),
    }
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// `24h`, `7d` or `30d`.
    #[serde(default)]
    pub range: Option<String>,
    /// `max` for the highest of each step; the mean otherwise.
    #[serde(default)]
    pub stat: Option<String>,
}

/// The stored history of one host: the points of a range, evenly spaced, a gap where nothing was kept.
#[get("/api/v1/hosts/{name}/history")]
pub async fn history(
    Inject(store): Inject<Store>,
    Inject(history): Inject<History>,
    Path(name): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    if store.get(&name).is_none() {
        return json_error(StatusCode::NOT_FOUND, "no such host");
    }
    let range = match query
        .range
        .as_deref()
        .map_or(Some(Range::Day), Range::parse)
    {
        Some(r) if r != Range::Live => r,
        _ => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "range is one of 24h, 7d, 30d (the last hour is in the host's own answer)",
            );
        }
    };
    if !history.available() {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "history is not available: Gauge has no database connection",
        );
    }
    let max = query.stat.as_deref() == Some("max");
    match history
        .read(&name, range, crate::domain::sources::now(), max)
        .await
    {
        Ok(points) => ok(&HostHistory {
            name,
            range: range.as_str().to_string(),
            step_secs: range.step_secs(),
            points,
        }),
        Err(e) => {
            quench_log::warn!("history: {name}: {e}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the history",
            )
        }
    }
}

/// How full each filesystem is and when it fills, from the stored hourly samples.
#[get("/api/v1/forecast")]
pub async fn forecast(Inject(history): Inject<History>) -> Response {
    if !history.available() {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "history is not available: Gauge has no database connection",
        );
    }
    match history.forecast(crate::domain::sources::now()).await {
        Ok(list) => ok(&list),
        Err(e) => {
            quench_log::warn!("forecast: {e}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the history",
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AlertLogQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

/// The alerts that opened or were open within the retention, newest first: the log behind "since when".
#[get("/api/v1/alerts/history")]
pub async fn alert_log(
    Inject(history): Inject<History>,
    Query(query): Query<AlertLogQuery>,
) -> Response {
    if !history.available() {
        return json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "history is not available: Gauge has no database connection",
        );
    }
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    match history
        .alert_log(
            crate::domain::sources::now(),
            crate::domain::history::retention_days(),
            limit,
        )
        .await
    {
        Ok(list) => ok(&list),
        Err(e) => {
            quench_log::warn!("alert log: {e}");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read the alert log",
            )
        }
    }
}

pub fn register_routes() {
    let _ = alert_log as fn(_, _) -> _;
    let _ = forecast as fn(_) -> _;
    let _ = history as fn(_, _, _, _) -> _;
    let _ = list as fn(_) -> _;
    let _ = detail as fn(_, _) -> _;
}
