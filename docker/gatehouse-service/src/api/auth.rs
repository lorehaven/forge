//! The realm's token API. This lives in gatehouse rather than in `quench-auth`
//! because only gatehouse serves it: a relying party verifies tokens, it never
//! issues them.

use actix_web::{HttpRequest, HttpResponse, Responder, cookie::Cookie, get, post, web};
use quench_auth::prelude::realm;
use quench_auth::prelude::{Claims, JwtConfig, Permissions, Session, SessionDb, User, UserDb};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[post("/login")]
async fn login(
    request: web::Json<LoginRequest>,
    config: web::Data<JwtConfig>,
    users: web::Data<std::sync::Arc<UserDb>>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let Some(user) = users.validate(&request.username, &request.password).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match issue_token_pair(&config, &sessions, &user).await {
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("Failed to create authentication session: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/refresh")]
async fn refresh(
    request: HttpRequest,
    body: Option<web::Json<RefreshRequest>>,
    config: web::Data<JwtConfig>,
    users: web::Data<std::sync::Arc<UserDb>>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    let cookie_refresh_token = request
        .cookie(&realm::refresh_cookie_name())
        .map(|cookie| cookie.value().to_string());
    let cookie_flow = body.is_none() && cookie_refresh_token.is_some();
    let Some(refresh_token) = body
        .map(|request| request.refresh_token.clone())
        .or(cookie_refresh_token)
    else {
        return HttpResponse::BadRequest().finish();
    };
    let rotated = match sessions
        .rotate(&refresh_token, config.refresh_token_ttl_secs)
        .await
    {
        Ok(Some(rotated)) => rotated,
        Ok(None) => return HttpResponse::Unauthorized().finish(),
        Err(err) => {
            tracing::error!("Failed to rotate refresh token: {}", err);
            return HttpResponse::InternalServerError().finish();
        }
    };
    let (session, refresh_token) = rotated;
    let Some(user) = users.get_user(&session.username).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match token_response(&config, &user, &session, refresh_token) {
        Ok(tokens) if cookie_flow => {
            let access_cookie = access_cookie(&config, tokens.access_token.clone());
            let refresh_cookie = refresh_cookie(&config, tokens.refresh_token.clone());
            HttpResponse::Ok()
                .cookie(access_cookie)
                .cookie(refresh_cookie)
                .json(tokens)
        }
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("Failed to issue access token: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/logout")]
async fn logout(
    request: web::Json<RefreshRequest>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    match sessions
        .revoke_by_refresh_token(&request.refresh_token)
        .await
    {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(err) => {
            tracing::error!("Failed to revoke session: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Serialize)]
pub struct UserInfo {
    pub sub: String,
    pub roles: Vec<String>,
    pub audiences: Vec<String>,
    /// The `service:action` grants the token carries. Empty for a wildcard
    /// role - `admin` in `roles` is what says "everything", and `/api/v1/me`
    /// is the endpoint that resolves that into an answer per service.
    pub permissions: Permissions,
}

/// Subject and roles behind a valid access token. Relying parties use this to
/// resolve an identity without reading the realm's tables themselves.
#[get("/userinfo")]
async fn userinfo(
    request: HttpRequest,
    config: web::Data<JwtConfig>,
    sessions: web::Data<SessionDb>,
) -> impl Responder {
    match access_claims(&request, &config, &sessions).await {
        Some(claims) => HttpResponse::Ok().json(UserInfo {
            // Roles only: the permission entries are reported separately rather
            // than mixed into the role list, which is what splitting the raw
            // scope string used to do.
            roles: claims
                .roles()
                .into_iter()
                .filter(|entry| !entry.contains(':'))
                .collect(),
            permissions: claims.permissions(),
            sub: claims.sub,
            audiences: claims.aud,
        }),
        None => HttpResponse::Unauthorized().finish(),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/auth")
        .service(login)
        .service(refresh)
        .service(logout)
        .service(userinfo)
}

pub async fn issue_token_pair(
    config: &JwtConfig,
    sessions: &SessionDb,
    user: &User,
) -> anyhow::Result<TokenResponse> {
    let (session, refresh_token) = sessions
        .create(&user.username, config.refresh_token_ttl_secs)
        .await?;
    Ok(token_response(config, user, &session, refresh_token)?)
}

fn token_response(
    config: &JwtConfig,
    user: &User,
    session: &Session,
    refresh_token: String,
) -> Result<TokenResponse, jsonwebtoken::errors::Error> {
    let access_token = config.issue_access_token_for(
        user.username.clone(),
        user_audiences(config, user),
        user_scope(user),
        Some(session.id.clone()),
    )?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.access_token_ttl_secs,
    })
}

/// The scope claim: roles, then one `service:action` entry per granted
/// action. A service granted several actions gets one token per action
/// (`sage:read sage:write`), not a combined one - the wire format is a flat
/// list of space-separated tokens either way, and that keeps the parser on
/// the other end (`Claims::permissions`) simple.
///
/// A wildcard role emits the role alone. Expanding it into a grant per service
/// would make the token bigger, go stale when the estate gained a service, and
/// tell a reader less - `admin` is the more informative claim.
fn user_scope(user: &User) -> String {
    let mut entries: Vec<String> = user
        .get_roles()
        .iter()
        .map(|role| role.as_str().to_string())
        .collect();

    if !user.has_wildcard() {
        for (service, actions) in user.get_permissions() {
            for action in actions {
                entries.push(format!("{service}:{action}"));
            }
        }
    }

    entries.join(" ")
}

/// Services this user's token is valid for.
///
/// The point of narrowing: the audience check already in every relying party's
/// middleware then rejects a user with no grant, so service-level access
/// enforces itself without a single line changing outside gatehouse.
///
/// Gatehouse is always included. It serves the login page, the home page and
/// refresh, so a token that excluded it would leave the user unable to reach the
/// thing that would grant them anything.
fn user_audiences(config: &JwtConfig, user: &User) -> Vec<String> {
    if user.has_wildcard() {
        return config.audiences.clone();
    }

    let mut wanted: Vec<String> = user.get_permissions().into_keys().collect();
    wanted.push(config.service_name.clone());

    let mut audiences = config.narrow_audiences(&wanted);
    // `narrow_audiences` filters against SERVICE_AUDIENCES, which need not list
    // gatehouse itself.
    if !audiences.contains(&config.service_name) {
        audiences.push(config.service_name.clone());
    }
    audiences
}

/// Realm-wide session cookie. The `config` argument is no longer read - the
/// name is shared across services now - but is kept so call sites do not churn.
pub fn access_cookie(_config: &JwtConfig, token: String) -> Cookie<'static> {
    realm::session_cookie(token)
}

pub fn refresh_cookie(_config: &JwtConfig, token: String) -> Cookie<'static> {
    realm::refresh_cookie(token)
}

async fn access_claims(
    request: &HttpRequest,
    config: &JwtConfig,
    sessions: &SessionDb,
) -> Option<Claims> {
    let token = request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    let claims = config.decode_claims(token).ok()?;
    let session_id = claims.sid.as_deref()?;
    let active = sessions.is_active(session_id, &claims.sub).await.ok()?;
    (claims.allows(&config.service_name) && active).then_some(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_auth::prelude::{Permissions, Role};

    fn config() -> JwtConfig {
        envmnt::set("JWT_SECRET", "test_secret");
        let mut config = JwtConfig::init();
        config.service_name = "gatehouse".to_string();
        config.audiences = vec![
            "sage".to_string(),
            "switchboard".to_string(),
            "warehouse".to_string(),
        ];
        config
    }

    fn user(roles: Vec<Role>, grants: &[(&str, &[&str])]) -> User {
        let permissions: Permissions = grants
            .iter()
            .map(|(service, actions)| {
                (
                    (*service).to_string(),
                    actions.iter().map(|action| action.to_string()).collect(),
                )
            })
            .collect();
        User::new(
            "someone".to_string(),
            "password".to_string(),
            roles,
            permissions,
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_grant_becomes_a_scope_entry_per_action() {
        let scope = user_scope(&user(
            vec![Role::User],
            &[("sage", &["write"]), ("warehouse", &["read"])],
        ));

        assert_eq!(scope, "user sage:write warehouse:read");
    }

    /// Two actions on the same service become two tokens, not a combined one.
    #[test]
    fn several_actions_on_one_service_become_several_scope_entries() {
        let scope = user_scope(&user(
            vec![Role::User],
            &[("switchboard", &["read", "launch"])],
        ));

        assert_eq!(scope, "user switchboard:launch switchboard:read");
    }

    /// The token stays small and stays true as the estate grows.
    #[test]
    fn a_wildcard_role_emits_the_role_and_nothing_else() {
        let scope = user_scope(&user(vec![Role::Admin], &[("sage", &["read"])]));
        assert_eq!(scope, "admin");
    }

    #[test]
    fn audiences_narrow_to_the_services_a_user_was_granted() {
        let config = config();
        let audiences = user_audiences(&config, &user(vec![Role::User], &[("sage", &["read"])]));

        assert!(audiences.contains(&"sage".to_string()));
        assert!(!audiences.contains(&"switchboard".to_string()));
        assert!(!audiences.contains(&"warehouse".to_string()));
    }

    #[test]
    fn an_admin_keeps_the_whole_realm() {
        let config = config();
        let audiences = user_audiences(&config, &user(vec![Role::Admin], &[]));
        assert_eq!(audiences, config.audiences);
    }

    /// Gatehouse serves the login page, the home page and refresh. A token that
    /// excluded it would leave the holder unable to reach the one service that
    /// could grant them anything.
    #[test]
    fn gatehouse_is_always_an_audience() {
        let config = config();

        for holder in [
            user(vec![Role::User], &[]),
            user(vec![Role::User], &[("sage", &["read"])]),
        ] {
            let audiences = user_audiences(&config, &holder);
            assert!(
                audiences.contains(&"gatehouse".to_string()),
                "gatehouse missing from {audiences:?}"
            );
        }
    }

    #[test]
    fn a_user_with_no_grants_gets_gatehouse_alone() {
        let config = config();
        let audiences = user_audiences(&config, &user(vec![Role::User], &[]));
        assert_eq!(audiences, vec!["gatehouse".to_string()]);
    }

    /// A grant left behind after a service was removed from the deployment must
    /// not put that service back into an audience list.
    #[test]
    fn a_grant_for_a_service_this_deployment_does_not_run_is_ignored() {
        let config = config();
        let audiences = user_audiences(
            &config,
            &user(vec![Role::User], &[("conveyor", &["write"])]),
        );

        assert_eq!(audiences, vec!["gatehouse".to_string()]);
    }
}
