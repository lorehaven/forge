//! The admin API: `/api/v1/admin/...`. See `songbook_core::api` for the shapes.

use crate::app::App;
use crate::index::{browse, follow, songs};
use crate::routers::api::{json_error, ok};
use crate::{convert, download, pipeline, similar, sync, youtube};
use quench_http::prelude::{Inject, Json, Query, Response, get, http::StatusCode, post};
use quench_serde::Deserialize;
use songbook_core::{
    AddArtists, Added, BlacklistBody, BlacklistEntry, BlacklistRemove, BlacklistSave, CategoryBody,
    Filter, GenresBody, JobStart, LinkBody, NewCategory, RemoveArtist, Removed, RenameCategory,
    Retagged, Schedule, Started, Updated,
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
        Ok(snapshot) if filter.sort == "suggested" => {
            // Which songs are on disk is looked at now, as for the statistics.
            let data = app.config.data.clone();
            match tokio::task::spawn_blocking(move || {
                snapshot.browse_on_disk(&filter, &crate::files::file_index(&data))
            })
            .await
            {
                Ok(browse) => ok(&browse),
                Err(e) => server_error(e),
            }
        }
        Ok(snapshot) => ok(&snapshot.browse(&filter)),
        Err(e) => server_error(e),
    }
}

/// How big the library is and how much of it is on disk. The files are looked at on every call: they change without the
/// index knowing (a download, a file put there by hand).
#[get("/api/v1/admin/stats")]
pub async fn stats(Inject(app): Inject<App>) -> Response {
    let snapshot = match app.index.snapshot().await {
        Ok(s) => s,
        Err(e) => return server_error(e),
    };
    let data = app.config.data.clone();
    match tokio::task::spawn_blocking(move || snapshot.stats(&crate::files::file_index(&data)))
        .await
    {
        Ok(stats) => ok(&stats),
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
        .run(async move |db| browse::set_link(db, body.track_id, &body.url).await)
        .await
    {
        Ok(Ok(updated)) => ok(&Updated { updated }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

/// Puts the audio of another YouTube link in place of a song's file (a wrong song, or the wrong version), as a job.
#[post("/api/v1/admin/replace")]
pub async fn replace(Inject(app): Inject<App>, Json(body): Json<LinkBody>) -> Response {
    if !browse::is_link(body.url.trim()) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "the link must start with http:// or https://",
        );
    }
    let worker = app.clone();
    let started = app.jobs.start("replace", move |log| async move {
        crate::replace::run(&worker, body.track_id, &body.url, &log).await
    });
    ok(&Started { started })
}

/// Every album kept out of the syncs.
#[get("/api/v1/admin/blacklist")]
pub async fn blacklist_list(Inject(app): Inject<App>) -> Response {
    match app
        .index
        .run(async |db| follow::blacklist_entries(db).await)
        .await
    {
        Ok(rows) => ok(&rows
            .into_iter()
            .map(|(id, artist, title)| BlacklistEntry { id, artist, title })
            .collect::<Vec<_>>()),
        Err(e) => server_error(e),
    }
}

/// Adds an album to the blacklist by name, or changes an entry.
#[post("/api/v1/admin/blacklist")]
pub async fn blacklist_save(Inject(app): Inject<App>, Json(body): Json<BlacklistSave>) -> Response {
    if app.jobs.running() {
        return busy();
    }
    match app
        .index
        .run(async move |db| browse::blacklist_save(db, body.id, &body.artist, &body.title).await)
        .await
    {
        Ok(Ok(removed)) => ok(&Removed { removed }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

/// Lets an album back in; the next sync brings it.
#[post("/api/v1/admin/blacklist/remove")]
pub async fn blacklist_remove(
    Inject(app): Inject<App>,
    Json(body): Json<BlacklistRemove>,
) -> Response {
    match app
        .index
        .run(async move |db| follow::blacklist_remove(db, body.id).await)
        .await
    {
        Ok(true) => ok(&Removed { removed: 1 }),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "unknown entry"),
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
        .run(async move |db| browse::blacklist_album(db, &body.artist, body.album_id).await)
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
        .run(async move |db| {
            db.transaction(
                async |db| -> crate::index::Result<
                    std::result::Result<Vec<String>, follow::Refused>,
                > {
                    let mut added = Vec::new();
                    for item in &body.items {
                        match follow::add_follow(db, &item.name, &item.id, &item.category).await? {
                            Ok(true) => added.push(item.name.clone()),
                            Ok(false) => {}
                            Err(e) => return Ok(Err(e)),
                        }
                    }
                    Ok(Ok(added))
                },
            )
            .await
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
        .run(async move |db| follow::remove_follow(db, &body.name).await)
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
        .run(async move |db| {
            db.transaction(
                async |db| -> crate::index::Result<std::result::Result<u32, follow::Refused>> {
                    if let Err(e) = follow::set_category(db, &body.name, &body.category).await? {
                        return Ok(Err(e));
                    }
                    let cmap = follow::category_map(db).await?;
                    Ok(Ok(songs::retag(db, &cmap).await?))
                },
            )
            .await
        })
        .await;
    match result {
        Ok(Ok(retagged)) => ok(&Retagged { retagged }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/artists/genres")]
pub async fn set_genres(Inject(app): Inject<App>, Json(body): Json<GenresBody>) -> Response {
    match app
        .index
        .run(async move |db| follow::set_genres(db, &body.name, &body.genres).await)
        .await
    {
        Ok(Ok(())) => ok(&quench_serde::Map::new()),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

#[post("/api/v1/admin/categories/add")]
pub async fn add_category(Inject(app): Inject<App>, Json(body): Json<NewCategory>) -> Response {
    let name = body.name.clone();
    match app
        .index
        .run(async move |db| follow::add_category(db, &body.name).await)
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

#[post("/api/v1/admin/categories/rename")]
pub async fn rename_category(
    Inject(app): Inject<App>,
    Json(body): Json<RenameCategory>,
) -> Response {
    if app.jobs.running() {
        return busy();
    }
    let result = app
        .index
        .run(async move |db| {
            db.transaction(
                async |db| -> crate::index::Result<std::result::Result<u32, follow::Refused>> {
                    if let Err(e) = follow::rename_category(db, &body.name, &body.new_name).await? {
                        return Ok(Err(e));
                    }
                    let cmap = follow::category_map(db).await?;
                    Ok(Ok(songs::retag(db, &cmap).await?))
                },
            )
            .await
        })
        .await;
    match result {
        Ok(Ok(retagged)) => ok(&Retagged { retagged }),
        Ok(Err(e)) => refused(&e),
        Err(e) => server_error(e),
    }
}

/// The jobs `POST /jobs` starts. Every one has a branch in [`start_job`]; a name missing here is refused as unknown (as
/// `purge` was, so the page's delete button never worked).
pub const JOBS: [&str; 11] = [
    "sync",
    "tags",
    "similar",
    "youtube",
    "download",
    "convert",
    "images",
    "purge",
    "fix-links",
    "pipeline",
    "scheduled",
];

#[post("/api/v1/admin/jobs")]
pub async fn start_job(Inject(app): Inject<App>, Json(body): Json<JobStart>) -> Response {
    if !JOBS.contains(&body.name.as_str()) {
        return json_error(StatusCode::BAD_REQUEST, "unknown job");
    }
    if body.name == "scheduled" {
        return ok(&Started {
            started: crate::schedule::start_now(&app).await,
        });
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
            "images" => crate::images::run(&worker, body.retry_missing, &log).await,
            "pipeline" => pipeline::run(&worker, &body, &log).await,
            "purge" => crate::purge::run(&worker, body.dry_run, &log).await,
            "fix-links" => crate::fixlinks::run(&worker, &body, &log).await,
            _ => convert::run(&worker.config.data, true, &log).await,
        }
    });
    ok(&Started { started })
}

/// The nightly schedule, with when it runs next and how the last run went.
#[get("/api/v1/admin/schedule")]
pub async fn get_schedule(Inject(app): Inject<App>) -> Response {
    match crate::schedule::load(&app).await {
        Ok(s) => ok(&s),
        Err(e) => server_error(e),
    }
}

/// Saves the nightly schedule; answers the saved one.
#[post("/api/v1/admin/schedule")]
pub async fn save_schedule(Inject(app): Inject<App>, Json(body): Json<Schedule>) -> Response {
    if let Err(e) = crate::schedule::check(body.clone()) {
        return json_error(StatusCode::BAD_REQUEST, &e);
    }
    match crate::schedule::save(&app, body).await {
        Ok(s) => ok(&s),
        Err(e) => server_error(e),
    }
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
    let _ = stats as fn(_) -> _;
    let _ = link as fn(_, _) -> _;
    let _ = replace as fn(_, _) -> _;
    let _ = blacklist as fn(_, _) -> _;
    let _ = blacklist_list as fn(_) -> _;
    let _ = blacklist_save as fn(_, _) -> _;
    let _ = blacklist_remove as fn(_, _) -> _;
    let _ = add_artists as fn(_, _) -> _;
    let _ = remove_artist as fn(_, _) -> _;
    let _ = set_category as fn(_, _) -> _;
    let _ = set_genres as fn(_, _) -> _;
    let _ = add_category as fn(_, _) -> _;
    let _ = rename_category as fn(_, _) -> _;
    let _ = start_job as fn(_, _) -> _;
    let _ = job as fn(_, _) -> _;
    let _ = get_schedule as fn(_) -> _;
    let _ = save_schedule as fn(_, _) -> _;
}
