use actix_web::dev::HttpServiceFactory;
use actix_web::web;
use quench_srv::prelude::JwtConfig;

pub mod chat;
pub mod ui;

pub fn root_scope() -> impl HttpServiceFactory {
    web::scope("").service(ui::assets)
}

pub fn base_path_scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("")
        .service(ui::scope(jwt_config.clone()))
        .service(chat::scope().wrap(quench_srv::actix::middleware::auth::Auth::new(jwt_config)))
}
