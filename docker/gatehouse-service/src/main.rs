use gatehouse_service::{VerificationTokens, clients, email, keys};
use quench_auth::domain::auth::UserDb;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_http::prelude::*;
use quench_starter::common::db::DbWrapper;
use quench_starter::common::routes::normalize_base_path;
use quench_starter::http::serve_app;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    envmnt::set("SERVICE_NAME", envmnt::get_or("SERVICE_NAME", "gatehouse"));
    // Gatehouse owns the realm - the one service that creates users.
    envmnt::set("AUTH_BOOTSTRAP", envmnt::get_or("AUTH_BOOTSTRAP", "true"));

    let catalog = Arc::new(gatehouse_service::PermissionCatalog::load().expect("permission catalog failed to load - see PERMISSIONS_CONFIG (default config/permissions.toml)"));

    gatehouse_service::ui::common::ensure_assets();

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    let db_wrapper = DbWrapper::init_env().await;

    // Retiring a rotated-out key after one access-token TTL keeps outstanding tokens verifying.
    let access_token_ttl_secs = envmnt::get_or("ACCESS_TOKEN_TTL_SECS", "900")
        .parse()
        .unwrap_or(900);
    let signing_keys = keys::SigningKeys::init(db_wrapper.db.clone(), access_token_ttl_secs)
        .await
        .expect("failed to load or generate gatehouse's signing keys");

    // Catalog's service list is the realm's audience list; gatehouse itself
    // is added explicitly since it isn't a grantable catalog service.
    let mut jwt_config = JwtConfig::init_signing(signing_keys.clone());
    jwt_config.audiences = catalog.service_names().map(str::to_string).collect();
    if !jwt_config.audiences.contains(&jwt_config.service_name) {
        jwt_config.audiences.push(jwt_config.service_name.clone());
    }

    tracing::info!(
        "Gatehouse starting: realm schema {}, audiences {:?}",
        quench_auth::domain::realm::auth_schema(),
        jwt_config.audiences
    );

    gatehouse_service::bootstrap::seed_users(&db_wrapper.db).await;
    if let Err(err) = clients::seed_clients(&db_wrapper.db).await {
        tracing::error!(
            "failed to seed OAuth clients from CLIENTS_CONFIG: {err} - the authorization-code and client_credentials grants will reject every client"
        );
    }
    let user_db = UserDb::init(db_wrapper.db.clone()).await;

    // Sessions live in the cache store - expiry is TTL, revocation is a delete.
    let session_db = SessionDb::from_env()
        .await
        .expect("session store unavailable");

    // The only sender that exists today - see `email`'s module docs.
    let mailer: Arc<dyn email::Sender> = Arc::new(email::LoggingSender);
    let tokens = Arc::new(
        VerificationTokens::from_env()
            .await
            .expect("verification token store unavailable"),
    );

    let health_state = quench_starter::common::health::HealthState::live();
    health_state.mark_ready();

    // `serve_app` re-detects TLS itself; this is only for `ExternalScheme`,
    // which a request can't otherwise tell which listener it arrived on.
    let has_tls = load_tls(
        envmnt::get_or("SERVER_CERT_PATH", "cert.pem"),
        envmnt::get_or("SERVER_KEY_PATH", "key.pem"),
    )
    .is_some();
    let external_scheme =
        gatehouse_service::ui::common::ExternalScheme(if has_tls { "https" } else { "http" });

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .provide(jwt_config)
        .provide_arc(signing_keys)
        .provide_arc(user_db)
        .provide_arc(session_db)
        .provide_arc(catalog)
        .provide(mailer)
        .provide_arc(tokens)
        .provide(external_scheme)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    // No `Auth`/`RequireWrite` wrap - gatehouse mints the tokens Auth would
    // check, so each route guards itself (see `action_claims!`/`admin_actor!`).
    gatehouse_service::api::auth::register_routes();
    gatehouse_service::api::jwks::register_routes();
    gatehouse_service::api::oauth::register_routes();
    gatehouse_service::api::test_tokens::register_routes();
    gatehouse_service::api::users::register_routes();
    gatehouse_service::ui::register_routes();

    let app = quench_starter::http::discover_and_mount(base_path);

    serve_app("gatehouse-service", app, container).await
}
