//! Realm user administration API - JSON surface over [`crate::realm`]; each route needs a catalog action.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use async_trait::async_trait;
use http::StatusCode;
use quench_auth::domain::auth::{Permissions, Role, User, UserDb};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::realm as auth_realm;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::domain::cookies::cookie_value;
use quench_db::prelude::Db;
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Json, Path, Response, delete, get, patch, post, put,
};
use serde::{Deserialize, Serialize};

// --- Wire types ---

/// Separate from `User` (not a skipped field) so "the hash never leaves" is
/// a property of the type, not a derive attribute someone could remove.
#[derive(Serialize, Deserialize)]
pub struct UserView {
    pub username: String,
    pub roles: Vec<Role>,
    pub permissions: Permissions,
    /// True when a role grants everything.
    pub wildcard: bool,
    pub email: Option<String>,
    pub email_verified: bool,
}

impl From<&User> for UserView {
    fn from(user: &User) -> Self {
        Self {
            username: user.username.clone(),
            roles: user.get_roles(),
            permissions: user.get_permissions(),
            wildcard: user.has_wildcard(),
            email: user.email.clone(),
            email_verified: user.email_verified_at.is_some(),
        }
    }
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub email: Option<String>,
}

/// Every field optional, so a `PATCH` can touch just one.
#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub password: Option<String>,
    pub roles: Option<Vec<Role>>,
    pub permissions: Option<Permissions>,
}

#[derive(Deserialize)]
pub struct ReplacePermissionsRequest {
    #[serde(default)]
    pub permissions: Permissions,
}

/// What the caller may do, wildcard already resolved - unlike `/userinfo`,
/// which just reports the token's literal role.
#[derive(Serialize, Deserialize)]
pub struct Me {
    pub username: String,
    pub roles: Vec<Role>,
    pub wildcard: bool,
    /// Per-service actions granted (or every catalog action, for a wildcard).
    pub effective: Permissions,
}

#[derive(Deserialize)]
pub struct ApplyTemplateRequest {
    pub template: String,
}

// --- Guards ---

/// A verified realm token - not behind `Auth` middleware (gatehouse mints the
/// token, so can't require one), so the checks happen here instead.
pub struct SubjectClaims(pub Claims);

#[async_trait]
impl FromRequest for SubjectClaims {
    async fn from_request(req: &mut quench_http::prelude::Request) -> Result<Self, HttpError> {
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Auth off is the estate-wide dev switch - no token to read.
        if !config.auth_enabled {
            tracing::warn!(
                "SERVICE_AUTH_ENABLED is off: serving {} unauthenticated",
                req.uri().path()
            );
            return Ok(Self(Claims::for_audiences(
                "anonymous".to_string(),
                vec![config.service_name.clone()],
                Role::Admin.as_str().to_string(),
                None,
                60,
            )));
        }

        let token = bearer_token(req)
            .or_else(|| cookie_value(req, &auth_realm::session_cookie_name()))
            .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED, ""))?;

        let claims = config
            .decode_claims(&token)
            .await
            .map_err(|_| HttpError::status(StatusCode::UNAUTHORIZED, ""))?;
        if !claims.allows(&config.service_name) {
            return Err(HttpError::status(StatusCode::UNAUTHORIZED, ""));
        }

        // Honour revocation - a logged-out session shouldn't keep managing users.
        if let Some(session_id) = claims.sid.as_deref() {
            let sessions = req
                .container()
                .get::<SessionDb>()
                .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if !sessions
                .is_active(session_id, &claims.sub)
                .await
                .unwrap_or(false)
            {
                return Err(HttpError::status(StatusCode::UNAUTHORIZED, ""));
            }
        }

        Ok(Self(claims))
    }
}

/// A verified token authorized for one `gatehouse` catalog action -
/// `Claims::can` treats a wildcard role as satisfying any action.
macro_rules! action_claims {
    ($name:ident, $action:literal) => {
        // Some routes only need the gate, never the claims themselves.
        #[allow(dead_code)]
        pub struct $name(pub Claims);

        #[async_trait]
        impl FromRequest for $name {
            async fn from_request(
                req: &mut quench_http::prelude::Request,
            ) -> Result<Self, HttpError> {
                let SubjectClaims(claims) = SubjectClaims::from_request(req).await?;
                if !claims.can("gatehouse", $action) {
                    tracing::warn!(
                        "{} lacks gatehouse:{}; refusing user administration",
                        claims.sub,
                        $action
                    );
                    return Err(HttpError::status(StatusCode::FORBIDDEN, ""));
                }
                Ok(Self(claims))
            }
        }
    };
}

action_claims!(ReadUsersClaims, "read-users");
action_claims!(CreateUserClaims, "create-user");
action_claims!(EditUserClaims, "edit-user");
action_claims!(DeleteUserClaims, "delete-user");
// Guards `POST /api/v1/admin/keys/rotate` in `crate::api::jwks`.
action_claims!(ManageSigningKeysClaims, "manage-signing-keys");
action_claims!(ManagePermissionsClaims, "manage-permissions");

pub(crate) fn bearer_token(req: &quench_http::prelude::Request) -> Option<String> {
    req.header("authorization")?
        .strip_prefix("Bearer ")
        .map(str::to_string)
}

// --- Routes: thin, all rules live in `crate::realm` ---

#[get("/api/v1/users")]
async fn list_users(_actor: ReadUsersClaims, Inject(db): Inject<Db>) -> Response {
    match realm::list(&db).await {
        Ok(users) => {
            let views: Vec<UserView> = users.iter().map(UserView::from).collect();
            json_ok(&views)
        }
        Err(err) => problem(&err),
    }
}

#[get("/api/v1/users/{username}")]
async fn get_user(
    _actor: ReadUsersClaims,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    match realm::get(&db, &username).await {
        Ok(user) => json_ok(&UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[post("/api/v1/users")]
async fn create_user(
    actor: CreateUserClaims,
    Json(request): Json<CreateUserRequest>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
) -> Response {
    match realm::create(
        &db,
        &catalog,
        actor.0.has_role(Role::Admin.as_str()),
        &request.username,
        &request.password,
        request.roles,
        request.permissions,
        request.email,
    )
    .await
    {
        Ok(user) => Response::json(StatusCode::CREATED, &UserView::from(&user))
            .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(err) => problem(&err),
    }
}

#[patch("/api/v1/users/{username}")]
async fn update_user(
    actor: EditUserClaims,
    Path(username): Path<String>,
    Json(request): Json<UpdateUserRequest>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    let changes = UserChanges {
        password: request.password,
        roles: request.roles,
        permissions: request.permissions,
        ..UserChanges::default()
    };

    match realm::update(
        &db,
        &catalog,
        &sessions,
        &actor.0.sub,
        actor.0.has_role(Role::Admin.as_str()),
        &username,
        changes,
    )
    .await
    {
        Ok(user) => json_ok(&UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[put("/api/v1/users/{username}/permissions")]
async fn replace_permissions(
    actor: ManagePermissionsClaims,
    Path(username): Path<String>,
    Json(request): Json<ReplacePermissionsRequest>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    match realm::replace_permissions(
        &db,
        &catalog,
        &sessions,
        &actor.0.sub,
        &username,
        request.permissions,
    )
    .await
    {
        Ok(user) => json_ok(&UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[post("/api/v1/users/{username}/template")]
async fn apply_template(
    actor: ManagePermissionsClaims,
    Path(username): Path<String>,
    Json(request): Json<ApplyTemplateRequest>,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    match realm::apply_template(
        &db,
        &catalog,
        &sessions,
        &actor.0.sub,
        &username,
        &request.template,
    )
    .await
    {
        Ok(user) => json_ok(&UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[delete("/api/v1/users/{username}")]
async fn delete_user(
    actor: DeleteUserClaims,
    Path(username): Path<String>,
    Inject(db): Inject<Db>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    match realm::delete(&db, &sessions, &actor.0.sub, &username).await {
        Ok(()) => Response::new(StatusCode::NO_CONTENT),
        Err(err) => problem(&err),
    }
}

/// The caller's own effective access - any authenticated user, for page rendering.
#[get("/api/v1/me")]
async fn me(
    subject: SubjectClaims,
    Inject(catalog): Inject<PermissionCatalog>,
    Inject(users): Inject<UserDb>,
) -> Response {
    let claims = subject.0;

    // Read the user, not the token's scope, so a grant added since minting shows up.
    let user = users.get_user(&claims.sub).await;
    let (roles, wildcard) = match &user {
        Some(user) => (user.get_roles(), user.has_wildcard()),
        None => (
            claims
                .roles()
                .iter()
                .filter_map(|entry| Role::parse(entry))
                .collect(),
            claims.has_wildcard(),
        ),
    };

    let granted = user
        .as_ref()
        .map(User::get_permissions)
        .unwrap_or_else(|| claims.permissions());

    let effective = catalog
        .service_names()
        .filter_map(|service| {
            // Wildcard reaches every catalog action without any being stored.
            let actions = if wildcard {
                catalog.actions_for(service).iter().cloned().collect()
            } else {
                granted.get(service).cloned().unwrap_or_default()
            };
            (!actions.is_empty()).then(|| (service.to_string(), actions))
        })
        .collect();

    json_ok(&Me {
        username: claims.sub,
        roles,
        wildcard,
        effective,
    })
}

pub fn register_routes() {
    let _ = list_users as fn(_, _) -> _;
    let _ = create_user as fn(_, _, _, _) -> _;
    let _ = get_user as fn(_, _, _) -> _;
    let _ = update_user as fn(_, _, _, _, _, _) -> _;
    let _ = replace_permissions as fn(_, _, _, _, _, _) -> _;
    let _ = apply_template as fn(_, _, _, _, _, _) -> _;
    let _ = delete_user as fn(_, _, _, _) -> _;
    let _ = me as fn(_, _, _) -> _;
}

#[derive(Serialize, Deserialize)]
pub struct Problem {
    pub error: String,
}

fn json_ok<T: Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Machine-readable reason alongside the status - a 409 alone doesn't say which rule.
fn problem(err: &RealmError) -> Response {
    Response::json(
        err.status(),
        &Problem {
            error: err.message(),
        },
    )
    .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}
