//! `/api/v1/admin/...`: the administrator's panel. The gate has already required `mailroom:admin`.

use super::{Caller, done, reply};
use crate::admin;
use crate::app::App;
use mailroom_core::admin::SettingsInput;
use quench_http::prelude::{Inject, Json, Limited, Path, Query, Response, delete, get, post, put};
use quench_serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[get("/api/v1/admin/stats")]
pub async fn stats(Inject(app): Inject<App>) -> Response {
    reply(admin::stats(&app).await)
}

#[get("/api/v1/admin/settings")]
pub async fn settings(Inject(app): Inject<App>) -> Response {
    reply(admin::settings(&app))
}

#[put("/api/v1/admin/settings")]
pub async fn update_settings(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<SettingsInput>, { super::JSON_LIMIT }>,
) -> Response {
    reply(admin::update_settings(&app, &caller.name, &body).await)
}

#[get("/api/v1/admin/mailboxes")]
pub async fn mailboxes(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    reply(admin::mailboxes(&app, q.offset.unwrap_or(0), q.limit.unwrap_or(50)).await)
}

#[post("/api/v1/admin/mailboxes/{owner}/disable")]
pub async fn disable(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(owner): Path<String>,
) -> Response {
    done(admin::set_disabled(&app, &caller.name, &owner, true).await)
}

#[post("/api/v1/admin/mailboxes/{owner}/enable")]
pub async fn enable(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(owner): Path<String>,
) -> Response {
    done(admin::set_disabled(&app, &caller.name, &owner, false).await)
}

#[delete("/api/v1/admin/mailboxes/{owner}")]
pub async fn remove_mailbox(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(owner): Path<String>,
) -> Response {
    done(admin::delete_mailbox(&app, &caller.name, &owner).await)
}

#[get("/api/v1/admin/deliveries")]
pub async fn deliveries(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    reply(admin::deliveries(&app, q.offset.unwrap_or(0), q.limit.unwrap_or(50)).await)
}

#[get("/api/v1/admin/audit")]
pub async fn audit(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    reply(admin::audit(&app, q.offset.unwrap_or(0), q.limit.unwrap_or(50)).await)
}

pub fn register_routes() {
    let _ = stats as fn(_) -> _;
    let _ = settings as fn(_) -> _;
    let _ = update_settings as fn(_, _, _) -> _;
    let _ = mailboxes as fn(_, _) -> _;
    let _ = disable as fn(_, _, _) -> _;
    let _ = enable as fn(_, _, _) -> _;
    let _ = remove_mailbox as fn(_, _, _) -> _;
    let _ = deliveries as fn(_, _) -> _;
    let _ = audit as fn(_, _) -> _;
}
