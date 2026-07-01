use actix_web::{HttpResponse, get, web};
use serde_json::json;

/// Get metrics endpoint
#[get("/metrics")]
async fn get_metrics() -> HttpResponse {
    // This is a placeholder - in a real implementation, you would collect
    // metrics from all services and return them in Prometheus format
    let metrics_text = r#"# HELP service_up Service is up and running
# TYPE service_up gauge
service_up{} 1
"#;

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(metrics_text)
}

/// Health check endpoint
#[get("/health")]
async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "healthy"
    }))
}

/// Ready check endpoint (for Kubernetes)
#[get("/health/ready")]
async fn ready() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "ready"
    }))
}

/// Live check endpoint (for Kubernetes)
#[get("/health/live")]
async fn live() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "status": "alive"
    }))
}

pub fn scope() -> actix_web::Scope {
    web::scope("")
        .service(get_metrics)
        .service(health)
        .service(ready)
        .service(live)
}
