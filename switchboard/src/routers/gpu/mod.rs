use std::process::Command;
use std::time::Duration;

use actix_web::dev::HttpServiceFactory;
use actix_web::{Error, HttpRequest, HttpResponse, get, web};
use actix_ws::{Message, MessageStream, Session};

use futures_util::StreamExt;

use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

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
// Response type
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, ToSchema)]
pub struct GpuInfo {
    // GPU name
    pub name: String,

    // Total available VRAM
    pub total_gb: f64,

    // Used VRAM
    pub used_gb: f64,

    // Free VRAM
    pub free_gb: f64,
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

// ---------------------------------------------------------------------------
// GPU status
// ---------------------------------------------------------------------------

fn get_gpu_info() -> std::io::Result<GpuInfo> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--json"])
        .output()?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    let card = json.as_object().unwrap().values().next().unwrap();

    let total: f64 = card["VRAM Total Memory (B)"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let used: f64 = card["VRAM Total Used Memory (B)"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let total_gb = total / 1024.0f64.powi(3);
    let used_gb = used / 1024.0f64.powi(3);

    Ok(GpuInfo {
        name: "card0".into(),
        total_gb: round2(total_gb),
        used_gb: round2(used_gb),
        free_gb: round2(total_gb - used_gb),
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
