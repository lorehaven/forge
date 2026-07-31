use std::time::Duration;

use actix_web::dev::HttpServiceFactory;
use actix_web::{Error, HttpResponse, Responder, get, web};
use bytes::Bytes;
use futures_util::StreamExt;
use quench_auth::actix::middleware::auth::Auth;
use quench_auth::actix::middleware::require_write::RequireWrite;
use quench_auth::prelude::JwtConfig;
use quench_web::prelude::*;
use tokio::sync::broadcast::Sender;
use tokio_stream::wrappers::BroadcastStream;

pub mod monitor;

pub use monitor::get_gpu_info;

pub struct GpuBroadcaster(pub Sender<String>);

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// Both routes are GET, so `RequireWrite` never actually refuses anything
/// here - it is mounted for the same reason every scope in the estate mounts
/// it: so a route added here later that does write is covered without anyone
/// having to remember to add the check.
pub fn scope(jwt_config: JwtConfig) -> impl HttpServiceFactory {
    web::scope("/api/v1/gpu")
        .wrap(RequireWrite::new(jwt_config.clone()))
        .wrap(Auth::new(jwt_config))
        .service(handle_sse)
        .service(get_status)
}

// ---------------------------------------------------------------------------
// REST endpoint
// ---------------------------------------------------------------------------

#[get("/status")]
pub async fn get_status() -> impl Responder {
    let gpu = get_gpu_info().unwrap_or_default();
    HttpResponse::Ok().json(gpu)
}

// ---------------------------------------------------------------------------
// SSE endpoint
// ---------------------------------------------------------------------------

#[get("/status/sse")]
pub async fn handle_sse(broadcaster: web::Data<GpuBroadcaster>) -> Result<HttpResponse, Error> {
    let rx = broadcaster.0.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| async move {
        match msg {
            Ok(html) => Some(Ok::<Bytes, Error>(Bytes::from(format!(
                "event: gpu-status\ndata: {}\n\n",
                html.replace('\n', "")
            )))),
            Err(_) => None,
        }
    });

    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/event-stream"))
        .insert_header(("Cache-Control", "no-cache"))
        .streaming(stream))
}

pub fn init_gpu_status_publisher(broadcaster: Sender<String>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            let gpu = get_gpu_info().unwrap_or_default();

            let html = div()
                .class("gpu")
                .attr("id", "gpu-status")
                .child(div().class("gpu-name").text(format!("GPU: {}", gpu.name)))
                .child(
                    div()
                        .class("gpu-total")
                        .child(span().attr("data-i18n", "ui_models_gpu_total"))
                        .child(span().text(format!(" {} GB", gpu.total_gb))),
                )
                .child(
                    div()
                        .class("gpu-free")
                        .child(span().attr("data-i18n", "ui_models_gpu_free"))
                        .child(span().text(format!(" {} GB", gpu.free_gb))),
                )
                .render();

            let _ = broadcaster.send(html);
        }
    });
}
