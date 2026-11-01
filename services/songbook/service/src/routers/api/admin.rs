//! The admin API: `/api/v1/admin/...`. See `songbook_core::api` for the shapes.

use crate::app::App;
use crate::index::{browse, follow, songs};
use crate::routers::api::{json_error, ok};
use crate::{convert, download, similar, sync, youtube};
use quench_http::prelude::{Inject, Json, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use songbook_core::{
    AddArtists, Added, BlacklistBody, CategoryBody, Filter, JobStart, LinkBody, NewCategory,
    RemoveArtist, Removed, Retagged, Started, Updated,
};

fn server_error(e: impl std::fmt::Display) -> Response {
    quench_log::warn!("admin api: {e}");
    json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}

fn busy() -> Response {
    json_error(
        StatusCode::CONFLICT,
        "A job is running; try again when it finishes.",
    )
}

fn refused(e: &follow::Refused) -> Response {
    json_error(StatusCode::BAD_REQUEST, &e.to_string())
}

#[get("/api/v1/admin/state")]
pub async fn state(Inject(app): Inject<App>) -> Response {
    match app.index.snapshot().await {
        Ok(snapshot) => ok(&snapshot.state),
        Err(e) => server_error(e),
    }
}

#[get("/api/v1/admin/browse")]
pub async fn browse_library(Inject(app): Inject<App>, Query(filter): Query<Filter>) -> Response {
    match app.index.snapshot().await {
        Ok(snapshot) => ok(&snapshot.browse(&filter)),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/link")]
pub async fn link(Inject(app): Inject<App>, Json(body): Json<LinkBody>) -> Response {
    if app.jobs.running() {
        return busy();
    }
    match app
        .index
        .run(move |db| browse::set_link(db, body.track_id, &body.url))
        .await
    {
        Ok(Ok(updated)) => ok(&Updated { updated }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/albums/blacklist")]
pub async fn blacklist(Inject(app): Inject<App>, Json(body): Json<BlacklistBody>) -> Response {
    if app.jobs.running() {
        return busy();
    }
    match app
        .index
        .run(move |db| browse::blacklist_album(db, &body.artist, body.album_id))
        .await
    {
        Ok(Ok(removed)) => ok(&Removed { removed }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/artists/add")]
pub async fn add_artists(Inject(app): Inject<App>, Json(body): Json<AddArtists>) -> Response {
    let result = app
        .index
        .run(move |db| {
            db.transaction(|db| {
                let mut added = Vec::new();
                for item in &body.items {
                    match follow::add_follow(db, &item.name, &item.id, &item.category)? {
                        Ok(true) => added.push(item.name.clone()),
                        Ok(false) => {}
                        Err(e) => return Ok(Err(e)),
                    }
                }
                Ok(Ok(added))
            })
        })
        .await;
    match result {
        Ok(Ok(added)) => ok(&Added { added }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/artists/remove")]
pub async fn remove_artist(Inject(app): Inject<App>, Json(body): Json<RemoveArtist>) -> Response {
    match app
        .index
        .run(move |db| follow::remove_follow(db, &body.name))
        .await
    {
        Ok(()) => ok(&quench_serde::Map::new()),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/artists/category")]
pub async fn set_category(Inject(app): Inject<App>, Json(body): Json<CategoryBody>) -> Response {
    if app.jobs.running() {
        return busy();
    }
    let result = app
        .index
        .run(move |db| {
            db.transaction(|db| {
                if let Err(e) = follow::set_category(db, &body.name, &body.category)? {
                    return Ok(Err(e));
                }
                let cmap = follow::category_map(db)?;
                Ok(Ok(songs::retag(db, &cmap)?))
            })
        })
        .await;
    match result {
        Ok(Ok(retagged)) => ok(&Retagged { retagged }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/categories/add")]
pub async fn add_category(Inject(app): Inject<App>, Json(body): Json<NewCategory>) -> Response {
    let name = body.name.clone();
    match app
        .index
        .run(move |db| follow::add_category(db, &body.name))
        .await
    {
        Ok(Ok(true)) => ok(&Added {
            added: vec![follow::clean(&name)],
        }),
        Ok(Ok(false)) => ok(&Added { added: Vec::new() }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/jobs")]
pub async fn start_job(Inject(app): Inject<App>, Json(body): Json<JobStart>) -> Response {
    let known = ["sync", "tags", "similar", "youtube", "download", "convert"];
    if !known.contains(&body.name.as_str()) {
        return json_error(StatusCode::BAD_REQUEST, "unknown job");
    }
    let worker = app.clone();
    let name = body.name.clone();
    let started = app.jobs.start(&name, move |log| async move {
        match body.name.as_str() {
            "sync" => sync::run(&worker, body.singles, &log).await,
            "tags" => sync::tags(&worker, &log).await,
            "similar" => {
                similar::run(
                    &worker,
                    body.top.unwrap_or(50),
                    body.per_artist.unwrap_or(20),
                    &body.category,
                    &log,
                )
                .await
            }
            "youtube" => {
                youtube::run(&worker, &body.artist, body.limit, body.retry_missing, &log).await
            }
            "download" => download::run(&worker, &body.artist, body.limit, 3, 5, &log).await,
            _ => convert::run(&worker.config.data, true, &log).await,
        }
    });
    ok(&Started { started })
}

#[derive(Debug, Deserialize)]
pub struct From {
    #[serde(default)]
    pub from: Option<u64>,
}

#[get("/api/v1/admin/job")]
pub async fn job(Inject(app): Inject<App>, Query(query): Query<From>) -> Response {
    ok(&app.jobs.read(query.from.unwrap_or(0)))
}

pub fn register_routes() {
    let _ = state as fn(_) -> _;
    let _ = browse_library as fn(_, _) -> _;
    let _ = link as fn(_, _) -> _;
    let _ = blacklist as fn(_, _) -> _;
    let _ = add_artists as fn(_, _) -> _;
    let _ = remove_artist as fn(_, _) -> _;
    let _ = set_category as fn(_, _) -> _;
    let _ = add_category as fn(_, _) -> _;
    let _ = start_job as fn(_, _) -> _;
    let _ = job as fn(_, _) -> _;
}
