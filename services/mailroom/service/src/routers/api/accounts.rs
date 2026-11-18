//! `/api/v1/me`, `/presets` and `/accounts`: who the person is and their mailboxes.

use super::{Caller, created, done, reply};
use crate::app::App;
use crate::{accounts, mailbox};
use mailroom_core::{AccountInput, Me};
use quench_http::prelude::{Inject, Json, Limited, Path, Response, delete, get, post, put};

#[get("/api/v1/me")]
pub async fn me(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(Ok(Me {
        username: caller.name,
        can_write: caller.write,
        admin: caller.admin,
        max_accounts: app.settings().max_accounts_per_user,
    }))
}

#[get("/api/v1/presets")]
pub async fn presets() -> Response {
    reply(Ok(accounts::presets()))
}

#[get("/api/v1/accounts")]
pub async fn list(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(accounts::list(&app, &caller.name).await)
}

#[post("/api/v1/accounts")]
pub async fn create(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(input)): Limited<Json<AccountInput>, { super::JSON_LIMIT }>,
) -> Response {
    created(accounts::create(&app, &caller.name, &input).await)
}

#[post("/api/v1/account-tests")]
pub async fn test(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(input)): Limited<Json<AccountInput>, { super::JSON_LIMIT }>,
) -> Response {
    reply(accounts::test(&app, &caller.name, &input).await)
}

#[put("/api/v1/accounts/{id}")]
pub async fn update(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
    Limited(Json(input)): Limited<Json<AccountInput>, { super::JSON_LIMIT }>,
) -> Response {
    reply(accounts::update(&app, &caller.name, &id, &input).await)
}

#[delete("/api/v1/accounts/{id}")]
pub async fn remove(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    done(accounts::delete(&app, &caller.name, &id).await)
}

#[post("/api/v1/accounts/{id}/refresh")]
pub async fn refresh(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    let result = async {
        mailbox::refresh(&app, &caller.name, &id).await?;
        let rows = accounts::list(&app, &caller.name).await?;
        rows.into_iter()
            .find(|a| a.id == id)
            .ok_or(crate::error::Error::NotFound)
    }
    .await;
    reply(result)
}

#[get("/api/v1/accounts/{id}/folders")]
pub async fn folders(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    reply(mailbox::folders(&app, &caller.name, &id).await)
}

pub fn register_routes() {
    let _ = me as fn(_, _) -> _;
    let _ = presets as fn() -> _;
    let _ = list as fn(_, _) -> _;
    let _ = create as fn(_, _, _) -> _;
    let _ = test as fn(_, _, _) -> _;
    let _ = update as fn(_, _, _, _) -> _;
    let _ = remove as fn(_, _, _) -> _;
    let _ = refresh as fn(_, _, _) -> _;
    let _ = folders as fn(_, _, _) -> _;
}
