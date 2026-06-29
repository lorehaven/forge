pub mod pages;

use crate::actix::domain::jwt::JwtConfig;
use actix_web::web;

pub fn is_ui_authenticated(req: &actix_web::HttpRequest, config: &JwtConfig) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let cookie_name = format!("{}_ui_session", config.service_name);
    let Some(cookie) = req.cookie(&cookie_name) else {
        return false;
    };

    match config.decode_claims(cookie.value()) {
        Ok(claims) => claims.service == config.service_name,
        Err(_) => false,
    }
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

    let cookie_name = format!("{}_ui_session", config.service_name);
    let cookie = req.cookie(&cookie_name)?;

    let claims = match config.decode_claims(cookie.value()) {
        Ok(c) if c.service == config.service_name => c,
        _ => return None,
    };

    if let Some(session_id) = claims.sid.as_deref()
        && let Some(session_db) =
            req.app_data::<web::Data<crate::actix::domain::session::SessionDb>>()
        && !session_db
            .is_active(session_id, &claims.sub)
            .await
            .unwrap_or(false)
    {
        return None;
    }

    Some(claims)
}
