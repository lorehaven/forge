//! Who is asking and who can be assigned: what the browser client cannot read from the database itself.

use crate::domain::realm_users;
use crate::routers::api::{ApiError, OptionalClaims, actor};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Response, get, http::StatusCode};
use workbench_core::{Me, User};

#[get("/api/v1/me")]
pub async fn me(OptionalClaims(claims): OptionalClaims) -> Response {
    let me = Me {
        username: actor(claims.as_ref()),
    };
    Response::json(StatusCode::OK, &me)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Every realm user, for the assignee picker.
#[get("/api/v1/users")]
pub async fn users(Inject(db): Inject<Db>) -> Result<Response, ApiError> {
    let users: Vec<User> = realm_users::list_users(&db)
        .await?
        .into_iter()
        .map(|user| User {
            username: user.username,
            display_name: user.display_name,
        })
        .collect();
    Ok(Response::json(StatusCode::OK, &users)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)))
}

pub fn register_routes() {
    let _ = me as fn(_) -> _;
    let _ = users as fn(_) -> _;
}
