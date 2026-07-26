use actix_web::web;
use quench_starter::prelude::*;

pub mod clients;
pub mod config;
pub mod domain;
pub mod files;
pub mod observability;
pub mod routers;
pub mod runtime;
pub mod startup;
pub mod tools;

pub fn root_scope() -> impl HttpServiceFactory {
    routers::root_scope()
}

pub fn base_path_scope(state: startup::AppState) -> impl HttpServiceFactory {
    let jwt_config = state.jwt_config.get_ref().clone();
    state
        .install(web::scope(""))
        .service(routers::base_path_scope(jwt_config))
}
