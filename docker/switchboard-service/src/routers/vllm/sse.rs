use super::list::render_instances_grid;
use crate::routers::vllm::engine::VllmEngine;
use bytes::Bytes;
use futures_util::StreamExt;
use quench_http::prelude::{HttpError, Inject, Response, get};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Sender;
use tokio_stream::wrappers::BroadcastStream;

pub struct VllmBroadcaster(pub Sender<String>);

pub fn init_vllm_status_publisher(broadcaster: Sender<String>, engine: Arc<dyn VllmEngine>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Ok(instances) = engine.list_instances().await {
                // Assumes admin view for now; the dashboard is primarily for admins.
                let html = render_instances_grid(instances, true);
                let _ = broadcaster.send(html);
            }
        }
    });
}

#[get("/api/v1/vllm/sse")]
pub async fn handle_sse_canonical(
    Inject(broadcaster): Inject<VllmBroadcaster>,
) -> Result<Response, HttpError> {
    handle_sse_impl(broadcaster).await
}

#[get("/api/v1/vllm/instances/sse")]
pub async fn handle_sse_alias(
    Inject(broadcaster): Inject<VllmBroadcaster>,
) -> Result<Response, HttpError> {
    handle_sse_impl(broadcaster).await
}

async fn handle_sse_impl(broadcaster: Arc<VllmBroadcaster>) -> Result<Response, HttpError> {
    let receiver = broadcaster.0.subscribe();
    let stream = BroadcastStream::new(receiver).map(|msg| match msg {
        Ok(html) => Ok::<_, std::io::Error>(Bytes::from(format!(
            "event: vllm-instances\ndata: {}\n\n",
            html.replace("\n", "")
        ))),
        Err(_) => Ok::<_, std::io::Error>(Bytes::from("event: error\ndata: stream closed\n\n")),
    });

    Ok(Response::streaming(http::StatusCode::OK, stream)
        .header("content-type", "text/event-stream"))
}
