//! The authorization-code + PKCE redirect flow, and the token endpoint that
//! finishes it - plus the `client_credentials` grant machine-to-machine
//! callers use instead of the Basic auth this replaced.
//!
//! `POST /api/v1/auth/login` (`crate::api::auth`) is untouched and stays the
//! resource-owner path CLIs use directly; this module is only the OAuth
//! client-facing surface.

use crate::api::auth::{issue_client_credentials_token, issue_token_pair_for_client};
use crate::clients::{ClientRow, hash_secret};
use crate::codes::AuthorizationCodeRow;
use crate::ui::common::ui_path;
use actix_web::{HttpRequest, HttpResponse, Responder, get, post, web};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use quench_auth::prelude::{Claims, JwtConfig, SessionDb, UserDb, realm};
use quench_db::prelude::{Crud, Db};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    #[serde(default)]
    scope: Option<String>,
    code_challenge: String,
    #[serde(default)]
    code_challenge_method: Option<String>,
}

/// No gatehouse session → hands the browser to the login form with a
/// `redirect` back to this exact request, reusing the same guarded
/// `?redirect=` mechanism every other login already goes through
/// (`quench_auth::actix::routers::ui::pages::auth::validated_redirect`).
/// A session → mints a code and sends the browser straight back to the
/// client, invisibly when a gatehouse session already existed - the SSO
/// moment.
#[get("/api/v1/authorize")]
pub async fn authorize(
    request: HttpRequest,
    query: web::Query<AuthorizeQuery>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    users: web::Data<Arc<UserDb>>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
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

    let Some(claims) = subject_from_cookie(&request, &config, &sessions).await else {
        // `with_base_path`, not a literal `/api/v1/authorize`: the redirect
        // the login form carries has to point back at the actual mounted
        // route (e.g. `/gatehouse/api/v1/authorize`), or the browser lands on
        // a 404 the moment it tries to follow it after logging in.
        let original = format!(
            "{}?{}",
            quench_starter::prelude::with_base_path("/api/v1/authorize"),
            request.query_string()
        );
        let login_url = format!(
            "{}?redirect={}",
            ui_path("/login"),
            urlencoding::encode(&original)
        );
        return HttpResponse::Found()
            .append_header(("Location", login_url))
            .finish();
    };
    let Some(user) = users.get_user(&claims.sub).await else {
        return HttpResponse::Unauthorized().finish();
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
        urlencoding::encode(&query.state),
    );
    HttpResponse::Found()
        .append_header(("Location", redirect))
        .finish()
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[post("/api/v1/token")]
pub async fn token(
    body: web::Form<TokenRequest>,
    config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    users: web::Data<Arc<UserDb>>,
    sessions: web::Data<Arc<SessionDb>>,
) -> impl Responder {
    match body.grant_type.as_str() {
        "authorization_code" => {
            authorization_code_grant(&body, &config, &db, &users, &sessions).await
        }
        "refresh_token" => refresh_token_grant(&body, &config, &users, &sessions).await,
        "client_credentials" => client_credentials_grant(&body, &config, &db).await,
        other => bad_request(&format!("unsupported grant_type '{other}'")),
    }
}

async fn authorization_code_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    db: &Db,
    users: &UserDb,
    sessions: &SessionDb,
) -> HttpResponse {
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

    // Consumed before the token is issued: a failure past this point costs the
    // caller a fresh `/authorize` round trip rather than letting the code be
    // redeemed twice.
    row.consumed_at = Some(now);
    if codes.update(&row).await.is_err() {
        return internal_error();
    }

    let Some(user) = users.get_user(&row.username).await else {
        return bad_request("the user this code was issued to no longer exists");
    };
    match issue_token_pair_for_client(config, sessions, &user, &client.allowed_scopes).await {
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("failed to issue tokens for {}: {err}", user.username);
            internal_error()
        }
    }
}

async fn refresh_token_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    users: &UserDb,
    sessions: &SessionDb,
) -> HttpResponse {
    let Some(refresh_token) = &body.refresh_token else {
        return bad_request("refresh_token grant requires refresh_token");
    };
    let rotated = match sessions
        .rotate(refresh_token, config.refresh_token_ttl_secs)
        .await
    {
        Ok(Some(rotated)) => rotated,
        Ok(None) => return HttpResponse::Unauthorized().finish(),
        Err(err) => {
            tracing::error!("failed to rotate refresh token: {err}");
            return internal_error();
        }
    };
    let (session, new_refresh_token) = rotated;
    let Some(user) = users.get_user(&session.username).await else {
        return HttpResponse::Unauthorized().finish();
    };
    match crate::api::auth::token_response(config, &user, &session, new_refresh_token).await {
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("failed to issue tokens for {}: {err}", user.username);
            internal_error()
        }
    }
}

async fn client_credentials_grant(
    body: &TokenRequest,
    config: &JwtConfig,
    db: &Db,
) -> HttpResponse {
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
        Ok(tokens) => HttpResponse::Ok().json(tokens),
        Err(err) => {
            tracing::error!("failed to issue a client_credentials token for {client_id}: {err}");
            internal_error()
        }
    }
}

async fn subject_from_cookie(
    request: &HttpRequest,
    config: &JwtConfig,
    sessions: &SessionDb,
) -> Option<Claims> {
    let cookie = request.cookie(&realm::session_cookie_name())?;
    let claims = config.decode_claims(cookie.value()).await.ok()?;
    let session_id = claims.sid.as_deref()?;
    sessions
        .is_active(session_id, &claims.sub)
        .await
        .ok()?
        .then_some(claims)
}

fn random_code() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

fn bad_request(message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({ "error": message }))
}

fn internal_error() -> HttpResponse {
    HttpResponse::InternalServerError().finish()
}
