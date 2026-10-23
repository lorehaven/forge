use super::list::render_instances_grid;
use crate::routers::vllm::engine::VllmEngine;
use quench_core::bytes::Bytes;
use quench_http::prelude::{HttpError, Inject, Response, get};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::Sender;
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug)]
pub struct VllmBroadcaster(pub Sender<String>);

pub fn init_vllm_status_publisher(broadcaster: Sender<String>, engine: Arc<dyn VllmEngine>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Ok(instances) = engine.list_instances().await {
                // Assumes admin view for now; the dashboard is primarily for admins.
                let html = render_instances_grid(&instances, true);
                let _ = broadcaster.send(html);
            }
        }
    });
}

#[get("/api/v1/vllm/sse")]
pub async fn handle_sse_canonical(
    Inject(broadcaster): Inject<VllmBroadcaster>,
) -> Result<Response, HttpError> {
    Ok(handle_sse_impl(&broadcaster))
}

#[get("/api/v1/vllm/instances/sse")]
pub async fn handle_sse_alias(
    Inject(broadcaster): Inject<VllmBroadcaster>,
) -> Result<Response, HttpError> {
    Ok(handle_sse_impl(&broadcaster))
}

fn handle_sse_impl(broadcaster: &VllmBroadcaster) -> Response {
    let receiver = broadcaster.0.subscribe();
    let stream = quench_core::stream::unfold(receiver, |mut receiver| async move {
        let frame = match receiver.recv().await {
            Ok(html) => Bytes::from(format!(
                "event: vllm-instances\ndata: {}\n\n",
                html.replace('\n', "")
            )),
            Err(RecvError::Lagged(_)) => Bytes::from("event: error\ndata: stream closed\n\n"),
            Err(RecvError::Closed) => return None,
        };
        Some((Ok::<_, std::io::Error>(frame), receiver))
    });

    Response::streaming(quench_http::http::StatusCode::OK, stream)
        .header("content-type", "text/event-stream")
}
