//! The realm's user administration API.
//!
//! This is one of the two callers of [`crate::realm`], the other being the admin
//! pages. The rules live there; what is here is the JSON surface and the guards.
//!
//! `quench-auth`'s `UserDb` stays read-only on purpose - a relying party needs to
//! read users for the Basic-auth service-to-service path, and must never be able
//! to create one - so all writing happens inside this service.
//!
//! Every route needs one `gatehouse` catalog action - `read-users`,
//! `create-user`, `edit-user`, `delete-user` or `manage-permissions` - enforced
//! by that action's `action_claims!`-generated extractor appearing in the
//! handler's arguments. An extractor rather than a middleware so that a route
//! added to this scope without one does not compile into an open endpoint; the
//! compiler is a better reviewer than mount order.
//!
//! Assigning the `admin` or `service` role is deliberately not one of these
//! actions: it stays gated on holding the literal `admin` role, checked inline
//! in `create_user`/`update_user` and enforced again in `realm::{create,update}`
//! itself. A catalog action that could grant "the power to grant admin" would
//! make `admin` optional rather than the emergency-only role it is meant to be
//! - see `permissions.toml`'s comment on `[services.gatehouse]`.

use crate::catalog::PermissionCatalog;
use crate::realm::{self, RealmError, UserChanges};
use actix_web::{
    FromRequest, HttpRequest, HttpResponse, Responder, delete, get, patch, post, put, web,
};
use futures_util::future::LocalBoxFuture;
use quench_auth::prelude::{Claims, JwtConfig, Permissions, Role, SessionDb, User, UserDb};
use quench_db::prelude::Db;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// A user as the API reports one.
///
/// A separate type rather than `User` with a skipped field: `User` is
/// `Serialize` and reachable from elsewhere, so "the hash never leaves the
/// service" should be a property of the type the route names, not of a
/// derive attribute somebody could remove.
#[derive(Serialize)]
pub struct UserView {
    pub username: String,
    pub roles: Vec<Role>,
    pub permissions: Permissions,
    /// True when a role grants everything, so a caller does not have to know
    /// which roles are wildcards to render the answer.
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

/// Every field optional: a `PATCH` that names only `permissions` leaves the
/// password and roles alone.
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

/// What the caller may do, with any wildcard already applied.
///
/// Not a duplicate of `/api/v1/auth/userinfo`: that reports what the token
/// literally says, which for an admin is `admin` and nothing else. This answers
/// "which actions may I perform on sage" per service, which is what a UI needs
/// to decide whether to render a control.
#[derive(Serialize)]
pub struct Me {
    pub username: String,
    pub roles: Vec<Role>,
    pub wildcard: bool,
    /// One entry per service the caller can reach, each holding the actions
    /// they were granted on it (or every action the catalog declares, for a
    /// wildcard role - see `me`).
    pub effective: Permissions,
}

#[derive(Deserialize)]
pub struct ApplyTemplateRequest {
    pub template: String,
}

// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

/// A verified realm token.
///
/// The user API is not behind the `Auth` middleware - gatehouse cannot be, since
/// it serves the login endpoints that mint the token in the first place - so the
/// checks the middleware would do happen here: signature, expiry, audience, and
/// session liveness. Accepts a bearer token or the realm session cookie, so the
/// admin UI can call these routes from the browser.
pub struct SubjectClaims(pub Claims);

impl FromRequest for SubjectClaims {
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            let Some(config) = req.app_data::<web::Data<JwtConfig>>().cloned() else {
                tracing::error!("JwtConfig missing from app_data; refusing the request");
                return Err(actix_web::error::ErrorInternalServerError(""));
            };

            // Auth off is the estate-wide dev switch: there is no token to read,
            // so requiring one would make the API unusable rather than safe.
            if !config.auth_enabled {
                tracing::warn!(
                    "SERVICE_AUTH_ENABLED is off: serving {} unauthenticated",
                    req.path()
                );
                return Ok(Self(Claims::for_audiences(
                    "anonymous".to_string(),
                    vec![config.service_name.clone()],
                    Role::Admin.as_str().to_string(),
                    None,
                    60,
                )));
            }

            let token = bearer_token(&req)
                .or_else(|| {
                    req.cookie(&quench_auth::prelude::realm::session_cookie_name())
                        .map(|cookie| cookie.value().to_string())
                })
                .ok_or_else(|| actix_web::error::ErrorUnauthorized(""))?;

            let claims = config
                .decode_claims(&token)
                .await
                .map_err(|_| actix_web::error::ErrorUnauthorized(""))?;
            if !claims.allows(&config.service_name) {
                return Err(actix_web::error::ErrorUnauthorized(""));
            }

            // Honour revocation: a logged-out session must not keep managing
            // users for the rest of the access token's lifetime.
            if let Some(session_id) = claims.sid.as_deref() {
                let Some(sessions) = req.app_data::<web::Data<Arc<SessionDb>>>() else {
                    tracing::error!("SessionDb missing from app_data; refusing the request");
                    return Err(actix_web::error::ErrorInternalServerError(""));
                };
                if !sessions
                    .is_active(session_id, &claims.sub)
                    .await
                    .unwrap_or(false)
                {
                    return Err(actix_web::error::ErrorUnauthorized(""));
                }
            }

            Ok(Self(claims))
        })
    }
}

/// A verified token authorized for one `gatehouse` catalog action.
///
/// `Claims::can` already treats a wildcard role (`admin`/`service`) as
/// satisfying any action on any service, `gatehouse` included - so `admin`
/// keeps working for every route below without a separate check, exactly as
/// the "emergency" fallback it is meant to be once a route is normally
/// reached through a narrower, catalog-granted action instead.
macro_rules! action_claims {
    ($name:ident, $action:literal) => {
        // Some routes only need the gate, never the claims themselves (`.0`
        // goes unread wherever the actor's identity does not matter beyond
        // having passed it) - allowed rather than worked around, since an
        // unread field here is a route needing nothing more, not a bug.
        #[allow(dead_code)]
        pub struct $name(pub Claims);

        impl FromRequest for $name {
            type Error = actix_web::Error;
            type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

            fn from_request(
                req: &HttpRequest,
                payload: &mut actix_web::dev::Payload,
            ) -> Self::Future {
                let subject = SubjectClaims::from_request(req, payload);
                Box::pin(async move {
                    let SubjectClaims(claims) = subject.await?;
                    if !claims.can("gatehouse", $action) {
                        tracing::warn!(
                            "{} lacks gatehouse:{}; refusing user administration",
                            claims.sub,
                            $action,
                        );
                        return Err(actix_web::error::ErrorForbidden(""));
                    }
                    Ok(Self(claims))
                })
            }
        }
    };
}

action_claims!(ReadUsersClaims, "read-users");
action_claims!(CreateUserClaims, "create-user");
action_claims!(EditUserClaims, "edit-user");
action_claims!(DeleteUserClaims, "delete-user");
// Guards `POST /api/v1/admin/keys/rotate` - see `crate::api::jwks`.
action_claims!(ManageSigningKeysClaims, "manage-signing-keys");
action_claims!(ManagePermissionsClaims, "manage-permissions");

fn bearer_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------
//
// Thin: every rule lives in `crate::realm`, which the admin pages call too. What
// is left here is turning a request into arguments and a `RealmError` into a
// status.

#[get("")]
async fn list_users(_actor: ReadUsersClaims, db: web::Data<Db>) -> impl Responder {
    match realm::list(&db).await {
        Ok(users) => {
            let views: Vec<UserView> = users.iter().map(UserView::from).collect();
            HttpResponse::Ok().json(views)
        }
        Err(err) => problem(&err),
    }
}

#[get("/{username}")]
async fn get_user(
    _actor: ReadUsersClaims,
    path: web::Path<String>,
    db: web::Data<Db>,
) -> impl Responder {
    match realm::get(&db, &path.into_inner()).await {
        Ok(user) => HttpResponse::Ok().json(UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[post("")]
async fn create_user(
    actor: CreateUserClaims,
    request: web::Json<CreateUserRequest>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
) -> impl Responder {
    let request = request.into_inner();
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
        Ok(user) => HttpResponse::Created().json(UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[patch("/{username}")]
async fn update_user(
    actor: EditUserClaims,
    path: web::Path<String>,
    request: web::Json<UpdateUserRequest>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    let request = request.into_inner();
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
        &path.into_inner(),
        changes,
    )
    .await
    {
        Ok(user) => HttpResponse::Ok().json(UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[put("/{username}/permissions")]
async fn replace_permissions(
    actor: ManagePermissionsClaims,
    path: web::Path<String>,
    request: web::Json<ReplacePermissionsRequest>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    match realm::replace_permissions(
        &db,
        &catalog,
        &sessions,
        &actor.0.sub,
        &path.into_inner(),
        request.into_inner().permissions,
    )
    .await
    {
        Ok(user) => HttpResponse::Ok().json(UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[post("/{username}/template")]
async fn apply_template(
    actor: ManagePermissionsClaims,
    path: web::Path<String>,
    request: web::Json<ApplyTemplateRequest>,
    catalog: web::Data<PermissionCatalog>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    match realm::apply_template(
        &db,
        &catalog,
        &sessions,
        &actor.0.sub,
        &path.into_inner(),
        &request.into_inner().template,
    )
    .await
    {
        Ok(user) => HttpResponse::Ok().json(UserView::from(&user)),
        Err(err) => problem(&err),
    }
}

#[delete("/{username}")]
async fn delete_user(
    actor: DeleteUserClaims,
    path: web::Path<String>,
    db: web::Data<Db>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    match realm::delete(&db, &sessions, &actor.0.sub, &path.into_inner()).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(err) => problem(&err),
    }
}

/// The caller's own effective access. Any authenticated user, not just admins -
/// it is how a page learns what to render.
#[get("")]
async fn me(
    subject: SubjectClaims,
    catalog: web::Data<PermissionCatalog>,
    users: web::Data<Arc<UserDb>>,
) -> impl Responder {
    let claims = subject.0;

    // Read the user rather than trusting the token's scope: a grant added since
    // the token was minted should show up here, which is what makes this the
    // endpoint a UI polls.
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
            // A wildcard reaches every action the catalog declares, without any
            // of them being written down against the user - the same reason
            // `user_scope` emits the role alone.
            let actions = if wildcard {
                catalog.actions_for(service).iter().cloned().collect()
            } else {
                granted.get(service).cloned().unwrap_or_default()
            };
            (!actions.is_empty()).then(|| (service.to_string(), actions))
        })
        .collect();

    HttpResponse::Ok().json(Me {
        username: claims.sub,
        roles,
        wildcard,
        effective,
    })
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/users")
        .service(list_users)
        .service(create_user)
        .service(get_user)
        .service(update_user)
        .service(replace_permissions)
        .service(apply_template)
        .service(delete_user)
}

pub fn me_scope() -> actix_web::Scope {
    web::scope("/api/v1/me").service(me)
}

#[derive(Serialize)]
struct Problem {
    error: String,
}

/// A machine-readable reason alongside the status, since "which rule did I
/// break" is not obvious from a 409 alone.
fn problem(err: &RealmError) -> HttpResponse {
    HttpResponse::build(err.status()).json(Problem {
        error: err.message(),
    })
}
