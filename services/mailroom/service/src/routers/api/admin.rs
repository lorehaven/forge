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
    pub status: Option<String>,
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
    reply(admin::settings(&app).await)
}

#[put("/api/v1/admin/settings")]
pub async fn update_settings(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<SettingsInput>, { super::JSON_LIMIT }>,
) -> Response {
    reply(admin::update_settings(&app, &caller.name, &body).await)
}

#[post("/api/v1/admin/transport/test")]
pub async fn test_transport(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(admin::test_transport(&app, &caller.name).await)
}

#[get("/api/v1/admin/accounts")]
pub async fn accounts(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    reply(admin::accounts(&app, q.offset.unwrap_or(0), q.limit.unwrap_or(50)).await)
}

#[post("/api/v1/admin/accounts/{id}/disable")]
pub async fn disable(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    done(admin::set_account_disabled(&app, &caller.name, &id, true).await)
}

#[post("/api/v1/admin/accounts/{id}/enable")]
pub async fn enable(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    done(admin::set_account_disabled(&app, &caller.name, &id, false).await)
}

#[delete("/api/v1/admin/accounts/{id}")]
pub async fn remove_account(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
) -> Response {
    done(admin::delete_account(&app, &caller.name, &id).await)
}

#[get("/api/v1/admin/outbox")]
pub async fn outbox(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    reply(
        admin::outbox(
            &app,
            q.status.as_deref(),
            q.offset.unwrap_or(0),
            q.limit.unwrap_or(50),
        )
        .await,
    )
}

#[post("/api/v1/admin/outbox/{id}/retry")]
pub async fn retry(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    done(admin::retry(&app, &caller.name, &id).await)
}

#[delete("/api/v1/admin/outbox/{id}")]
pub async fn remove_outbox(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
) -> Response {
    done(admin::remove_outbox(&app, &caller.name, &id).await)
}

#[get("/api/v1/admin/audit")]
pub async fn audit(Inject(app): Inject<App>, Query(q): Query<PageQuery>) -> Response {
    let result = async {
        let (items, total) = app
            .store
            .audit_list(
                q.offset.unwrap_or(0).max(0),
                q.limit.unwrap_or(50).clamp(1, admin::MAX_PAGE),
            )
            .await?;
        Ok(mailroom_core::admin::Page {
            items,
            total: u32::try_from(total).unwrap_or(0),
        })
    }
    .await;
    reply(result)
}

pub fn register_routes() {
    let _ = stats as fn(_) -> _;
    let _ = settings as fn(_) -> _;
    let _ = update_settings as fn(_, _, _) -> _;
    let _ = test_transport as fn(_, _) -> _;
    let _ = accounts as fn(_, _) -> _;
    let _ = disable as fn(_, _, _) -> _;
    let _ = enable as fn(_, _, _) -> _;
    let _ = remove_account as fn(_, _, _) -> _;
    let _ = outbox as fn(_, _) -> _;
    let _ = retry as fn(_, _, _) -> _;
    let _ = remove_outbox as fn(_, _, _) -> _;
    let _ = audit as fn(_, _) -> _;
}
