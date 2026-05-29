pub mod engine;
pub mod kubernetes;
pub mod native;

use crate::routers::vllm::engine::{VllmEngine, VllmManagementMode};
use crate::routers::vllm::kubernetes::KubernetesVllmEngine;
use crate::routers::vllm::native::NativeVllmEngine;
use actix_web::dev::HttpServiceFactory;
use actix_web::{HttpResponse, Responder, delete, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

// ---------------------------------------------------------------------------
// OpenAPI
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    paths(list_instances, launch_instance, stop_instance, handle_ws),
    components(schemas(VllmInstance, LaunchRequest))
)]
pub struct VllmApiDoc;

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct VllmInstance {
    pub id: String,
    pub namespace: String,
    pub model: String,
    pub host: String,
    pub port: u16,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,

    #[schema(value_type = String, format = DateTime)]
    pub started_at: DateTime<Utc>,

    pub status: String,
    pub log_path: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LaunchRequest {
    pub model: String,
    pub host: String,
    pub port: u16,
    pub namespace: Option<String>,
    pub quantization: Option<String>,
    pub max_model_len: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub enable_prefix_caching: bool,
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

pub async fn init_engine() -> Arc<dyn VllmEngine> {
    let mode = VllmManagementMode::from_env();
    tracing::info!("Initializing vLLM management in {:?} mode", mode);

    match mode {
        VllmManagementMode::Native => Arc::new(NativeVllmEngine),
        VllmManagementMode::Kubernetes => match KubernetesVllmEngine::new().await {
            Ok(e) => Arc::new(e),
            Err(err) => {
                tracing::error!(
                    "Failed to initialize Kubernetes vLLM engine: {}. Falling back to Native.",
                    err
                );
                Arc::new(NativeVllmEngine)
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

pub fn scope(engine: Arc<dyn VllmEngine>) -> impl HttpServiceFactory {
    web::scope("/api/v1/vllm")
        .app_data(web::Data::new(engine))
        .service(list_instances)
        .service(launch_instance)
        .service(stop_instance)
        .service(handle_ws)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/instances",
    responses((status = 200, body = [VllmInstance]))
)]
#[get("/instances")]
async fn list_instances(engine: web::Data<Arc<dyn VllmEngine>>) -> impl Responder {
    match engine.list_instances().await {
        Ok(list) => HttpResponse::Ok().json(list),
        Err(err) => {
            tracing::error!("Failed to list vLLM instances: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

#[utoipa::path(
    post,
    path = "/instances",
    request_body = LaunchRequest,
    responses((status = 202, body = VllmInstance))
)]
#[post("/instances")]
async fn launch_instance(
    req: web::Json<LaunchRequest>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    match engine.launch_instance(req.into_inner()).await {
        Ok(instance) => HttpResponse::Accepted().json(instance),
        Err(err) => {
            tracing::error!("Failed to launch vLLM instance: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

#[utoipa::path(
    delete,
    path = "/instances/{id}",
    responses((status = 204))
)]
#[delete("/instances/{id}")]
async fn stop_instance(
    id: web::Path<String>,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> impl Responder {
    match engine.stop_instance(id.into_inner()).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(err) => {
            tracing::error!("Failed to stop vLLM instance: {}", err);
            HttpResponse::InternalServerError().body(err)
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/instances/ws",
    responses(
        (status = 101, description = "WebSocket upgraded"),
    )
)]
#[get("/instances/ws")]
async fn handle_ws(
    req: actix_web::HttpRequest,
    body: web::Payload,
    engine: web::Data<Arc<dyn VllmEngine>>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, stream) = actix_ws::handle(&req, body)?;

    let session_clone = session.clone();
    let engine_clone = engine.clone();

    actix_web::rt::spawn(async move {
        websocket_sender(session, engine_clone).await;
    });

    actix_web::rt::spawn(async move {
        websocket_receiver(stream, session_clone).await;
    });

    Ok(response)
}

async fn websocket_sender(mut session: actix_ws::Session, engine: web::Data<Arc<dyn VllmEngine>>) {
    let mut interval = actix_web::rt::time::interval(std::time::Duration::from_secs(1));

    loop {
        interval.tick().await;

        match engine.list_instances().await {
            Ok(list) => match serde_json::to_string(&list) {
                Ok(json) => {
                    if session.text(json).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("serialize vllm instances failed: {e}");
                }
            },
            Err(e) => {
                tracing::error!("list vllm instances failed: {e}");
            }
        }
    }
}

async fn websocket_receiver(mut stream: actix_ws::MessageStream, mut session: actix_ws::Session) {
    use futures_util::StreamExt;

    while let Some(item) = stream.next().await {
        match item {
            Ok(actix_ws::Message::Ping(bytes)) if session.pong(&bytes).await.is_err() => {
                break;
            }

            Ok(actix_ws::Message::Close(reason)) => {
                let _ = session.close(reason).await;
                break;
            }

            _ => {}
        }
    }
}
