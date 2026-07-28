//! Warehouse - the estate's storage service.
//!
//! Three things live behind one address: a cargo registry, a docker registry,
//! and plain file storage. Each is a feature that can be turned off, and each
//! addresses its content its own way - a crate by name and version, an image by
//! digest, a file by path within a named storage.
//!
//! The scopes are split by where they have to be mounted rather than by what
//! they do: the docker registry owns `/v2` at the server root because the
//! registry protocol says so, and everything else sits under `BASE_PATH`.

use actix_web::web;
use quench_auth::prelude::{JwtConfig, SessionDb, UserDb};
use quench_config::ConfigLoader;
use quench_starter::prelude::HttpServiceFactory;
use std::sync::Arc;

pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;

/// Mounted at the server root, outside the base path.
///
/// The docker registry API is fixed at `/v2` by the specification, and the
/// token endpoint with it, so neither can move under `BASE_PATH`.
pub fn root_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    let loader = ConfigLoader::new("WAREHOUSE");
    let max_body_bytes = loader.env_u64("MAX_REQUEST_BODY_BYTES", 1024 * 1024 * 1024) as usize;
    let max_concurrent_uploads = loader.env_u64("MAX_CONCURRENT_UPLOADS", 32) as usize;

    web::scope("")
        .app_data(actix_web::web::PayloadConfig::new(max_body_bytes))
        .app_data(jwt_config.clone())
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .wrap(middleware::auth::WarehouseAuth::new(
            jwt_config.get_ref().clone(),
        ))
        .wrap(middleware::limits::WarehouseLimits::new(
            max_concurrent_uploads,
        ))
        .service(routers::docker::scope())
        .service(routers::docker::token::handle)
}

pub fn base_path_scope(
    jwt_config: web::Data<JwtConfig>,
    user_db: Arc<UserDb>,
    session_db: Arc<SessionDb>,
) -> impl HttpServiceFactory {
    let loader = ConfigLoader::new("WAREHOUSE");
    let max_body_bytes = loader.env_u64("MAX_REQUEST_BODY_BYTES", 1024 * 1024 * 1024) as usize;

    web::scope("")
        // The files API streams its body to disk rather than buffering it, but
        // the extractors around it still answer to this, and the default of
        // 256KB would turn every artifact into a 400.
        .app_data(actix_web::web::PayloadConfig::new(max_body_bytes))
        .app_data(jwt_config.clone())
        .app_data(web::Data::new(user_db))
        .app_data(web::Data::new(session_db))
        .service(routers::admin::scope())
        .service(routers::crates::scope())
        .service(routers::crates::scope_index())
        // Unlike crates and docker, the files API applies the realm's auth to
        // itself: there is no registry protocol here to negotiate a token, and
        // a caller presents a realm identity directly.
        .service(routers::files::scope(jwt_config.get_ref().clone()))
        .service(routers::ui::scope())
}
