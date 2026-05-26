use std::time::Duration;

use actix_web::dev::HttpServiceFactory;
use actix_web::{Error, HttpRequest, HttpResponse, get, web};
use actix_ws::{Message, MessageStream, Session};

use futures_util::StreamExt;

use utoipa::OpenApi;

pub mod monitor;

pub use monitor::{GpuInfo, get_gpu_info};

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(handle_ws),
    tags((name = "gpu", description = "GPU status endpoints"))
)]
pub struct GpuApiDoc;

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/api/v1/gpu").service(handle_ws)
}

// ---------------------------------------------------------------------------
// WebSocket endpoint
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/status/ws",
    operation_id = "gpu_status_ws",
    tags = ["gpu"],
    responses(
        (
            status = 101,
            description = "WebSocket upgraded"
        ),
    )
)]
#[get("/status/ws")]
pub async fn handle_ws(req: HttpRequest, body: web::Payload) -> Result<HttpResponse, Error> {
    let (response, session, stream) = actix_ws::handle(&req, body)?;

    let session_clone = session.clone();

    actix_web::rt::spawn(async move {
        websocket_sender(session).await;
    });

    actix_web::rt::spawn(async move {
        websocket_receiver(stream, session_clone).await;
    });

    Ok(response)
}

// ---------------------------------------------------------------------------
// WebSocket tasks
// ---------------------------------------------------------------------------

async fn websocket_sender(mut session: Session) {
    let mut interval = actix_web::rt::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        match get_gpu_info() {
            Ok(gpu) => match serde_json::to_string(&gpu) {
                Ok(json) => {
                    if session.text(json).await.is_err() {
                        break;
                    }
                }

                Err(e) => {
                    tracing::error!("serialize gpu status failed: {e}");
                }
            },

            Err(e) => {
                tracing::error!("fetch GPU status failed: {e}");
            }
        }
    }
}

async fn websocket_receiver(mut stream: MessageStream, mut session: Session) {
    while let Some(item) = stream.next().await {
        match item {
            Ok(Message::Ping(bytes)) => {
                if session.pong(&bytes).await.is_err() {
                    break;
                }
            }

            Ok(Message::Close(reason)) => {
                let _ = session.close(reason).await;
                break;
            }

            Ok(Message::Text(_))
            | Ok(Message::Binary(_))
            | Ok(Message::Continuation(_))
            | Ok(Message::Nop)
            | Ok(Message::Pong(_)) => {}

            Err(e) => {
                tracing::error!("websocket protocol error: {e}");

                break;
            }
        }
    }
}
