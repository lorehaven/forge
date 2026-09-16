use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_http::prelude::{Endpoint, HttpError, Inject, Json, OnPathPrefix, Response, get, wrap};
use quench_web::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tokio_stream::wrappers::BroadcastStream;

pub mod monitor;

pub use monitor::get_gpu_info;

pub struct GpuBroadcaster(pub Sender<String>);

/// Wraps `/api/v1/gpu` in `Auth`. `base_path` matters: `OnPathPrefix` sees
/// the raw un-mounted path, which still carries `BASE_PATH`.
pub fn wrap_auth(
    app: Arc<dyn Endpoint>,
    jwt_config: JwtConfig,
    base_path: &str,
) -> Arc<dyn Endpoint> {
    let prefix: &'static str = Box::leak(format!("{base_path}/api/v1/gpu").into_boxed_str());
    wrap(app, OnPathPrefix::new(prefix, Auth::new(jwt_config)))
}

pub fn register_routes() {
    let _ = get_status as fn() -> _;
    let _ = handle_sse as fn(_) -> _;
}

// REST endpoint

#[get("/api/v1/gpu/status")]
pub async fn get_status() -> Json<monitor::GpuInfo> {
    Json(get_gpu_info().unwrap_or_default())
}

// SSE endpoint

#[get("/api/v1/gpu/status/sse")]
pub async fn handle_sse(
    Inject(broadcaster): Inject<GpuBroadcaster>,
) -> Result<Response, HttpError> {
    let rx = broadcaster.0.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| async move {
        match msg {
            Ok(html) => Some(Ok::<Bytes, std::io::Error>(Bytes::from(format!(
                "event: gpu-status\ndata: {}\n\n",
                html.replace('\n', "")
            )))),
            Err(_) => None,
        }
    });

    Ok(Response::streaming(http::StatusCode::OK, stream)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache"))
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
