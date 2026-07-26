//! Unit tests for `actix/routers/health.rs`.

use actix_web::{App, http::StatusCode, test, web};
use quench_starter::actix::routers::health::*;

#[actix_web::test]
async fn live_endpoint_reports_live() {
    let state = HealthState::live();
    let app = test::init_service(App::new().app_data(web::Data::new(state)).service(scope())).await;

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
