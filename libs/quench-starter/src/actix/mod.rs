use crate::actix::domain::db::DbWrapper;
use crate::prelude::normalize_base_path;
use actix_service::ServiceFactory;
use actix_web::dev::{HttpServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, Scope, web};
use quench_cli::prelude::{Tone, print_status};
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

pub mod domain;
pub mod middleware;
pub mod routers;

pub type AppConfigFn = fn(&mut web::ServiceConfig);

pub trait ScopedModule: Send + Sync + 'static {
    type ScopeType: ServiceFactory<
            ServiceRequest,
            Config = (),
            Response = ServiceResponse,
            Error = Error,
            InitError = (),
        >;

    fn register(&self, scope: Scope) -> Scope<Self::ScopeType>;
}

pub async fn serve<R, S, RF, SF, I>(
    root_module: R,
    scoped_module: S,
    db: Option<Arc<DbWrapper>>,
    init: I,
) -> std::io::Result<()>
where
    R: Fn() -> RF + Send + Clone + 'static,
    S: Fn() -> SF + Send + Clone + 'static,
    RF: HttpServiceFactory + 'static,
    SF: HttpServiceFactory + 'static,
    I: Future<Output = ()> + Send + 'static,
{
    // Install default crypto provider for rustls 0.23+
    let _ = rustls::crypto::ring::default_provider().install_default();

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {}", base_path);

    let db_wrapper = match db {
        Some(d) => d,
        None => DbWrapper::init_env().await,
    };
    let health_state = routers::health::HealthState::live();
    let (https_addr, http_addr) = get_server_addr();

    let init_health_state = health_state.clone();
    tokio::spawn(async move {
        init.await;
        init_health_state.mark_ready();
        tracing::info!("Service initialization complete");
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db_wrapper.db.clone()))
            .app_data(web::Data::new(health_state.clone()))
            .wrap(middleware::logger::FilteredLogger::default())
            .service(
                web::scope(&base_path)
                    .service(routers::health::scope())
                    .service(routers::swagger::swagger_redirect)
                    .service(routers::swagger::swagger_index_redirect)
                    .service(scoped_module()),
            )
            .service(root_module())
    });

    let service_name = envmnt::get_or("SERVICE_NAME", "service");
    if let Some(config) = load_tls(
        envmnt::get_or("SERVER_CERT_PATH", "cert.pem"),
        envmnt::get_or("SERVER_KEY_PATH", "key.pem"),
    ) {
        print_status(
            Tone::Success,
            &format!("{}-service", service_name),
            &format!("starting HTTPS server on {https_addr}"),
        );
        print_status(
            Tone::Info,
            &format!("{}-service", service_name),
            &format!("starting HTTP redirect server on {http_addr}"),
        );

        let https_server = server.bind_rustls_0_23(https_addr, config)?.run();

        let redirect_server = HttpServer::new(move || {
            App::new().default_service(web::to(move |req: HttpRequest| {
                redirect_to_https(req, https_addr.port())
            }))
        })
        .bind(http_addr)?
        .run();

        tokio::try_join!(https_server, redirect_server)?;
        Ok(())
    } else {
        print_status(
            Tone::Warn,
            &format!("{}-service", service_name),
            "starting plain HTTP server",
        );
        server.bind(https_addr)?.run().await
    }
}

fn get_server_addr() -> (SocketAddr, SocketAddr) {
    let addr_str: String = envmnt::get_or("SERVER_ADDR", "0.0.0.0:443");
    let addr_redir_str: String = envmnt::get_or("SERVER_HTTP_REDIRECT_ADDR", "0.0.0.0:80");

    let https_addr: SocketAddr = addr_str.parse().unwrap();
    let http_addr: SocketAddr = addr_redir_str.parse().unwrap();
    (https_addr, http_addr)
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

pub fn load_tls(
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
