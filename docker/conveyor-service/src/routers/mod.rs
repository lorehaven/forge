use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use quench_auth::prelude::JwtConfig;

pub mod api;
pub mod ui;

/// Mounted at the server root, outside the base path. Assets are here so a
/// stylesheet referenced with an absolute path resolves whether or not the
/// deployment sets `BASE_PATH`.
pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("").service(ui::assets)
}

pub fn base_path_scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("")
        .service(ui::scope(jwt_config.clone()))
        // `api::scope` applies auth itself: the webhook endpoint inside it is
        // authenticated by signature rather than by a realm token.
        .service(api::scope(jwt_config))
}
