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
async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/live")]
async fn live(state: web::Data<HealthState>) -> impl Responder {
    if state.is_live() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "live" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_live" }))
    }
}

#[get("/ready")]
async fn ready(state: web::Data<HealthState>) -> impl Responder {
    if state.is_ready() {
        HttpResponse::Ok().json(serde_json::json!({ "status": "ready" }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({ "status": "not_ready" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test};

    #[actix_web::test]
    async fn live_endpoint_reports_live() {
        let state = HealthState::live();
        let app =
            test::init_service(App::new().app_data(web::Data::new(state)).service(scope())).await;

        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/live").to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn ready_endpoint_waits_for_initialization() {
        let state = HealthState::live();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .service(scope()),
        )
        .await;

        let initializing_response = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request(),
        )
        .await;
        assert_eq!(
            initializing_response.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );

        state.mark_ready();

        let ready_response = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request(),
        )
        .await;
        assert_eq!(ready_response.status(), StatusCode::OK);
    }
}
