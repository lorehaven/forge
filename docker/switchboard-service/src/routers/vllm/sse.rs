use super::list::render_instances_grid;
use crate::routers::vllm::engine::VllmEngine;
use actix_web::{Error, HttpResponse, get, web};
use bytes::Bytes;
use futures_util::StreamExt;
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
                // We assume admin view for the broadcast for now,
                // as the management dashboard is primarily for admins.
                let html = render_instances_grid(instances, true);
                let _ = broadcaster.send(html);
            }
        }
    });
}

#[get("/sse")]
pub async fn handle_sse_canonical(
    broadcaster: web::Data<VllmBroadcaster>,
) -> Result<HttpResponse, Error> {
    handle_sse_impl(broadcaster).await
}

#[get("/instances/sse")]
pub async fn handle_sse_alias(
    broadcaster: web::Data<VllmBroadcaster>,
) -> Result<HttpResponse, Error> {
    handle_sse_impl(broadcaster).await
}

async fn handle_sse_impl(broadcaster: web::Data<VllmBroadcaster>) -> Result<HttpResponse, Error> {
    let receiver = broadcaster.0.subscribe();
    let stream = BroadcastStream::new(receiver).map(|msg| match msg {
        Ok(html) => Ok::<_, Error>(Bytes::from(format!(
            "event: vllm-instances\ndata: {}\n\n",
            html.replace("\n", "")
        ))),
        Err(_) => Ok::<_, Error>(Bytes::from("event: error\ndata: stream closed\n\n")),
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(stream))
}
