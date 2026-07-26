use actix_web::dev::HttpServiceFactory;
use actix_web::middleware::NormalizePath;
use actix_web::{HttpResponse, Responder, get, web};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Default)]
pub struct HealthState {
    live: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl HealthState {
    pub fn live() -> Self {
        Self {
            live: Arc::new(AtomicBool::new(true)),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }
}

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/health")
        .wrap(NormalizePath::trim())
        .service(health)
        .service(live)
        .service(ready)
}

#[get("")]
#[doc(hidden)]
pub async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/live")]
#[doc(hidden)]
pub async fn live(state: web::Data<HealthState>) -> impl Responder {
    if state.is_live() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "live" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_live" }))
    }
}

#[get("/ready")]
#[doc(hidden)]
pub async fn ready(state: web::Data<HealthState>) -> impl Responder {
    if state.is_ready() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "ready" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_ready" }))
    }
}
