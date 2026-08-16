//! Workbench - the estate's task management service.

use actix_web::web;
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_starter::prelude::HttpServiceFactory;
use std::sync::Arc;

pub mod domain;
pub mod routers;

pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    sso_config: web::Data<SsoConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    let api_jwt_config = jwt_config.get_ref().clone();

    web::scope("")
        .app_data(jwt_config)
        .app_data(sso_config)
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .service(routers::api::scope(api_jwt_config))
        .service(routers::ui::scope())
}
