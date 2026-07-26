pub mod pages;

use crate::actix::domain::jwt::JwtConfig;
use actix_web::web;

/// Whether the request carries a usable realm session.
///
/// Checks the session store as well as the token: identity is shared now, so a
/// logout at any service must take effect everywhere immediately rather than
/// when the access token happens to expire.
pub async fn is_ui_authenticated(req: &actix_web::HttpRequest, config: &JwtConfig) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(cookie) = req.cookie(&crate::actix::domain::realm::session_cookie_name()) else {
        return false;
    };

    let Ok(claims) = config.decode_claims(cookie.value()) else {
        return false;
    };
    if !claims.allows(&config.service_name) {
        return false;
    }

    session_is_active(req, &claims).await
}

/// Looks up the claim's session in whichever `SessionDb` the service registered.
/// Tokens without a `sid` (machine-to-machine) carry no session to check.
async fn session_is_active(
    req: &actix_web::HttpRequest,
    claims: &crate::actix::domain::jwt::Claims,
) -> bool {
    use crate::actix::domain::session::SessionDb;

    let Some(session_id) = claims.sid.as_deref() else {
        return true;
    };

    if let Some(sessions) = req.app_data::<web::Data<SessionDb>>() {
        return sessions
            .is_active(session_id, &claims.sub)
            .await
            .unwrap_or(false);
    }
    if let Some(sessions) = req.app_data::<web::Data<std::sync::Arc<SessionDb>>>() {
        return sessions
            .is_active(session_id, &claims.sub)
            .await
            .unwrap_or(false);
    }

    tracing::debug!("no SessionDb registered; trusting token validity alone");
    true
}

pub async fn get_user_from_req(
    req: &actix_web::HttpRequest,
    config: &JwtConfig,
) -> Option<crate::actix::domain::jwt::Claims> {
    use actix_web::HttpMessage;
    if let Some(claims) = req.extensions().get::<crate::actix::domain::jwt::Claims>() {
        return Some(claims.clone());
    }

    if !config.auth_enabled {
        return Some(crate::actix::domain::jwt::Claims::new(
            "admin".to_string(),
            config.service_name.clone(),
            "admin".to_string(),
            None,
            3600,
        ));
    }

    let cookie = req.cookie(&crate::actix::domain::realm::session_cookie_name())?;

    let claims = match config.decode_claims(cookie.value()) {
        Ok(c) if c.allows(&config.service_name) => c,
        _ => return None,
    };

    if !session_is_active(req, &claims).await {
        return None;
    }

    Some(claims)
}
