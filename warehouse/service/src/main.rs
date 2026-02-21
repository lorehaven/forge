use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod middleware;
pub mod routers;
pub mod shared;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let addr: SocketAddr = envmnt::get_or("SERVER_ADDR", "0.0.0.0:8698")
        .parse()
        .unwrap();

    let jwt_config = shared::jwt::JwtConfig::init();
    let max_body_bytes: usize = envmnt::get_or("MAX_REQUEST_BODY_BYTES", "1073741824")
        .parse()
        .unwrap_or(1024 * 1024 * 1024);
    let max_concurrent_uploads: usize = envmnt::get_or("MAX_CONCURRENT_UPLOADS", "32")
        .parse()
        .unwrap_or(32);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::PayloadConfig::new(max_body_bytes))
            .app_data(web::Data::new(jwt_config.clone()))
            // Middleware
            .wrap(middleware::limits::WarehouseLimits::new(
                max_concurrent_uploads,
            ))
            .wrap(middleware::auth::WarehouseAuth::new(jwt_config.clone()))
            .wrap(Logger::default())
            // Register Actix services
            .service(routers::admin::scope())
            .service(routers::docker::scope())
            .service(routers::docker::token::handle)
            .service(routers::health::scope())
            .service(routers::crates::scope())
            .service(routers::ui::assets)
            .service(routers::ui::scope())
            // Swagger UI
            .service(routers::swagger_redirect)
            .service(routers::swagger_index_redirect)
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}")
                    .url("/api-doc/openapi.json", routers::OpenApiDoc::openapi()),
            )
    });

    if let Some(config) = load_tls(
        envmnt::get_or("SERVER_CERT_PATH", "cert.pem"),
        envmnt::get_or("SERVER_KEY_PATH", "key.pem"),
    ) {
        println!("🔒 Starting HTTPS server");
        server.bind_rustls_0_23(addr, config)?.run().await
    } else {
        println!("⚠ Starting plain HTTP server");
        server.bind(addr)?.run().await
    }
}

fn load_tls(
    cert_path: impl AsRef<std::path::Path>,
    key_path: impl AsRef<std::path::Path>,
) -> Option<ServerConfig> {
    let mut cert_reader = BufReader::new(File::open(cert_path).ok()?);
    let mut key_reader = BufReader::new(File::open(key_path).ok()?);

    let cert_chain: Vec<CertificateDer<'static>> =
        certs(&mut cert_reader).collect::<Result<_, _>>().ok()?;

    let mut keys = pkcs8_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    let key = keys.pop()?;

    // Convert PrivatePkcs8KeyDer -> PrivateKeyDer
    let key: PrivateKeyDer<'static> = key.into();

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .ok()?;

    Some(config)
}
