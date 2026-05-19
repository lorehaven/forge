use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use quench_cli::terminal::{Tone, print_status};
use routers::normalize_base_path;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::net::SocketAddr;
use utoipa_swagger_ui::SwaggerUi;

pub mod domain;
pub mod middleware;
pub mod routers;
pub mod utils;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt().init();
    dotenvy::dotenv().ok();

    let addr_str: String = envmnt::get_or("SERVER_ADDR", "0.0.0.0:443");
    let addr_redir_str: String = envmnt::get_or("SERVER_HTTP_REDIRECT_ADDR", "0.0.0.0:80");

    let addr: SocketAddr = addr_str.parse().unwrap();

    let jwt_config = domain::jwt::JwtConfig::init();
    let max_body_bytes: usize = envmnt::get_or("MAX_REQUEST_BODY_BYTES", "1073741824")
        .parse()
        .unwrap_or(1024 * 1024 * 1024);
    let max_concurrent_uploads: usize = envmnt::get_or("MAX_CONCURRENT_UPLOADS", "32")
        .parse()
        .unwrap_or(32);

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::PayloadConfig::new(max_body_bytes))
            .app_data(web::Data::new(jwt_config.clone()))
            .wrap(middleware::limits::WarehouseLimits::new(
                max_concurrent_uploads,
            ))
            .wrap(middleware::auth::WarehouseAuth::new(jwt_config.clone()))
            .wrap(middleware::logger::FilteredLogger)
            .service(
                web::scope(&base_path)
                    // Register Actix services
                    .service(routers::admin::scope())
                    .service(routers::crates::scope())
                    .service(routers::crates::scope_index())
                    .service(routers::files::scope())
                    .service(routers::health::scope())
                    .service(routers::ui::scope())
                    // Swagger UI
                    .service(routers::swagger_redirect)
                    .service(routers::swagger_index_redirect)
                    .service(
                        SwaggerUi::new("/swagger-ui/{_:.*}")
                            .url("/api-doc/openapi.json", routers::openapi()),
                    ),
            )
            .service(routers::docker::scope())
            .service(routers::docker::token::handle)
    });

    if let Some(config) = load_tls(
        envmnt::get_or("SERVER_CERT_PATH", "cert.pem"),
        envmnt::get_or("SERVER_KEY_PATH", "key.pem"),
    ) {
        let http_redirect_addr: SocketAddr = addr_redir_str.parse().unwrap();
        let https_port = addr.port();

        print_status(
            Tone::Success,
            "warehouse-service",
            &format!("starting HTTPS server on {addr}"),
        );
        print_status(
            Tone::Info,
            "warehouse-service",
            &format!("starting HTTP redirect server on {http_redirect_addr}"),
        );

        let https_server = server.bind_rustls_0_23(addr, config)?.run();

        let redirect_server = HttpServer::new(move || {
            App::new().default_service(web::to(move |req: HttpRequest| {
                redirect_to_https(req, https_port)
            }))
        })
        .bind(http_redirect_addr)?
        .run();

        tokio::try_join!(https_server, redirect_server)?;
        Ok(())
    } else {
        print_status(
            Tone::Warn,
            "warehouse-service",
            "starting plain HTTP server",
        );
        server.bind(addr)?.run().await
    }
}

async fn redirect_to_https(req: HttpRequest, https_port: u16) -> HttpResponse {
    let host = req.connection_info().host().to_string();
    let authority = build_https_authority(&host, https_port);
    let location = format!("https://{authority}{}", req.uri());

    HttpResponse::PermanentRedirect()
        .insert_header(("Location", location))
        .finish()
}

fn build_https_authority(host: &str, https_port: u16) -> String {
    if let Ok(authority) = host.parse::<actix_web::http::uri::Authority>() {
        let parsed_host = authority.host();
        let rendered_host = if parsed_host.contains(':') {
            format!("[{parsed_host}]")
        } else {
            parsed_host.to_string()
        };

        if https_port == 443 {
            rendered_host
        } else {
            format!("{rendered_host}:{https_port}")
        }
    } else if https_port == 443 {
        host.to_string()
    } else {
        format!("{host}:{https_port}")
    }
}

fn load_tls(
    cert_path: impl AsRef<std::path::Path>,
    key_path: impl AsRef<std::path::Path>,
) -> Option<ServerConfig> {
    let cert_chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;

    let key: PrivateKeyDer<'static> = PrivateKeyDer::from_pem_file(key_path).ok()?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .ok()?;

    Some(config)
}
