//! Authorization-code + PKCE flow, its token endpoint, and `client_credentials`
//! - the OAuth client-facing surface; `api::auth`'s CLI login is untouched.

use crate::api::auth::{issue_client_credentials_token, issue_token_pair_for_client};
use crate::clients::{ClientRow, hash_secret};
use crate::codes::AuthorizationCodeRow;
use crate::ui::common::ui_path;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use http::StatusCode;
use quench_auth::domain::auth::UserDb;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::realm;
use quench_auth::domain::session::SessionDb;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{FromRequest, HttpError, Inject, Query, Request, Response, get, post};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
pub struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    #[serde(default)]
    scope: Option<String>,
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: Option<String>,
}

/// Raw query string (to rebuild the login redirect) plus any subject
/// already proved by the session cookie.
pub struct AuthorizeContext {
    query_string: String,
    claims: Option<Claims>,
}

#[async_trait]
impl FromRequest for AuthorizeContext {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let query_string = req.uri().query().unwrap_or("").to_string();
        let config = req
            .container()
            .get::<JwtConfig>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let sessions = req
            .container()
            .get::<SessionDb>()
            .map_err(|e| HttpError::status(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let claims = subject_from_cookie(req, &config, &sessions).await;
        Ok(Self {
            query_string,
            claims,
        })
    }
}

/// No session sends the browser to login with a `redirect` back here; a
/// session mints a code straight away - the SSO moment.
#[get("/api/v1/authorize")]
pub async fn authorize(
    ctx: AuthorizeContext,
    Query(query): Query<AuthorizeQuery>,
    Inject(db): Inject<Db>,
    Inject(users): Inject<UserDb>,
) -> Response {
    let clients = db.repository::<ClientRow>();
    let Ok(Some(client)) = clients.read(&query.client_id).await else {
        return bad_request("unknown client");
    };
    if !client.redirect_uri_matches(&query.redirect_uri) {
        return bad_request("redirect_uri does not match the client's registration");
    }
    if query.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return bad_request("only S256 PKCE is supported");
    }

    let Some(claims) = ctx.claims else {
        // `with_base_path`, not a literal path, so post-login lands on the mounted route.
        let original = format!(
            "{}?{}",
            quench_starter::common::routes::with_base_path("/api/v1/authorize"),
            ctx.query_string
        );
        let login_url = format!(
            "{}?redirect={}",
            ui_path("/login"),
            urlencoding::encode(&original)
        );
        return Response::new(StatusCode::FOUND).header("Location", login_url);
    };
    let Some(user) = users.get_user(&claims.sub).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };

    let code = random_code();
    let now = Utc::now();
    let row = AuthorizationCodeRow {
        code_hash: hash_secret(&code),
        client_id: client.client_id.clone(),
        username: user.username.clone(),
        redirect_uri: query.redirect_uri.clone(),
        scope: query.scope.clone().unwrap_or_default(),
        pkce_challenge: query.code_challenge.clone(),
        created_at: now,
        expires_at: now + chrono::Duration::seconds(60),
        consumed_at: None,
    };
    if db
        .repository::<AuthorizationCodeRow>()
        .create(&row)
        .await
        .is_err()
    {
        return internal_error();
    }

    let redirect = format!(
        "{}?code={}&state={}",
        query.redirect_uri,
        urlencoding::encode(&code),
        urlencoding::encode(&query.state)
    );
    Response::new(StatusCode::FOUND).header("Location", redirect)
}

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[post("/api/v1/token")]
pub async fn token(
    quench_http::prelude::Form(body): quench_http::prelude::Form<TokenRequest>,
    Inject(config): Inject<JwtConfig>,
    Inject(db): Inject<Db>,
    Inject(users): Inject<UserDb>,
    Inject(sessions): Inject<SessionDb>,
) -> Response {
    match body.grant_type.as_str() {
        "authorization_code" => {
            authorization_code_grant(&body, &config, &db, &users, &sessions).await
        }
        "refresh_token" => refresh_token_grant(&body, &config, &users, &sessions).await,
        "client_credentials" => client_credentials_grant(&body, &config, &db).await,
        other => bad_request(&format!("unsupported grant_type '{other}'")),
    }
}

pub async fn authorization_code_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    db: &Db,
    users: &UserDb,
    sessions: &SessionDb,
) -> Response {
    let (Some(code), Some(redirect_uri), Some(client_id), Some(client_secret)) = (
        &body.code,
        &body.redirect_uri,
        &body.client_id,
        &body.client_secret,
    ) else {
        return bad_request(
            "authorization_code requires code, redirect_uri, client_id and client_secret",
        );
    };

    let clients = db.repository::<ClientRow>();
    let Ok(Some(client)) = clients.read(client_id).await else {
        return bad_request("unknown client");
    };
    if !client.secret_matches(client_secret) {
        return bad_request("invalid client credentials");
    }

    let codes = db.repository::<AuthorizationCodeRow>();
    let Ok(Some(mut row)) = codes.read(&hash_secret(code)).await else {
        return bad_request("invalid code");
    };
    let now = Utc::now();
    if !row.is_usable(now) || row.client_id != *client_id || row.redirect_uri != *redirect_uri {
        return bad_request("invalid, expired or already-used code");
    }

    let Some(verifier) = &body.code_verifier else {
        return bad_request("missing code_verifier");
    };
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    if computed != row.pkce_challenge {
        return bad_request("PKCE verification failed");
    }

    // Consumed before the token is issued, so a code is never redeemed twice.
    row.consumed_at = Some(now);
    if codes.update(&row).await.is_err() {
        return internal_error();
    }

    let Some(user) = users.get_user(&row.username).await else {
        return bad_request("the user this code was issued to no longer exists");
    };
    match issue_token_pair_for_client(config, sessions, &user, &client.allowed_scopes).await {
        Ok(tokens) => json_ok(&tokens),
        Err(err) => {
            tracing::error!("failed to issue tokens for {}: {err}", user.username);
            internal_error()
        }
    }
}

pub async fn refresh_token_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    users: &UserDb,
    sessions: &SessionDb,
) -> Response {
    let Some(refresh_token) = &body.refresh_token else {
        return bad_request("refresh_token grant requires refresh_token");
    };
    let rotated = match sessions
        .rotate(refresh_token, config.refresh_token_ttl_secs)
        .await
    {
        Ok(Some(rotated)) => rotated,
        Ok(None) => return Response::new(StatusCode::UNAUTHORIZED),
        Err(err) => {
            tracing::error!("failed to rotate refresh token: {err}");
            return internal_error();
        }
    };
    let (session, new_refresh_token) = rotated;
    let Some(user) = users.get_user(&session.username).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };
    match crate::api::auth::token_response(config, &user, &session, new_refresh_token).await {
        Ok(tokens) => json_ok(&tokens),
        Err(err) => {
            tracing::error!("failed to issue tokens for {}: {err}", user.username);
            internal_error()
        }
    }
}

pub async fn client_credentials_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    db: &Db,
) -> Response {
    let (Some(client_id), Some(client_secret)) = (&body.client_id, &body.client_secret) else {
        return bad_request("client_credentials requires client_id and client_secret");
    };
    let clients = db.repository::<ClientRow>();
    let Ok(Some(client)) = clients.read(client_id).await else {
        return bad_request("unknown client");
    };
    if !client.secret_matches(client_secret) {
        return bad_request("invalid client credentials");
    }

    match issue_client_credentials_token(config, &client.client_id, &client.allowed_scopes).await {
        Ok(tokens) => json_ok(&tokens),
        Err(err) => {
            tracing::error!("failed to issue a client_credentials token for {client_id}: {err}");
            internal_error()
        }
    }
}

pub async fn subject_from_cookie(
    request: &Request,
    config: &JwtConfig,
    sessions: &SessionDb,
) -> Option<Claims> {
    let cookie =
        quench_auth::http::domain::cookies::cookie_value(request, &realm::session_cookie_name())?;
    let claims = config.decode_claims(&cookie).await.ok()?;
    let session_id = claims.sid.as_deref()?;
    sessions
        .is_active(session_id, &claims.sub)
        .await
        .ok()?
        .then_some(claims)
}

pub fn random_code() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn bad_request(message: &str) -> Response {
    quench_starter::http::domain::api_error::json_error(StatusCode::BAD_REQUEST, message)
}

fn internal_error() -> Response {
    Response::new(StatusCode::INTERNAL_SERVER_ERROR)
}

fn json_ok<T: serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = authorize as fn(_, _, _, _) -> _;
    let _ = token as fn(_, _, _, _, _) -> _;
}
