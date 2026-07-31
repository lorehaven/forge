use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::middleware::require_write::RequireWrite;
use quench_auth::prelude::JwtConfig;

pub mod chat;
pub mod files;
pub mod ui;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("").service(ui::assets)
}

/// Both scopes here have a clean method shape - upload/reprocess/delete and
/// chat completions are the only writes, and every one of them is already a
/// POST or DELETE - so `RequireWrite` needs no route-level exceptions.
/// `RequireWrite` has to sit *inside* `Auth`: `Auth`'s `.wrap()` is the last
/// one registered, so it runs first and populates the claims `RequireWrite`
/// reads. See `require_write`'s module docs.
pub fn base_path_scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("")
        .service(ui::scope(jwt_config.clone()))
        .service(
            files::scope()
                .wrap(RequireWrite::new(jwt_config.clone()))
                .wrap(Auth::new(jwt_config.clone())),
        )
        .service(
            chat::scope()
                .wrap(RequireWrite::new(jwt_config.clone()))
                .wrap(Auth::new(jwt_config)),
        )
}
