//! Application tokens - a person's own long-lived, revocable credentials for command-line tools.
//!
//! The secret is shown once at creation and stored only as a hash; a tool
//! exchanges it at `POST /token` (`grant_type=app_token`) for a short-lived
//! access token, optionally narrowed to a few `service:action` grants.

use crate::api::auth::{TokenResponse, user_audiences, user_scope};
use crate::api::oauth::{TokenRequest, random_code};
use crate::api::users::SubjectClaims;
use crate::app_tokens::{AppTokenRow, TOKEN_PREFIX};
use crate::clients::hash_secret;
use quench_auth::domain::auth::{User, UserDb};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::time::{Duration, Time};
use quench_db::prelude::{Crud, Db};
use quench_http::http::StatusCode;
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, post};
use quench_serde::{Deserialize, Serialize};

/// Upper bound on a token's life; a tool that needs longer mints a new one.
const MAX_LIFETIME_DAYS: i64 = 365;

#[derive(Debug, Deserialize)]
pub struct CreateAppToken {
    pub name: String,
    /// `service:action` entries the token is limited to; empty is the owner's full access.
    #[serde(default)]
    pub scope: Vec<String>,
    /// Lifetime in days; omitted means the maximum.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppTokenView {
    pub id: String,
    pub name: String,
    pub scope: Vec<String>,
    pub created_at: Time,
    pub expires_at: Option<Time>,
    pub last_used_at: Option<Time>,
    pub revoked: bool,
}

impl From<&AppTokenRow> for AppTokenView {
    fn from(row: &AppTokenRow) -> Self {
        Self {
            id: row.id.clone(),
            name: row.name.clone(),
            scope: row.entries(),
            created_at: row.created_at,
            expires_at: row.expires_at,
            last_used_at: row.last_used_at,
            revoked: row.revoked_at.is_some(),
        }
    }
}

/// The response to creating a token: the only time the secret is visible.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatedAppToken {
    #[serde(flatten)]
    pub token: AppTokenView,
    pub secret: String,
}

fn json(status: StatusCode, value: &impl Serialize) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn problem(status: StatusCode, message: &str) -> Response {
    json(status, &quench_serde::json!({ "error": message }))
}

fn valid_entry(entry: &str) -> bool {
    entry
        .split_once(':')
        .is_some_and(|(service, action)| !service.is_empty() && !action.is_empty())
}

/// Why a create request is refused, if it is.
#[must_use]
pub fn refusal(body: &CreateAppToken, user: &User) -> Option<String> {
    if body.name.trim().is_empty() {
        return Some("a token needs a name".to_string());
    }
    if body.expires_in_days.is_some_and(|days| days < 1) {
        return Some("expires_in_days must be at least 1".to_string());
    }
    body.scope.iter().find_map(|entry| {
        if !valid_entry(entry) {
            return Some(format!("'{entry}' is not a service:action entry"));
        }
        let (service, action) = entry.split_once(':')?;
        (!user.can(service, action)).then(|| format!("you do not hold {entry}"))
    })
}

#[must_use]
pub fn new_row(username: &str, body: &CreateAppToken, now: Time) -> (AppTokenRow, String) {
    let secret = format!("{TOKEN_PREFIX}{}", random_code());
    let days = body
        .expires_in_days
        .unwrap_or(MAX_LIFETIME_DAYS)
        .min(MAX_LIFETIME_DAYS);
    let row = AppTokenRow {
        token_hash: hash_secret(&secret),
        id: random_code().chars().take(12).collect(),
        username: username.to_string(),
        name: body.name.trim().to_string(),
        scope: body.scope.join(" "),
        created_at: now,
        expires_at: Some(now + Duration::days(days)),
        last_used_at: None,
        revoked_at: None,
    };
    (row, secret)
}

#[post("/api/v1/me/tokens")]
async fn create_token(
    subject: SubjectClaims,
    Json(body): Json<CreateAppToken>,
    Inject(db): Inject<Db>,
    Inject(users): Inject<UserDb>,
) -> Response {
    let claims = subject.0;
    // A token minted from an app token could never be revoked by revoking the
    // first, so creation needs a login session behind it.
    if claims.sid.is_none() {
        return problem(
            StatusCode::FORBIDDEN,
            "application tokens are created from a signed-in session",
        );
    }
    let Some(user) = users.get_user(&claims.sub).await else {
        return problem(StatusCode::UNAUTHORIZED, "unknown user");
    };
    if let Some(reason) = refusal(&body, &user) {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, &reason);
    }

    let (row, secret) = new_row(&user.username, &body, Time::now());
    if let Err(err) = db.repository::<AppTokenRow>().create(&row).await {
        quench_log::error!("failed to store an application token: {err}");
        return Response::new(StatusCode::INTERNAL_SERVER_ERROR);
    }
    json(
        StatusCode::CREATED,
        &CreatedAppToken {
            token: AppTokenView::from(&row),
            secret,
        },
    )
}

#[get("/api/v1/me/tokens")]
async fn list_tokens(subject: SubjectClaims, Inject(db): Inject<Db>) -> Response {
    match db
        .repository::<AppTokenRow>()
        .find_by("username", &subject.0.sub)
        .await
    {
        Ok(rows) => json(
            StatusCode::OK,
            &rows.iter().map(AppTokenView::from).collect::<Vec<_>>(),
        ),
        Err(err) => {
            quench_log::error!("failed to list application tokens: {err}");
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[delete("/api/v1/me/tokens/{id}")]
async fn revoke_token(
    subject: SubjectClaims,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    let tokens = db.repository::<AppTokenRow>();
    let Ok(rows) = tokens.find_by("id", &id).await else {
        return Response::new(StatusCode::INTERNAL_SERVER_ERROR);
    };
    // Someone else's token answers like an unknown one.
    let Some(mut row) = rows.into_iter().find(|row| row.username == subject.0.sub) else {
        return Response::new(StatusCode::NOT_FOUND);
    };
    if row.revoked_at.is_none() {
        row.revoked_at = Some(Time::now());
        if tokens.update(&row).await.is_err() {
            return Response::new(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    Response::new(StatusCode::NO_CONTENT)
}

/// What the exchanged access token carries: the owner's current grants,
/// narrowed to the token's entries (so a later demotion still applies).
fn exchange_claims(config: &JwtConfig, user: &User, row: &AppTokenRow) -> (Vec<String>, String) {
    let entries = row.entries();
    if entries.is_empty() {
        return (user_audiences(config, user), user_scope(user));
    }
    let held: Vec<String> = entries
        .into_iter()
        .filter(|entry| {
            entry
                .split_once(':')
                .is_some_and(|(service, action)| user.can(service, action))
        })
        .collect();
    let mut wanted: Vec<String> = held
        .iter()
        .filter_map(|entry| {
            entry
                .split_once(':')
                .map(|(service, _)| service.to_string())
        })
        .collect();
    wanted.push(config.service_name.clone());
    let mut audiences = config.narrow_audiences(&wanted);
    if !audiences.contains(&config.service_name) {
        audiences.push(config.service_name.clone());
    }
    (audiences, held.join(" "))
}

/// `grant_type=app_token`: access-only, no refresh token - the app token is the
/// long-lived half.
pub async fn app_token_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    db: &Db,
    users: &UserDb,
) -> Response {
    let Some(secret) = &body.app_token else {
        return problem(
            StatusCode::BAD_REQUEST,
            "app_token grant requires app_token",
        );
    };
    let tokens = db.repository::<AppTokenRow>();
    let Ok(Some(mut row)) = tokens.read(&hash_secret(secret)).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };
    let now = Time::now();
    if !row.is_usable(now) {
        return Response::new(StatusCode::UNAUTHORIZED);
    }
    let Some(user) = users.get_user(&row.username).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };

    let (audiences, scope) = exchange_claims(config, &user, &row);
    row.last_used_at = Some(now);
    if let Err(err) = tokens.update(&row).await {
        quench_log::warn!("could not record application token use: {err}");
    }
    match config
        .issue_access_token_for(user.username.clone(), audiences, scope, None)
        .await
    {
        Ok(access_token) => json(
            StatusCode::OK,
            &TokenResponse {
                access_token,
                refresh_token: String::new(),
                token_type: "Bearer".to_string(),
                expires_in: config.access_token_ttl_secs,
            },
        ),
        Err(err) => {
            quench_log::error!("failed to issue a token for {}: {err}", user.username);
            Response::new(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn register_routes() {
    let _ = create_token as fn(_, _, _, _) -> _;
    let _ = list_tokens as fn(_, _) -> _;
    let _ = revoke_token as fn(_, _, _) -> _;
}
